#!/usr/bin/env bash
set -euo pipefail

DEFAULT_REPO_DIR="/tmp/hunk-large-diff-repo"
repo_dir="$DEFAULT_REPO_DIR"
file_count=1
lines_per_file=25000
force=0
branch_name="main"
language="txt"
scenario="default"
files_explicit=0
lines_explicit=0
run_seed="$(od -An -N4 -tu4 /dev/urandom | tr -d '[:space:]')"
if [[ -z "$run_seed" ]]; then
    run_seed="$(date +%s)"
fi

usage() {
    cat <<'USAGE'
Create a synthetic Git repository with a very large text diff for Hunk performance testing.

Usage:
  ./scripts/create_large_diff_repo.sh [options]

Options:
  --dir <path>      Destination repo path (default: /tmp/hunk-large-diff-repo)
  --files <count>   Number of files with large diffs (default: 1)
  --lines <count>   Changed lines per file (default: 25000)
  --lang <kind>     Diff content kind: txt | js | ts (default: txt)
  --scenario <kind> Fixture shape: default | many-files-small-patches | rename-heavy | binary-heavy | ignored-tree-pressure
  --force           Replace destination directory if it already exists
  -h, --help        Show this help message

Examples:
  ./scripts/create_large_diff_repo.sh
  ./scripts/create_large_diff_repo.sh --scenario rename-heavy --force
  ./scripts/create_large_diff_repo.sh --files 4 --lines 6000 --lang ts --force
  ./scripts/create_large_diff_repo.sh --dir /tmp/hunk-stress --lines 30000 --force
USAGE
}

is_positive_integer() {
    [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dir)
            [[ $# -ge 2 ]] || {
                echo "Missing value for --dir" >&2
                exit 1
            }
            repo_dir="$2"
            shift 2
            ;;
        --files)
            [[ $# -ge 2 ]] || {
                echo "Missing value for --files" >&2
                exit 1
            }
            file_count="$2"
            files_explicit=1
            shift 2
            ;;
        --lines)
            [[ $# -ge 2 ]] || {
                echo "Missing value for --lines" >&2
                exit 1
            }
            lines_per_file="$2"
            lines_explicit=1
            shift 2
            ;;
        --force)
            force=1
            shift
            ;;
        --lang)
            [[ $# -ge 2 ]] || {
                echo "Missing value for --lang" >&2
                exit 1
            }
            language="$2"
            shift 2
            ;;
        --scenario)
            [[ $# -ge 2 ]] || {
                echo "Missing value for --scenario" >&2
                exit 1
            }
            scenario="$2"
            shift 2
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

case "$language" in
    txt|js|ts) ;;
    *)
        echo "--lang must be one of: txt, js, ts" >&2
        exit 1
        ;;
esac

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
            [[ "$files_explicit" -eq 1 ]] || file_count=400
            [[ "$lines_explicit" -eq 1 ]] || lines_per_file=80
            ;;
        rename-heavy)
            [[ "$files_explicit" -eq 1 ]] || file_count=150
            [[ "$lines_explicit" -eq 1 ]] || lines_per_file=400
            ;;
        binary-heavy)
            [[ "$files_explicit" -eq 1 ]] || file_count=60
            [[ "$lines_explicit" -eq 1 ]] || lines_per_file=1500
            ;;
        ignored-tree-pressure)
            [[ "$files_explicit" -eq 1 ]] || file_count=50
            [[ "$lines_explicit" -eq 1 ]] || lines_per_file=2000
            ;;
    esac
}

apply_scenario_defaults

is_positive_integer "$file_count" || {
    echo "--files must be a positive integer" >&2
    exit 1
}

is_positive_integer "$lines_per_file" || {
    echo "--lines must be a positive integer" >&2
    exit 1
}

if [[ -e "$repo_dir" ]]; then
    if [[ "$force" -eq 1 ]]; then
        rm -rf "$repo_dir"
    else
        echo "Destination already exists: $repo_dir" >&2
        echo "Use --force to replace it." >&2
        exit 1
    fi
fi

mkdir -p "$repo_dir"
git init "$repo_dir" >/dev/null 2>&1
git -C "$repo_dir" checkout -b "$branch_name" >/dev/null 2>&1
git -C "$repo_dir" config user.name "Hunk Perf"
git -C "$repo_dir" config user.email "perf@hunk.invalid"

