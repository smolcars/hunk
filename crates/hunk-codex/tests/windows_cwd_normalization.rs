#![cfg(windows)]

use std::path::PathBuf;

use hunk_codex::protocol::ServerNotification;
use hunk_codex::protocol::SessionSource;
use hunk_codex::protocol::Thread;
use hunk_codex::protocol::ThreadStartedNotification;
use hunk_codex::protocol::ThreadStatus;
use hunk_codex::threads::ThreadService;

const WINDOWS_WORKSPACE_CWD: &str = r"C:\Users\nites\Documents\hunk";
const WINDOWS_WORKSPACE_CWD_VERBATIM: &str = r"\\?\C:\Users\nites\Documents\hunk";

#[test]
fn thread_started_notification_matches_equivalent_windows_cwd_forms() {
    let mut service = ThreadService::new(PathBuf::from(WINDOWS_WORKSPACE_CWD_VERBATIM));

    service.apply_server_notification(ServerNotification::ThreadStarted(
        ThreadStartedNotification {
            thread: thread("thread-1", WINDOWS_WORKSPACE_CWD),
        },
    ));

    let thread = service
        .state()
        .threads
        .get("thread-1")
        .expect("thread should be ingested for equivalent Windows cwd forms");
    assert_eq!(thread.cwd, WINDOWS_WORKSPACE_CWD);
}

fn thread(id: &str, cwd: &str) -> Thread {
    Thread {
        id: id.to_string(),
        preview: format!("preview-{id}"),
        ephemeral: false,
        model_provider: "openai".to_string(),
        path: Some(format!(r"C:\tmp\.codex\threads\{id}.jsonl").into()),
        name: Some(format!("Thread {id}")),
        cwd: PathBuf::from(cwd),
        cli_version: "0.1.0".to_string(),
        source: SessionSource::AppServer,
        agent_nickname: None,
        agent_role: None,
        forked_from_id: None,
        git_info: None,
        turns: Vec::new(),
        status: ThreadStatus::Idle,
        created_at: 0,
        updated_at: 0,
    }
}
