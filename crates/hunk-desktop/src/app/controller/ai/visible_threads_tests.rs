#[cfg(test)]
mod ai_visible_threads_tests {
    use std::collections::BTreeMap;

    use hunk_codex::state::AiState;
    use hunk_codex::state::ThreadLifecycleStatus;
    use hunk_codex::state::ThreadSummary;

    use super::AiWorkspaceState;
    use super::merged_ai_visible_threads;
    use super::state_snapshot_workspace_key;

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
        );
        let thread_ids = threads.into_iter().map(|thread| thread.id).collect::<Vec<_>>();

        assert_eq!(thread_ids, vec!["thread-worktree", "thread-local"]);
    }
}