create_file_contents() {
    local phase="$1"
    local output_path="$2"
    local file_index="$3"
    local file_seed="$4"
    awk -v lines="$lines_per_file" -v phase="$phase" -v lang="$language" -v file_idx="$file_index" -v seed="$file_seed" -v scenario="$scenario" '
        function mix(line_no, salt, value) {
            value = seed + (line_no * 48271) + (file_idx * 69621) + (salt * 17123)
            value = value % 2147483647
            if (value < 0) {
                value += 2147483647
            }
            return value
        }

        BEGIN {
            for (i = 1; i <= lines; i++) {
                a = mix(i, 1) % 10007
                b = mix(i, 2) % 8191
                c = mix(i, 3) % 4099
                variant = mix(i, 4) % 3

                if (scenario == "rename-heavy" || scenario == "many-files-small-patches" || scenario == "ignored-tree-pressure") {
                    changed = (phase == "after" && (i % 40) == 0)
                    if (lang == "ts") {
                        if (!changed) {
                            if (variant == 0) {
                                printf "export const metric_%03d_%06d: number = (%d + %d);\n", file_idx, i, a, b
                            } else if (variant == 1) {
                                printf "export const metric_%03d_%06d: number = (%d - %d);\n", file_idx, i, a + b, c % 97
                            } else {
                                printf "export const metric_%03d_%06d: number = (%d * %d);\n", file_idx, i, (a % 97) + 3, (b % 13) + 2
                            }
                        } else {
                            printf "export const metric_%03d_%06d: number = (%d + %d) / %d;\n", file_idx, i, a + c, b + 7, (c % 9) + 1
                        }
                    } else if (lang == "js") {
                        if (!changed) {
                            if (variant == 0) {
                                printf "export const metric_%03d_%06d = (%d + %d);\n", file_idx, i, a, b
                            } else if (variant == 1) {
                                printf "export const metric_%03d_%06d = (%d - %d);\n", file_idx, i, a + b, c % 97
                            } else {
                                printf "export const metric_%03d_%06d = (%d * %d);\n", file_idx, i, (a % 97) + 3, (b % 13) + 2
                            }
                        } else {
                            printf "export const metric_%03d_%06d = (%d + %d) / %d;\n", file_idx, i, a + c, b + 7, (c % 9) + 1
                        }
                    } else {
                        if (!changed) {
                            printf "stable file %03d line %06d payload %05d %05d %05d mostly unchanged content for rename stress\n", file_idx, i, a, b, c
                        } else {
                            printf "edited file %03d line %06d payload %05d %05d %05d sparse changes for rename stress\n", file_idx, i, (a + b) % 10007, (b + c) % 8191, (c + a) % 4099
                        }
                    }
                } else if (lang == "ts") {
                    if (phase == "before") {
                        if (variant == 0) {
                            printf "export const metric_%03d_%06d: number = (%d + %d);\n", file_idx, i, a, b
                        } else if (variant == 1) {
                            printf "export const metric_%03d_%06d: number = (%d - %d);\n", file_idx, i, a + b, c % 97
                        } else {
                            printf "export const metric_%03d_%06d: number = (%d * %d);\n", file_idx, i, (a % 97) + 3, (b % 13) + 2
                        }
                    } else {
                        if (variant == 0) {
                            printf "export const metric_%03d_%06d: number = (%d * %d) - %d;\n", file_idx, i, (a % 89) + 11, (b % 11) + 2, c % 53
                        } else if (variant == 1) {
                            printf "export const metric_%03d_%06d: number = (%d + %d) / %d;\n", file_idx, i, a + c, b + 7, (c % 9) + 1
                        } else {
                            printf "export const metric_%03d_%06d: number = (%d ^ %d) + %d;\n", file_idx, i, a % 2048, b % 1024, c % 61
                        }
                    }
                } else if (lang == "js") {
                    if (phase == "before") {
                        if (variant == 0) {
                            printf "export const metric_%03d_%06d = (%d + %d);\n", file_idx, i, a, b
                        } else if (variant == 1) {
                            printf "export const metric_%03d_%06d = (%d - %d);\n", file_idx, i, a + b, c % 97
                        } else {
                            printf "export const metric_%03d_%06d = (%d * %d);\n", file_idx, i, (a % 97) + 3, (b % 13) + 2
                        }
                    } else {
                        if (variant == 0) {
                            printf "export const metric_%03d_%06d = (%d * %d) - %d;\n", file_idx, i, (a % 89) + 11, (b % 11) + 2, c % 53
                        } else if (variant == 1) {
                            printf "export const metric_%03d_%06d = (%d + %d) / %d;\n", file_idx, i, a + c, b + 7, (c % 9) + 1
                        } else {
                            printf "export const metric_%03d_%06d = (%d ^ %d) + %d;\n", file_idx, i, a % 2048, b % 1024, c % 61
                        }
                    }
                } else {
                    if (phase == "before") {
                        printf "before file %03d line %06d payload %05d %05d %05d steady text for renderer stress\n", file_idx, i, a, b, c
                    } else {
                        printf "after file %03d line %06d payload %05d %05d %05d steady text for renderer stress\n", file_idx, i, (a + b) % 10007, (b + c) % 8191, (c + a) % 4099
                    }
                }
            }
        }
    ' >"$output_path"
}

compute_file_seed() {
    local index="$1"
    echo $(((run_seed + (index * 7919)) % 2147483647))
}

extension_for_language() {
    case "$language" in
        ts) echo "ts" ;;
        js) echo "js" ;;
        *) echo "txt" ;;
    esac
}

file_extension="$(extension_for_language)"

text_file_count_for_scenario() {
    if [[ "$scenario" == "binary-heavy" ]]; then
        if [[ "$file_count" -eq 1 ]]; then
            echo 1
        else
            echo $(((file_count + 1) / 2))
        fi
        return
    fi

    echo "$file_count"
}

