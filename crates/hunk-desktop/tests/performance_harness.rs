#[path = "../src/app/highlight.rs"]
mod highlight;

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use hunk_domain::diff::{DiffCellKind, DiffRowKind, parse_patch_side_by_side};
use hunk_jj::jj::{ChangedFile, load_patches_for_files, load_snapshot};

const DEFAULT_LANG: &str = "ts";
const DEFAULT_FILES: usize = 50;
const DEFAULT_LINES: usize = 10_000;
const DEFAULT_MAX_TTFD_MS: f64 = 300.0;
const DEFAULT_MAX_SELECTED_FILE_MS: f64 = 800.0;
const DEFAULT_MIN_SCROLL_FPS: f64 = 115.0;
const DEFAULT_SCROLL_VIEWPORT_ROWS: usize = 84;
const DEFAULT_SCROLL_STEP_ROWS: usize = 24;
const DEFAULT_SCROLL_PREFETCH_RADIUS_ROWS: usize = 120;
const DEFAULT_SCROLL_FRAMES: usize = 240;
const DEFAULT_SCROLL_ROW_BUDGET: usize = 20_000;
const DETAILED_SEGMENT_MAX_CHANGED_LINES: u64 = 8_000;

#[derive(Debug, Clone, Copy)]
struct PerfThresholds {
    max_ttfd_ms: f64,
    max_selected_file_ms: f64,
    min_scroll_fps: f64,
}

#[derive(Debug, Clone)]
struct PerfHarnessConfig {
    repo: Option<PathBuf>,
    lang: String,
    files: usize,
    lines: usize,
    enforce: bool,
    thresholds: PerfThresholds,
    viewport_rows: usize,
    scroll_step_rows: usize,
    prefetch_radius_rows: usize,
    scroll_frames: usize,
    scroll_row_budget: usize,
}

#[derive(Debug, Clone)]
struct ScrollSampleRow {
    file_path: String,
    left_text: String,
    right_text: String,
    left_kind: DiffCellKind,
    right_kind: DiffCellKind,
    use_detailed_segments: bool,
}

#[derive(Debug, Clone, Copy)]
struct PerfMetrics {
    changed_files: usize,
    total_core_rows: usize,
    total_code_rows: usize,
    scroll_sample_rows: usize,
    ttfd_ms: f64,
    selected_file_latency_ms: f64,
    full_stream_ms: f64,
    scroll_fps_avg: f64,
    scroll_fps_p95: f64,
}

impl PerfHarnessConfig {
    fn from_env() -> Self {
        let repo = env::var_os("HUNK_PERF_REPO").map(PathBuf::from);
        let lang = env::var("HUNK_PERF_LANG").unwrap_or_else(|_| DEFAULT_LANG.to_string());
        let files = env_usize("HUNK_PERF_FILES", DEFAULT_FILES).max(1);
        let lines = env_usize("HUNK_PERF_LINES", DEFAULT_LINES).max(1);
        let enforce = env_bool("HUNK_PERF_ENFORCE", true);
        let thresholds = PerfThresholds {
            max_ttfd_ms: env_f64("HUNK_PERF_MAX_TTFD_MS", DEFAULT_MAX_TTFD_MS),
            max_selected_file_ms: env_f64(
                "HUNK_PERF_MAX_SELECTED_FILE_MS",
                DEFAULT_MAX_SELECTED_FILE_MS,
            ),
            min_scroll_fps: env_f64("HUNK_PERF_MIN_SCROLL_FPS", DEFAULT_MIN_SCROLL_FPS),
        };
        let viewport_rows = env_usize(
            "HUNK_PERF_SCROLL_VIEWPORT_ROWS",
            DEFAULT_SCROLL_VIEWPORT_ROWS,
        )
        .max(1);
        let scroll_step_rows =
            env_usize("HUNK_PERF_SCROLL_STEP_ROWS", DEFAULT_SCROLL_STEP_ROWS).max(1);
        let prefetch_radius_rows = env_usize(
            "HUNK_PERF_SCROLL_PREFETCH_RADIUS_ROWS",
            DEFAULT_SCROLL_PREFETCH_RADIUS_ROWS,
        );
        let scroll_frames = env_usize("HUNK_PERF_SCROLL_FRAMES", DEFAULT_SCROLL_FRAMES).max(1);
        let scroll_row_budget =
            env_usize("HUNK_PERF_SCROLL_ROW_BUDGET", DEFAULT_SCROLL_ROW_BUDGET).max(1);

        Self {
            repo,
            lang,
            files,
            lines,
            enforce,
            thresholds,
            viewport_rows,
            scroll_step_rows,
            prefetch_radius_rows,
            scroll_frames,
            scroll_row_budget,
        }
    }
}

