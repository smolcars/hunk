#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REAL_BINARY_NAME="${HUNK_LINUX_REAL_BINARY_NAME:-hunk_desktop_bin}"
REAL_BINARY_PATH="$SCRIPT_DIR/$REAL_BINARY_NAME"
PRIVATE_LIB_DIR="$SCRIPT_DIR/lib"

run_hunk() {
  env LD_LIBRARY_PATH="$PRIVATE_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
    "$REAL_BINARY_PATH" "$@"
}

run_hunk_x11() {
  env \
    LD_LIBRARY_PATH="$PRIVATE_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
    WAYLAND_DISPLAY='' \
    "$REAL_BINARY_PATH" "$@"
}

print_launch_debug() {
  if [[ -n "${HUNK_LINUX_LAUNCH_DEBUG:-}" ]]; then
    echo "$1" >&2
  fi
}

wayland_launch_failed() {
  local log_path="$1"

  grep -Fq "Protocol error 7 on object @0:" "$log_path" \
    || grep -Fq "failed to open window: Surface reports no supported texture formats" "$log_path" \
    || grep -Fq "failed to import supplied dmabufs" "$log_path" \
    || grep -Fq "NoWaylandLib" "$log_path" \
    || grep -Fq "ERROR_SURFACE_LOST_KHR" "$log_path"
}

launch_with_log_capture() {
  local log_path="$1"
  shift

  if [[ -n "${HUNK_LINUX_LAUNCH_DEBUG:-}" ]]; then
    "$@" > >(tee -a "$log_path") 2> >(tee -a "$log_path" >&2) &
  else
    "$@" >>"$log_path" 2>&1 &
  fi
}

kill_run_tree() {
  local pid="$1"

  pkill -TERM -P "$pid" 2>/dev/null || true
  kill -TERM "$pid" 2>/dev/null || true
  sleep 0.2
  pkill -KILL -P "$pid" 2>/dev/null || true
  kill -KILL "$pid" 2>/dev/null || true
}

if [[ ! -x "$REAL_BINARY_PATH" ]]; then
  echo "error: expected Linux GUI binary at $REAL_BINARY_PATH" >&2
  exit 1
fi

if [[ "$(uname -s)" != "Linux" ]]; then
  run_hunk "$@"
  exit $?
fi

if [[ -n "${HUNK_FORCE_X11:-}" ]]; then
  print_launch_debug "Launching hunk-desktop with explicit X11 override."
  run_hunk_x11 "$@"
  exit $?
fi

if [[ -n "${DISPLAY:-}" && -n "${WAYLAND_DISPLAY:-}" && -z "${HUNK_PREFER_WAYLAND:-}" ]]; then
  print_launch_debug "Launching hunk-desktop via X11 by default; set HUNK_PREFER_WAYLAND=1 to opt into native Wayland."
  run_hunk_x11 "$@"
  exit $?
fi

if [[ -z "${WAYLAND_DISPLAY:-}" || -z "${DISPLAY:-}" ]]; then
  print_launch_debug "Launching hunk-desktop without dual-display fallback."
  run_hunk "$@"
  exit $?
fi

log_path="$(mktemp "${TMPDIR:-/tmp}/hunk-linux-release-run.XXXXXX.log")"

launch_with_log_capture "$log_path" run_hunk "$@"
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
