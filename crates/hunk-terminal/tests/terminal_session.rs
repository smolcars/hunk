#![cfg(not(target_os = "windows"))]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hunk_terminal::{
    TerminalEvent, TerminalScreenSnapshot, TerminalScroll, TerminalSpawnRequest,
    spawn_terminal_session,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(3);

#[test]
fn terminal_session_emits_vt_screen_snapshots_for_output() {
    let request = TerminalSpawnRequest::new(repo_root(), "printf 'hello from vt\\n'".to_string())
        .with_shell_program(test_shell_program());
    let (_handle, event_rx) =
        spawn_terminal_session(request).expect("terminal session should start");

    let events = collect_events_until_exit(&event_rx);

    let output_text = events
        .iter()
        .filter_map(|event| match event {
            TerminalEvent::Output(bytes) => Some(String::from_utf8_lossy(bytes).to_string()),
            _ => None,
        })
        .collect::<String>();
    assert!(output_text.contains("hello from vt"));

    let rendered_text = events
        .iter()
        .filter_map(|event| match event {
            TerminalEvent::Screen(screen) => Some(screen_text(screen)),
            _ => None,
        })
        .find(|text| text.contains("hello from vt"))
        .expect("expected a VT screen snapshot containing rendered output");
    assert!(rendered_text.contains("hello from vt"));
}

#[test]
fn terminal_session_emits_updated_screen_snapshot_after_resize() {
    let request = TerminalSpawnRequest::new(repo_root(), "sleep 1".to_string())
        .with_shell_program(test_shell_program());
    let (handle, event_rx) =
        spawn_terminal_session(request).expect("terminal session should start");

    handle.resize(40, 140).expect("resize should succeed");

    let resized = collect_until(&event_rx, |event| match event {
        TerminalEvent::Screen(screen) => screen.rows == 40 && screen.cols == 140,
        _ => false,
    })
    .expect("expected screen snapshot with resized dimensions");

    let TerminalEvent::Screen(screen) = resized else {
        panic!("expected a screen event");
    };
    assert_eq!(screen.rows, 40);
    assert_eq!(screen.cols, 140);

    handle.kill().expect("kill should succeed");
}

#[test]
fn terminal_session_supports_scrollback_after_output() {
    let mut request = TerminalSpawnRequest::new(
        repo_root(),
        "i=1; while [ \"$i\" -le 12 ]; do printf 'line %s\\n' \"$i\"; i=$((i + 1)); done; sleep 1"
            .to_string(),
    )
    .with_shell_program(test_shell_program());
    request.rows = 5;
    request.cols = 80;

    let (handle, event_rx) =
        spawn_terminal_session(request).expect("terminal session should start");

    let _ = collect_until(&event_rx, |event| match event {
        TerminalEvent::Screen(screen) => screen_text(screen).contains("line 12"),
        _ => false,
    })
    .expect("expected screen snapshot containing generated output");

    handle
        .scroll_display(TerminalScroll::PageUp)
        .expect("page up should succeed");

    let scrolled = collect_until(&event_rx, |event| match event {
        TerminalEvent::Screen(screen) => {
            screen.display_offset > 0 && screen_text(screen).contains("line 8")
        }
        _ => false,
    })
    .expect("expected scrolled screen snapshot");

    let TerminalEvent::Screen(screen) = scrolled else {
        panic!("expected a screen event");
    };
    assert!(screen.display_offset > 0);

    handle.kill().expect("kill should succeed");
}

fn collect_events_until_exit(
    event_rx: &std::sync::mpsc::Receiver<TerminalEvent>,
) -> Vec<TerminalEvent> {
    let deadline = Instant::now() + TEST_TIMEOUT;
    let mut events = Vec::new();
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let event = event_rx
            .recv_timeout(remaining.min(Duration::from_millis(250)))
            .expect("expected terminal event before timeout");
        let exited = matches!(event, TerminalEvent::Exit { .. });
        events.push(event);
        if exited {
            return events;
        }
    }
    panic!("timed out waiting for terminal exit event");
}

fn collect_until(
    event_rx: &std::sync::mpsc::Receiver<TerminalEvent>,
    predicate: impl Fn(&TerminalEvent) -> bool,
) -> Option<TerminalEvent> {
    let deadline = Instant::now() + TEST_TIMEOUT;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let event = event_rx
            .recv_timeout(remaining.min(Duration::from_millis(250)))
            .ok()?;
        if predicate(&event) {
            return Some(event);
        }
    }
    None
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .map(ToOwned::to_owned)
        .expect("crate should live under workspace/crates")
}

fn test_shell_program() -> &'static str {
    if PathBuf::from("/bin/sh").exists() {
        "/bin/sh"
    } else {
        "/bin/bash"
    }
}

fn screen_text(screen: &Arc<TerminalScreenSnapshot>) -> String {
    let mut cells = screen.cells.clone();
    cells.sort_by_key(|cell| (cell.line, cell.column));

    let mut current_line = None;
    let mut output = String::new();
    for cell in cells {
        if current_line != Some(cell.line) {
            if current_line.is_some() {
                output.push('\n');
            }
            current_line = Some(cell.line);
        }
        output.push(cell.character);
    }
    output
}