#[test]
#[ignore = "Runs a large-diff benchmark harness and enforces perf thresholds when configured."]
fn large_diff_perf_harness() -> Result<()> {
    let cfg = PerfHarnessConfig::from_env();
    let repo_root = prepare_repo(&cfg)?;
    let metrics = run_perf_harness(&repo_root, &cfg)?;

    println!("PERF_METRIC changed_files={}", metrics.changed_files);
    println!("PERF_METRIC total_core_rows={}", metrics.total_core_rows);
    println!("PERF_METRIC total_code_rows={}", metrics.total_code_rows);
    println!(
        "PERF_METRIC scroll_sample_rows={}",
        metrics.scroll_sample_rows
    );
    println!("PERF_METRIC ttfd_ms={:.2}", metrics.ttfd_ms);
    println!(
        "PERF_METRIC selected_file_latency_ms={:.2}",
        metrics.selected_file_latency_ms
    );
    println!("PERF_METRIC full_stream_ms={:.2}", metrics.full_stream_ms);
    println!("PERF_METRIC scroll_fps_avg={:.2}", metrics.scroll_fps_avg);
    println!("PERF_METRIC scroll_fps_p95={:.2}", metrics.scroll_fps_p95);

    if cfg.enforce {
        let failures = threshold_failures(&metrics, cfg.thresholds);
        if !failures.is_empty() {
            return Err(anyhow!(failures.join("\n")));
        }
    } else {
        println!("PERF_INFO threshold enforcement disabled (HUNK_PERF_ENFORCE=0)");
    }

    Ok(())
}

fn run_perf_harness(repo_root: &Path, cfg: &PerfHarnessConfig) -> Result<PerfMetrics> {
    let snapshot = load_snapshot(repo_root)?;
    let files = snapshot.files;
    if files.is_empty() {
        return Err(anyhow!(
            "repository has no changed files; benchmark needs a diff fixture"
        ));
    }

    let selected_file = files[0].clone();
    let (ttfd_ms, selected_file_latency_ms) =
        measure_selected_file_latency(repo_root, &selected_file, cfg)?;
    let (full_stream_ms, total_core_rows, total_code_rows, scroll_rows) =
        measure_full_stream_and_collect_scroll_rows(repo_root, &files, cfg.scroll_row_budget)?;
    if total_code_rows == 0 {
        return Err(anyhow!(
            "parsed zero code rows; fixture is invalid for perf benchmarking"
        ));
    }
    let (scroll_fps_avg, scroll_fps_p95) = measure_scroll_fps(&scroll_rows, cfg);

    Ok(PerfMetrics {
        changed_files: files.len(),
        total_core_rows,
        total_code_rows,
        scroll_sample_rows: scroll_rows.len(),
        ttfd_ms,
        selected_file_latency_ms,
        full_stream_ms,
        scroll_fps_avg,
        scroll_fps_p95,
    })
}

fn measure_selected_file_latency(
    repo_root: &Path,
    selected_file: &ChangedFile,
    cfg: &PerfHarnessConfig,
) -> Result<(f64, f64)> {
    let selected_stage_started = Instant::now();
    let selected_patch_map =
        load_patches_for_files(repo_root, std::slice::from_ref(selected_file))?;
    let patch = selected_patch_map
        .get(selected_file.path.as_str())
        .map(String::as_str)
        .unwrap_or_default();
    let rows = parse_patch_side_by_side(patch);
    let ttfd_ms = selected_stage_started.elapsed().as_secs_f64() * 1_000.0;

    let mut code_rows = Vec::new();
    let mut selected_changed_lines = 0_u64;
    for row in rows {
        if row.kind != DiffRowKind::Code {
            continue;
        }
        if row.left.kind == DiffCellKind::Removed {
            selected_changed_lines = selected_changed_lines.saturating_add(1);
        }
        if row.right.kind == DiffCellKind::Added {
            selected_changed_lines = selected_changed_lines.saturating_add(1);
        }

        if code_rows.len() < cfg.viewport_rows {
            code_rows.push(ScrollSampleRow {
                file_path: selected_file.path.clone(),
                left_text: row.left.text,
                right_text: row.right.text,
                left_kind: row.left.kind,
                right_kind: row.right.kind,
                use_detailed_segments: true,
            });
        }
    }
    if code_rows.is_empty() {
        return Err(anyhow!(
            "selected file '{}' produced zero code rows",
            selected_file.path
        ));
    }
    let use_detailed_segments = selected_changed_lines <= DETAILED_SEGMENT_MAX_CHANGED_LINES;
    for row in &mut code_rows {
        row.use_detailed_segments = use_detailed_segments;
    }

    // Include first-viewport segment build so selected-file latency reflects first useful paint cost.
    let mut cache = HashMap::new();
    for row_ix in 0..code_rows.len() {
        compute_cell_segment_count(&code_rows, &mut cache, row_ix, true);
        compute_cell_segment_count(&code_rows, &mut cache, row_ix, false);
    }

    let selected_file_latency_ms = selected_stage_started.elapsed().as_secs_f64() * 1_000.0;
    Ok((ttfd_ms, selected_file_latency_ms))
}