binary_file_count_for_scenario() {
    if [[ "$scenario" == "binary-heavy" ]]; then
        local text_count
        text_count="$(text_file_count_for_scenario)"
        echo $((file_count - text_count))
        return
    fi

    echo 0
}

text_file_path() {
    local file_index="$1"
    local phase="$2"
    local prefix="stress"

    if [[ "$scenario" == "rename-heavy" ]]; then
        if [[ "$phase" == "before" ]]; then
            prefix="stress/original"
        else
            prefix="stress/renamed"
        fi
    fi

    printf "%s/%s/file_%03d.%s" "$repo_dir" "$prefix" "$file_index" "$file_extension"
}

binary_file_path() {
    local file_index="$1"
    printf "%s/stress/binary_%03d.bin" "$repo_dir" "$file_index"
}

create_binary_file() {
    local phase="$1"
    local output_path="$2"
    local file_index="$3"
    mkdir -p "$(dirname "$output_path")"
    : >"$output_path"
    dd if=/dev/zero of="$output_path" bs=1024 count=4 status=none
    if [[ "$phase" == "before" ]]; then
        printf "binary-before-%03d-%s" "$file_index" "$run_seed" >>"$output_path"
    else
        printf "binary-after-%03d-%s" "$file_index" "$run_seed" >>"$output_path"
    fi
}

create_ignored_tree_pressure() {
    mkdir -p "$repo_dir/ignored-cache"
    for dir_index in $(seq 1 120); do
        dir_path="$repo_dir/ignored-cache/dir_$(printf '%03d' "$dir_index")"
        mkdir -p "$dir_path"
        for file_index in $(seq 1 20); do
            ignored_path="$dir_path/blob_$(printf '%03d' "$file_index").ignorelog"
            printf "ignored %03d %03d %s\n" "$dir_index" "$file_index" "$run_seed" >"$ignored_path"
        done
    done
}

text_file_count="$(text_file_count_for_scenario)"
binary_file_count="$(binary_file_count_for_scenario)"

if [[ "$scenario" == "ignored-tree-pressure" ]]; then
    printf "ignored-cache/\n*.ignorelog\n" >"$repo_dir/.gitignore"
fi

for i in $(seq 1 "$text_file_count"); do
    file_path="$(text_file_path "$i" "before")"
    file_seed="$(compute_file_seed "$i")"
    mkdir -p "$(dirname "$file_path")"
    create_file_contents "before" "$file_path" "$i" "$file_seed"
done

for i in $(seq 1 "$binary_file_count"); do
    file_path="$(binary_file_path "$i")"
    create_binary_file "before" "$file_path" "$i"
done

git -C "$repo_dir" add .
git -C "$repo_dir" commit -m "Baseline for Hunk perf fixture" >/dev/null 2>&1

if [[ "$scenario" == "ignored-tree-pressure" ]]; then
    create_ignored_tree_pressure
fi

for i in $(seq 1 "$text_file_count"); do
    before_path="$(text_file_path "$i" "before")"
    file_path="$(text_file_path "$i" "after")"
    file_seed="$(compute_file_seed "$i")"
    if [[ "$before_path" != "$file_path" ]]; then
        mkdir -p "$(dirname "$file_path")"
        mv "$before_path" "$file_path"
    fi
    create_file_contents "after" "$file_path" "$i" "$file_seed"
done

for i in $(seq 1 "$binary_file_count"); do
    file_path="$(binary_file_path "$i")"
    create_binary_file "after" "$file_path" "$i"
done

total_changed_rows=$((text_file_count * lines_per_file))
if [[ "$scenario" == "rename-heavy" || "$scenario" == "many-files-small-patches" || "$scenario" == "ignored-tree-pressure" ]]; then
    total_changed_rows=$((text_file_count * ((lines_per_file + 39) / 40)))
fi
total_changed_lines=$((total_changed_rows * 2))
total_changed_files=$((text_file_count + binary_file_count))

printf "Created Git repo: %s\n" "$repo_dir"
printf "Scenario: %s\n" "$scenario"
printf "Files changed: %d\n" "$total_changed_files"
printf "Text files changed: %d\n" "$text_file_count"
if [[ "$binary_file_count" -gt 0 ]]; then
    printf "Binary files changed: %d\n" "$binary_file_count"
fi
printf "Per-file paired rows in Hunk: %d\n" "$lines_per_file"
printf "Total paired rows in Hunk: %d\n" "$total_changed_rows"
printf "Total changed lines in patch (+/-): %d\n" "$total_changed_lines"
if [[ "$scenario" == "ignored-tree-pressure" ]]; then
    printf "Ignored files created: %d\n" $((120 * 20))
fi
printf "Language mode: %s (.%s)\n" "$language" "$file_extension"
printf "Randomization seed: %s\n" "$run_seed"
printf "Active branch: %s\n" "$branch_name"
printf "\nOpen this folder in Hunk and watch the FPS badge while scrolling.\n"
