#!/usr/bin/env bash
set -euo pipefail

DEFAULT_REPO_DIR="/tmp/hunk-large-diff-repo"
repo_dir="$DEFAULT_REPO_DIR"
file_count=1
lines_per_file=25000
force=0
branch_name="main"
language="txt"
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
  --force           Replace destination directory if it already exists
  -h, --help        Show this help message

Examples:
  ./scripts/create_large_diff_repo.sh
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
            shift 2
            ;;
        --lines)
            [[ $# -ge 2 ]] || {
                echo "Missing value for --lines" >&2
                exit 1
            }
            lines_per_file="$2"
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

is_positive_integer "$file_count" || {
    echo "--files must be a positive integer" >&2
    exit 1
}

is_positive_integer "$lines_per_file" || {
    echo "--lines must be a positive integer" >&2
    exit 1
}

case "$language" in
    txt|js|ts) ;;
    *)
        echo "--lang must be one of: txt, js, ts" >&2
        exit 1
        ;;
esac

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
    awk -v lines="$lines_per_file" -v phase="$phase" -v lang="$language" -v file_idx="$file_index" -v seed="$file_seed" '
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

                if (lang == "ts") {
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

for i in $(seq 1 "$file_count"); do
    file_path="$repo_dir/stress/file_$(printf '%03d' "$i").$file_extension"
    file_seed="$(compute_file_seed "$i")"
    mkdir -p "$(dirname "$file_path")"
    create_file_contents "before" "$file_path" "$i" "$file_seed"
done

git -C "$repo_dir" add .
git -C "$repo_dir" commit -m "Baseline for Hunk large-diff stress test" >/dev/null 2>&1

for i in $(seq 1 "$file_count"); do
    file_path="$repo_dir/stress/file_$(printf '%03d' "$i").$file_extension"
    file_seed="$(compute_file_seed "$i")"
    create_file_contents "after" "$file_path" "$i" "$file_seed"
done

total_changed_rows=$((file_count * lines_per_file))
total_changed_lines=$((total_changed_rows * 2))

printf "Created Git repo: %s\n" "$repo_dir"
printf "Files changed: %d\n" "$file_count"
printf "Per-file paired rows in Hunk: %d\n" "$lines_per_file"
printf "Total paired rows in Hunk: %d\n" "$total_changed_rows"
printf "Total changed lines in patch (+/-): %d\n" "$total_changed_lines"
printf "Language mode: %s (.%s)\n" "$language" "$file_extension"
printf "Randomization seed: %s\n" "$run_seed"
printf "Active branch: %s\n" "$branch_name"
printf "\nOpen this folder in Hunk and watch the FPS badge while scrolling.\n"
