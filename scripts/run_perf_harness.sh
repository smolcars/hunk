#!/usr/bin/env bash
set -euo pipefail

repo_dir=""
scenario="default"
lang="ts"
files=50
lines=10000
files_explicit=0
lines_explicit=0
enforce=1
max_ttfd_ms=300
max_selected_file_ms=800
min_scroll_fps=115
scroll_frames=240
viewport_rows=84
scroll_step_rows=24
prefetch_radius_rows=120
scroll_row_budget=20000
build_profile="--release"

usage() {
    cat <<'USAGE'
Run Hunk's performance harness and enforce perf thresholds.

Usage:
  ./scripts/run_perf_harness.sh [options]

Options:
  --repo <path>                   Use an existing Git repo fixture
  --scenario <name>               Fixture scenario: default | many-files-small-patches | rename-heavy | binary-heavy | ignored-tree-pressure
  --lang <txt|js|ts>              Fixture language (default: ts)
  --files <count>                 Fixture changed file count (default: 50)
  --lines <count>                 Changed lines per file (default: 10000)
  --no-gate                       Collect metrics without failing on thresholds
  --max-ttfd-ms <ms>              Threshold for TTFD metric (default: 300)
  --max-selected-ms <ms>          Threshold for selected-file latency (default: 800)
  --min-scroll-fps <fps>          Threshold for scroll fps p95 proxy (default: 115)
  --scroll-frames <count>         Number of simulated scroll frames (default: 240)
  --viewport-rows <count>         Rows visible per simulated frame (default: 84)
  --scroll-step-rows <count>      Rows advanced per simulated frame (default: 24)
  --prefetch-radius-rows <count>  Simulated prefetch radius rows (default: 120)
  --scroll-row-budget <count>     Max code rows sampled for scroll sim (default: 20000)
  --debug                         Use debug test build (default is --release)
  -h, --help                      Show this help

Examples:
  ./scripts/run_perf_harness.sh
  ./scripts/run_perf_harness.sh --scenario many-files-small-patches --no-gate
  ./scripts/run_perf_harness.sh --lang txt --no-gate
  ./scripts/run_perf_harness.sh --repo /tmp/hunk-large-diff-repo --min-scroll-fps 100
USAGE
}

