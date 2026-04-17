#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
WORKTREE_QUERY="${1:-}"

if [[ -z "$WORKTREE_QUERY" ]]; then
  printf 'usage: %s <worktree-name-or-path>\n' "$(basename "$0")" >&2
  exit 1
fi

mapfile -t MATCHES < <(
  git -C "$ROOT_DIR" worktree list --porcelain |
    awk '/^worktree /{sub(/^worktree /, ""); print}' |
    while IFS= read -r WORKTREE; do
      if [[ "$WORKTREE" == "$WORKTREE_QUERY" || "$(basename "$WORKTREE")" == "$WORKTREE_QUERY" ]]; then
        printf '%s\n' "$WORKTREE"
      fi
    done
)

if [[ "${#MATCHES[@]}" -eq 0 ]]; then
  printf 'unknown worktree: %s\n' "$WORKTREE_QUERY" >&2
  exit 1
fi

if [[ "${#MATCHES[@]}" -gt 1 ]]; then
  printf 'ambiguous worktree "%s":\n' "$WORKTREE_QUERY" >&2
  printf '  %s\n' "${MATCHES[@]}" >&2
  exit 1
fi

WORKTREE="${MATCHES[0]}"
printf 'Building %s with its default Cargo target directory\n' "$WORKTREE"

if [[ "$(uname -s)" == "Darwin" ]]; then
  (
    cd "$WORKTREE"
    cargo build -p hunk-desktop
  )
else
  (
    cd "$WORKTREE"
    cargo build -p hunk-desktop
  )
fi
