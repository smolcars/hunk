#[cfg(test)]
mod ai_visible_threads_tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    use hunk_codex::state::AiState;
    use hunk_codex::state::ThreadLifecycleStatus;
    use hunk_codex::state::ThreadSummary;

    use super::AiWorkspaceState;
    use super::ai_visible_thread_sections;
    use super::merged_ai_visible_threads;
    use super::state_snapshot_workspace_key;

    fn ai_test_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_temp_hunk_home<T>(test_name: &str, f: impl FnOnce(PathBuf) -> T) -> T {
        let _guard = ai_test_env_lock()
            .lock()
            .expect("ai test env lock should be available");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let temp_home = std::env::temp_dir().join(format!("hunk-ai-visible-{test_name}-{unique}"));
        let previous = std::env::var_os(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR);
        unsafe { std::env::set_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR, &temp_home) };
        let _ = std::fs::remove_dir_all(&temp_home);
        std::fs::create_dir_all(&temp_home).expect("temp hunk home should be created");

        let result = f(temp_home.clone());

        match previous {
            Some(value) => unsafe {
                std::env::set_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR, value)
            },
            None => unsafe { std::env::remove_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR) },
        }
        let _ = std::fs::remove_dir_all(&temp_home);

        result
    }

    fn thread_summary(
        id: &str,
        cwd: &str,
        created_at: i64,
        updated_at: i64,
    ) -> ThreadSummary {
        ThreadSummary {
            id: id.to_string(),
            cwd: cwd.to_string(),
            title: Some(id.to_string()),
            status: ThreadLifecycleStatus::Idle,
            created_at,
            updated_at,
            last_sequence: 1,
        }
    }

    #[test]
    fn state_snapshot_workspace_key_prefers_loaded_snapshot_workspace() {
        let mut state_snapshot = AiState::default();
        state_snapshot.threads.insert(
            "thread-local".to_string(),
            thread_summary("thread-local", "/repo", 10, 10),
        );

        let workspace_key = state_snapshot_workspace_key(
            &state_snapshot,
            None,
            Some("/repo/worktrees/task-7"),
            Some("/repo/worktrees/task-7"),
            true,
            false,
        );

        assert_eq!(workspace_key.as_deref(), Some("/repo"));
    }

    #[test]
    fn merged_visible_threads_keeps_background_worktree_threads_when_snapshot_is_local() {
        let mut state_snapshot = AiState::default();
        state_snapshot.threads.insert(
            "thread-local".to_string(),
            thread_summary("thread-local", "/repo", 10, 10),
        );

        let mut background_workspace_states = BTreeMap::new();

        let mut local_workspace_state = AiWorkspaceState::default();
        local_workspace_state.state_snapshot.threads.insert(
            "thread-local".to_string(),
            thread_summary("thread-local", "/repo", 10, 10),
        );
        background_workspace_states.insert("/repo".to_string(), local_workspace_state);

        let mut worktree_workspace_state = AiWorkspaceState::default();
        worktree_workspace_state.state_snapshot.threads.insert(
            "thread-worktree".to_string(),
            thread_summary("thread-worktree", "/repo/worktrees/task-7", 20, 20),
        );
        background_workspace_states.insert(
            "/repo/worktrees/task-7".to_string(),
            worktree_workspace_state,
        );

        let threads = merged_ai_visible_threads(
            &state_snapshot,
            Some("/repo"),
            &background_workspace_states,
            &[PathBuf::from("/repo")],
            Some(std::path::Path::new("/repo")),
            Some(std::path::Path::new("/repo")),
        );
        let thread_ids = threads.into_iter().map(|thread| thread.id).collect::<Vec<_>>();

        assert_eq!(thread_ids, vec!["thread-worktree", "thread-local"]);
    }

    #[test]
    fn merged_visible_threads_excludes_background_threads_from_other_projects() {
        let mut state_snapshot = AiState::default();
        state_snapshot.threads.insert(
            "thread-local".to_string(),
            thread_summary("thread-local", "/repo-b", 10, 10),
        );

        let mut background_workspace_states = BTreeMap::new();

        let mut current_worktree_state = AiWorkspaceState::default();
        current_worktree_state.state_snapshot.threads.insert(
            "thread-worktree-b".to_string(),
            thread_summary("thread-worktree-b", "/repo-b/worktrees/task-7", 20, 20),
        );
        background_workspace_states.insert(
            "/repo-b/worktrees/task-7".to_string(),
            current_worktree_state,
        );

        let mut old_project_state = AiWorkspaceState::default();
        old_project_state.state_snapshot.threads.insert(
            "thread-old-a".to_string(),
            thread_summary("thread-old-a", "/repo-a", 30, 30),
        );
        background_workspace_states.insert("/repo-a".to_string(), old_project_state);

        let threads = merged_ai_visible_threads(
            &state_snapshot,
            Some("/repo-b"),
            &background_workspace_states,
            &[PathBuf::from("/repo-b")],
            Some(std::path::Path::new("/repo-b")),
            Some(std::path::Path::new("/repo-b")),
        );
        let thread_ids = threads.into_iter().map(|thread| thread.id).collect::<Vec<_>>();

        assert_eq!(thread_ids, vec!["thread-worktree-b", "thread-local"]);
    }

    #[test]
    fn visible_thread_sections_follow_workspace_order_and_cap_per_project() {
        with_temp_hunk_home("visible-thread-sections-order", |_| {
            let threads = vec![
                thread_summary("repo-a-6", "/repo-a", 60, 60),
                thread_summary("repo-a-5", "/repo-a", 50, 50),
                thread_summary("repo-a-4", "/repo-a/worktrees/task-4", 40, 40),
                thread_summary("repo-a-3", "/repo-a/worktrees/task-3", 30, 30),
                thread_summary("repo-a-2", "/repo-a", 20, 20),
                thread_summary("repo-a-1", "/repo-a", 10, 10),
                thread_summary("repo-b-1", "/repo-b", 70, 70),
            ];

            let sections = ai_visible_thread_sections(
                threads,
                &[PathBuf::from("/repo-a"), PathBuf::from("/repo-b")],
                Some(std::path::Path::new("/repo-b")),
                Some(std::path::Path::new("/repo-b")),
                &BTreeSet::new(),
            );

            assert_eq!(sections.len(), 3);
            assert_eq!(sections[0].project_label, "Chats");
            assert_eq!(sections[0].threads.len(), 0);

            assert_eq!(sections[1].project_root, PathBuf::from("/repo-a"));
            assert_eq!(sections[1].total_thread_count, 6);
            assert_eq!(sections[1].threads.len(), 5);
            assert_eq!(sections[1].hidden_thread_count, 1);
            let repo_a_thread_ids = sections[1]
                .threads
                .iter()
                .map(|thread| thread.id.as_str())
                .collect::<Vec<_>>();
            assert_eq!(
                repo_a_thread_ids,
                vec!["repo-a-6", "repo-a-5", "repo-a-4", "repo-a-3", "repo-a-2"]
            );

            assert_eq!(sections[2].project_root, PathBuf::from("/repo-b"));
            assert_eq!(sections[2].threads.len(), 1);
        });
    }
}