is_positive_integer() {
    [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

is_positive_number() {
    [[ "$1" =~ ^([0-9]+([.][0-9]+)?|[.][0-9]+)$ ]] || return 1
    awk -v value="$1" 'BEGIN { exit !(value > 0) }'
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --repo)
            [[ $# -ge 2 ]] || { echo "Missing value for --repo" >&2; exit 1; }
            repo_dir="$2"
            shift 2
            ;;
        --scenario)
            [[ $# -ge 2 ]] || { echo "Missing value for --scenario" >&2; exit 1; }
            scenario="$2"
            shift 2
            ;;
        --lang)
            [[ $# -ge 2 ]] || { echo "Missing value for --lang" >&2; exit 1; }
            lang="$2"
            shift 2
            ;;
        --files)
            [[ $# -ge 2 ]] || { echo "Missing value for --files" >&2; exit 1; }
            files="$2"
            files_explicit=1
            shift 2
            ;;
        --lines)
            [[ $# -ge 2 ]] || { echo "Missing value for --lines" >&2; exit 1; }
            lines="$2"
            lines_explicit=1
            shift 2
            ;;
        --no-gate)
            enforce=0
            shift
            ;;
        --max-ttfd-ms)
            [[ $# -ge 2 ]] || { echo "Missing value for --max-ttfd-ms" >&2; exit 1; }
            max_ttfd_ms="$2"
            shift 2
            ;;
        --max-selected-ms)
            [[ $# -ge 2 ]] || { echo "Missing value for --max-selected-ms" >&2; exit 1; }
            max_selected_file_ms="$2"
            shift 2
            ;;
        --min-scroll-fps)
            [[ $# -ge 2 ]] || { echo "Missing value for --min-scroll-fps" >&2; exit 1; }
            min_scroll_fps="$2"
            shift 2
            ;;
        --scroll-frames)
            [[ $# -ge 2 ]] || { echo "Missing value for --scroll-frames" >&2; exit 1; }
            scroll_frames="$2"
            shift 2
            ;;
        --viewport-rows)
            [[ $# -ge 2 ]] || { echo "Missing value for --viewport-rows" >&2; exit 1; }
            viewport_rows="$2"
            shift 2
            ;;
        --scroll-step-rows)
            [[ $# -ge 2 ]] || { echo "Missing value for --scroll-step-rows" >&2; exit 1; }
            scroll_step_rows="$2"
            shift 2
            ;;
        --prefetch-radius-rows)
            [[ $# -ge 2 ]] || { echo "Missing value for --prefetch-radius-rows" >&2; exit 1; }
            prefetch_radius_rows="$2"
            shift 2
            ;;
        --scroll-row-budget)
            [[ $# -ge 2 ]] || { echo "Missing value for --scroll-row-budget" >&2; exit 1; }
            scroll_row_budget="$2"
            shift 2
            ;;
        --debug)
            build_profile=""
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

case "$scenario" in
    large-diff)
        scenario="default"
        ;;
    default|many-files-small-patches|rename-heavy|binary-heavy|ignored-tree-pressure)
        ;;
    *)
        echo "--scenario must be one of: default, many-files-small-patches, rename-heavy, binary-heavy, ignored-tree-pressure" >&2
        exit 1
        ;;
esac

apply_scenario_defaults() {
    case "$scenario" in
        many-files-small-patches)
            [[ "$files_explicit" -eq 1 ]] || files=400
            [[ "$lines_explicit" -eq 1 ]] || lines=80
            ;;
        rename-heavy)
            [[ "$files_explicit" -eq 1 ]] || files=150
            [[ "$lines_explicit" -eq 1 ]] || lines=400
            ;;
        binary-heavy)
            [[ "$files_explicit" -eq 1 ]] || files=60
            [[ "$lines_explicit" -eq 1 ]] || lines=1500
            ;;
        ignored-tree-pressure)
            [[ "$files_explicit" -eq 1 ]] || files=50
            [[ "$lines_explicit" -eq 1 ]] || lines=2000
            ;;
    esac
}

apply_scenario_defaults

case "$lang" in
    txt|js|ts) ;;
    *)
        echo "--lang must be one of: txt, js, ts" >&2
        exit 1
        ;;
esac

is_positive_integer "$files" || { echo "--files must be a positive integer" >&2; exit 1; }
is_positive_integer "$lines" || { echo "--lines must be a positive integer" >&2; exit 1; }
is_positive_integer "$scroll_frames" || { echo "--scroll-frames must be a positive integer" >&2; exit 1; }
is_positive_integer "$viewport_rows" || { echo "--viewport-rows must be a positive integer" >&2; exit 1; }
is_positive_integer "$scroll_step_rows" || { echo "--scroll-step-rows must be a positive integer" >&2; exit 1; }
is_positive_integer "$scroll_row_budget" || { echo "--scroll-row-budget must be a positive integer" >&2; exit 1; }
[[ "$prefetch_radius_rows" =~ ^[0-9]+$ ]] || { echo "--prefetch-radius-rows must be a non-negative integer" >&2; exit 1; }
is_positive_number "$max_ttfd_ms" || { echo "--max-ttfd-ms must be a positive number" >&2; exit 1; }
is_positive_number "$max_selected_file_ms" || { echo "--max-selected-ms must be a positive number" >&2; exit 1; }
is_positive_number "$min_scroll_fps" || { echo "--min-scroll-fps must be a positive number" >&2; exit 1; }

if [[ -z "$repo_dir" ]]; then
    repo_dir="/tmp/hunk-perf-fixture-${scenario}-${lang}-${files}f-${lines}l-$(date +%s)-$$"
    ./scripts/create_large_diff_repo.sh \
        --scenario "$scenario" \
        --dir "$repo_dir" \
        --files "$files" \
        --lines "$lines" \
        --lang "$lang" \
        --force
fi

echo "Running performance harness"
echo "  repo: $repo_dir"
echo "  scenario: $scenario"
echo "  gate: $enforce"
echo "  thresholds: ttfd<=${max_ttfd_ms}ms selected<=${max_selected_file_ms}ms scroll_p95>=${min_scroll_fps}fps"

HUNK_PERF_REPO="$repo_dir" \
HUNK_PERF_SCENARIO="$scenario" \
HUNK_PERF_LANG="$lang" \
HUNK_PERF_FILES="$files" \
HUNK_PERF_LINES="$lines" \
HUNK_PERF_ENFORCE="$enforce" \
HUNK_PERF_MAX_TTFD_MS="$max_ttfd_ms" \
HUNK_PERF_MAX_SELECTED_FILE_MS="$max_selected_file_ms" \
HUNK_PERF_MIN_SCROLL_FPS="$min_scroll_fps" \
HUNK_PERF_SCROLL_FRAMES="$scroll_frames" \
HUNK_PERF_SCROLL_VIEWPORT_ROWS="$viewport_rows" \
HUNK_PERF_SCROLL_STEP_ROWS="$scroll_step_rows" \
HUNK_PERF_SCROLL_PREFETCH_RADIUS_ROWS="$prefetch_radius_rows" \
HUNK_PERF_SCROLL_ROW_BUDGET="$scroll_row_budget" \
cargo test $build_profile -p hunk-desktop --test performance_harness large_diff_perf_harness -- --ignored --nocapture