fn measure_full_stream_and_collect_scroll_rows(
    repo_root: &Path,
    files: &[ChangedFile],
    scroll_row_budget: usize,
) -> Result<(f64, usize, usize, Vec<ScrollSampleRow>)> {
    let full_stage_started = Instant::now();
    let patch_map = load_patches_for_files(repo_root, files)?;
    let mut total_core_rows = 0_usize;
    let mut total_code_rows = 0_usize;
    let mut scroll_rows = Vec::with_capacity(scroll_row_budget.min(4_096));

    for file in files {
        let patch = patch_map
            .get(file.path.as_str())
            .map(String::as_str)
            .unwrap_or_default();
        let mut file_changed_lines = 0_u64;
        let mut file_samples = Vec::new();
        for row in parse_patch_side_by_side(patch) {
            if matches!(
                row.kind,
                DiffRowKind::Code | DiffRowKind::HunkHeader | DiffRowKind::Empty
            ) {
                total_core_rows = total_core_rows.saturating_add(1);
            }
            if row.kind != DiffRowKind::Code {
                continue;
            }

            total_code_rows = total_code_rows.saturating_add(1);
            if row.left.kind == DiffCellKind::Removed {
                file_changed_lines = file_changed_lines.saturating_add(1);
            }
            if row.right.kind == DiffCellKind::Added {
                file_changed_lines = file_changed_lines.saturating_add(1);
            }

            if scroll_rows.len().saturating_add(file_samples.len()) >= scroll_row_budget {
                continue;
            }
            file_samples.push(ScrollSampleRow {
                file_path: file.path.clone(),
                left_text: row.left.text,
                right_text: row.right.text,
                left_kind: row.left.kind,
                right_kind: row.right.kind,
                use_detailed_segments: true,
            });
        }

        let use_detailed_segments = file_changed_lines <= DETAILED_SEGMENT_MAX_CHANGED_LINES;
        for mut sample in file_samples {
            sample.use_detailed_segments = use_detailed_segments;
            scroll_rows.push(sample);
        }
    }

    let full_stream_ms = full_stage_started.elapsed().as_secs_f64() * 1_000.0;
    Ok((
        full_stream_ms,
        total_core_rows,
        total_code_rows,
        scroll_rows,
    ))
}

fn measure_scroll_fps(rows: &[ScrollSampleRow], cfg: &PerfHarnessConfig) -> (f64, f64) {
    if rows.is_empty() {
        return (0.0, 0.0);
    }

    let mut cache: HashMap<(usize, u8), usize> = HashMap::new();
    let mut anchor = 0_usize;
    let max_anchor = rows.len().saturating_sub(1);
    let timed_started = Instant::now();
    let mut rendered_frames = 0_usize;
    let mut consumed_segments = 0_usize;
    let mut frame_fps = Vec::with_capacity(cfg.scroll_frames);

    for _ in 0..cfg.scroll_frames {
        let frame_started = Instant::now();
        let prefetch_start = anchor.saturating_sub(cfg.prefetch_radius_rows);
        let prefetch_end = anchor
            .saturating_add(cfg.prefetch_radius_rows.saturating_add(cfg.viewport_rows))
            .min(rows.len());
        for row_ix in prefetch_start..prefetch_end {
            consumed_segments = consumed_segments
                .saturating_add(compute_cell_segment_count(rows, &mut cache, row_ix, true));
            consumed_segments = consumed_segments
                .saturating_add(compute_cell_segment_count(rows, &mut cache, row_ix, false));
        }

        let visible_end = anchor.saturating_add(cfg.viewport_rows).min(rows.len());
        for row_ix in anchor..visible_end {
            consumed_segments =
                consumed_segments.saturating_add(cache.get(&(row_ix, 0)).copied().unwrap_or(0));
            consumed_segments =
                consumed_segments.saturating_add(cache.get(&(row_ix, 1)).copied().unwrap_or(0));
        }
        rendered_frames = rendered_frames.saturating_add(1);

        if anchor.saturating_add(cfg.scroll_step_rows) >= max_anchor {
            anchor = 0;
        } else {
            anchor = anchor.saturating_add(cfg.scroll_step_rows);
        }

        let frame_elapsed = frame_started.elapsed().as_secs_f64();
        if frame_elapsed > f64::EPSILON {
            frame_fps.push(1.0 / frame_elapsed);
        }
    }

    // Keep optimizer honest around the render-loop workload.
    std::hint::black_box(consumed_segments);

    let elapsed = timed_started.elapsed().as_secs_f64();
    if elapsed <= f64::EPSILON {
        return (0.0, 0.0);
    }
    let avg_fps = rendered_frames as f64 / elapsed;
    let p95_fps = percentile_fps(&mut frame_fps, 95);
    (avg_fps, p95_fps)
}

