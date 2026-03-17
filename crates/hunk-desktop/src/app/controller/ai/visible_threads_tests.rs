#[cfg(test)]
mod ai_visible_threads_tests {
    use std::collections::BTreeMap;

    use hunk_codex::state::AiState;
    use hunk_codex::state::ThreadLifecycleStatus;
    use hunk_codex::state::ThreadSummary;
    use hunk_git::worktree::WorkspaceTargetKind;
    use hunk_git::worktree::WorkspaceTargetSummary;

    use super::AiWorkspaceState;
    use super::merged_ai_visible_threads;
    use super::state_snapshot_workspace_key;
    use std::path::PathBuf;

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

    fn workspace_target(
        id: &str,
        kind: WorkspaceTargetKind,
        root: &str,
    ) -> WorkspaceTargetSummary {
        WorkspaceTargetSummary {
            id: id.to_string(),
            kind,
            root: PathBuf::from(root),
            name: id.to_string(),
            display_name: id.to_string(),
            branch_name: "main".to_string(),
            managed: matches!(kind, WorkspaceTargetKind::LinkedWorktree),
            is_active: false,
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
            &[
                workspace_target("primary", WorkspaceTargetKind::PrimaryCheckout, "/repo"),
                workspace_target(
                    "task-7",
                    WorkspaceTargetKind::LinkedWorktree,
                    "/repo/worktrees/task-7",
                ),
            ],
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
            &[
                workspace_target("primary", WorkspaceTargetKind::PrimaryCheckout, "/repo-b"),
                workspace_target(
                    "task-7",
                    WorkspaceTargetKind::LinkedWorktree,
                    "/repo-b/worktrees/task-7",
                ),
            ],
            Some(std::path::Path::new("/repo-b")),
            Some(std::path::Path::new("/repo-b")),
        );
        let thread_ids = threads.into_iter().map(|thread| thread.id).collect::<Vec<_>>();

        assert_eq!(thread_ids, vec!["thread-worktree-b", "thread-local"]);
    }
}
