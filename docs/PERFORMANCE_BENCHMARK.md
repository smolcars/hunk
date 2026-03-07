# Performance Benchmark Baseline

Date: 2026-03-07
Build: `cargo test --release -p hunk-desktop --test performance_harness large_diff_perf_harness -- --ignored --nocapture`
Harness entrypoint: [performance_harness.rs](/Volumes/hulk/dev/projects/hunk/crates/hunk-desktop/tests/performance_harness.rs)
Fixture generator: [create_large_diff_repo.sh](/Volumes/hulk/dev/projects/hunk/scripts/create_large_diff_repo.sh)
Wrapper: [run_perf_harness.sh](/Volumes/hulk/dev/projects/hunk/scripts/run_perf_harness.sh)

## Scenarios

| Scenario | Fixture shape | changed_files | total_code_rows | ttfd_ms | selected_file_latency_ms | full_stream_ms | scroll_fps_p95 |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `default` | 50 files x 10,000 changed lines | 52 | 500,000 | 5.43 | 20.14 | 195.67 | 319.10 |
| `many-files-small-patches` | 400 files x 80 lines with sparse edits | 402 | 4,400 | 1.90 | 5.56 | 32.57 | 86021.51 |
| `rename-heavy` | 150 moved files x 400 lines with sparse edits | 302 | 120,000 | 9.64 | 17.92 | 43.19 | 674.76 |
| `binary-heavy` | 60 mixed text/binary files | 60 | 45,000 | 2.09 | 16.95 | 22.80 | 0.00 |
| `ignored-tree-pressure` | 50 files x 2,000 lines plus 2,400 ignored files | 52 | 17,350 | 2.24 | 15.89 | 32.85 | 324.86 |

## Notes

- `scroll_fps_p95` is a synthetic scroll/render proxy from the harness, not a literal on-screen UI FPS reading.
- Small fixtures can saturate the proxy and produce very large values. Treat it as a coarse regression detector, not a user-facing performance number.
- `binary-heavy` intentionally skips scroll-threshold enforcement because a binary-first selected file may not produce code rows for the scroll simulation.
- The current large-scale `rename-heavy` scenario behaves more like path churn in the unstaged worktree benchmark than a perfectly collapsed rename-only view. It is still useful for measuring path-change pressure, but it should not be read as a pure rename benchmark.

## Conclusion

The current Git refresh path is in good shape:

- the default large-diff path remains well inside the current thresholds
- background read-only refreshes now use the lightweight Git snapshot path
- no-op/manual refreshes no longer trigger unnecessary full line-stat recomputation

Based on this sweep, no Phase 4 product-path optimization is required right now.