fn compute_cell_segment_count(
    rows: &[ScrollSampleRow],
    cache: &mut HashMap<(usize, u8), usize>,
    row_ix: usize,
    left_side: bool,
) -> usize {
    let side_tag = if left_side { 0_u8 } else { 1_u8 };
    if let Some(cached) = cache.get(&(row_ix, side_tag)).copied() {
        return cached;
    }

    let row = &rows[row_ix];
    let (line, kind, peer_line, peer_kind) = if left_side {
        (
            row.left_text.as_str(),
            row.left_kind,
            row.right_text.as_str(),
            row.right_kind,
        )
    } else {
        (
            row.right_text.as_str(),
            row.right_kind,
            row.left_text.as_str(),
            row.left_kind,
        )
    };

    let segment_count = if row.use_detailed_segments {
        highlight::build_line_segments(
            Some(row.file_path.as_str()),
            line,
            kind,
            peer_line,
            peer_kind,
        )
        .len()
        .max(1)
    } else {
        highlight::build_syntax_only_line_segments(Some(row.file_path.as_str()), line)
            .len()
            .max(1)
    };
    cache.insert((row_ix, side_tag), segment_count);
    segment_count
}

fn threshold_failures(metrics: &PerfMetrics, thresholds: PerfThresholds) -> Vec<String> {
    let mut failures = Vec::new();

    if metrics.ttfd_ms > thresholds.max_ttfd_ms {
        failures.push(format!(
            "TTFD {:.2} ms exceeded threshold {:.2} ms",
            metrics.ttfd_ms, thresholds.max_ttfd_ms
        ));
    }
    if metrics.selected_file_latency_ms > thresholds.max_selected_file_ms {
        failures.push(format!(
            "Selected-file latency {:.2} ms exceeded threshold {:.2} ms",
            metrics.selected_file_latency_ms, thresholds.max_selected_file_ms
        ));
    }
    if metrics.scroll_fps_p95 < thresholds.min_scroll_fps {
        failures.push(format!(
            "Scroll FPS p95 {:.2} below threshold {:.2}",
            metrics.scroll_fps_p95, thresholds.min_scroll_fps
        ));
    }

    failures
}

fn prepare_repo(cfg: &PerfHarnessConfig) -> Result<PathBuf> {
    if let Some(repo) = &cfg.repo {
        return Ok(repo.clone());
    }

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| anyhow!("system clock before unix epoch: {err}"))?
        .as_nanos();
    let repo = env::temp_dir().join(format!("hunk-perf-harness-{unique}"));

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("create_large_diff_repo.sh");
    let status = Command::new("bash")
        .arg(script_path)
        .arg("--dir")
        .arg(&repo)
        .arg("--files")
        .arg(cfg.files.to_string())
        .arg("--lines")
        .arg(cfg.lines.to_string())
        .arg("--lang")
        .arg(cfg.lang.as_str())
        .arg("--force")
        .status()
        .map_err(|err| anyhow!("failed to execute create_large_diff_repo.sh: {err}"))?;
    if !status.success() {
        return Err(anyhow!("create_large_diff_repo.sh exited with {status}"));
    }

    Ok(repo)
}

fn env_bool(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn env_f64(key: &str, default: f64) -> f64 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(default)
}

fn percentile_fps(samples: &mut [f64], percentile: usize) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p = percentile.clamp(1, 100);
    let rank = (samples.len().saturating_mul(p).div_ceil(100)).saturating_sub(1);
    samples[rank.min(samples.len().saturating_sub(1))]
}
