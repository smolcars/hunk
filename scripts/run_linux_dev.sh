#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -z "${CARGO_TARGET_DIR:-}" ]]; then
  CARGO_TARGET_DIR="$("$ROOT_DIR/scripts/resolve_cargo_target_dir.sh" "$ROOT_DIR")"
  export CARGO_TARGET_DIR
fi

run_hunk() {
  (
    cd "$ROOT_DIR"
    cargo run -p hunk-desktop "$@"
  )
}

run_hunk_x11() {
  (
    cd "$ROOT_DIR"
    env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET \
      XDG_SESSION_TYPE=x11 \
      DISPLAY="${DISPLAY:-:0}" \
      cargo run -p hunk-desktop "$@"
  )
}

wayland_launch_failed() {
  local log_path="$1"

  rg -Fq "Protocol error 7 on object @0:" "$log_path" \
    || rg -Fq "failed to open window: Surface reports no supported texture formats" "$log_path" \
    || rg -Fq "failed to import supplied dmabufs" "$log_path" \
    || rg -Fq "Server-side decorations requested, but the Wayland server does not support them. Falling back to client-side decorations." "$log_path" \
    || rg -Fq "ERROR_SURFACE_LOST_KHR" "$log_path"
}

kill_run_tree() {
  local pid="$1"

  pkill -TERM -P "$pid" 2>/dev/null || true
  kill -TERM "$pid" 2>/dev/null || true
  sleep 0.2
  pkill -KILL -P "$pid" 2>/dev/null || true
  kill -KILL "$pid" 2>/dev/null || true
}

if [[ "$(uname -s)" != "Linux" ]]; then
  run_hunk "$@"
  exit $?
fi

if [[ -n "${HUNK_FORCE_X11:-}" ]]; then
  run_hunk_x11 "$@"
  exit $?
fi

if [[ -z "${WAYLAND_DISPLAY:-}" || -z "${DISPLAY:-}" ]]; then
  run_hunk "$@"
  exit $?
fi

log_path="$(mktemp "${TMPDIR:-/tmp}/hunk-linux-dev-run.XXXXXX.log")"

run_hunk "$@" \
  > >(tee -a "$log_path") \
  2> >(tee -a "$log_path" >&2) &
wayland_pid=$!

while kill -0 "$wayland_pid" 2>/dev/null; do
  if wayland_launch_failed "$log_path"; then
    echo "Wayland launch failed; retrying hunk-desktop under X11 fallback." >&2
    kill_run_tree "$wayland_pid"
    wait "$wayland_pid" 2>/dev/null || true
    rm -f "$log_path"
    run_hunk_x11 "$@"
    exit $?
  fi
  sleep 0.2
done

if wait "$wayland_pid"; then
  rm -f "$log_path"
  exit 0
fi

if wayland_launch_failed "$log_path"; then
  echo "Wayland launch failed; retrying hunk-desktop under X11 fallback." >&2
  rm -f "$log_path"
  run_hunk_x11 "$@"
  exit $?
fi

echo "Linux launch failed without a known Wayland fallback signature; log saved at $log_path" >&2
exit 1
