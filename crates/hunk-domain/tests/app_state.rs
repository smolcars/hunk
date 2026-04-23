use std::path::PathBuf;

use hunk_domain::state::AiCollaborationModeSelection;
use hunk_domain::state::AiServiceTierSelection;
use hunk_domain::state::AiThreadSessionState;
use hunk_domain::state::AppState;
use hunk_domain::state::CachedChangedFileState;
use hunk_domain::state::CachedLocalBranchState;
use hunk_domain::state::CachedRecentCommitState;
use hunk_domain::state::CachedRecentCommitsState;
use hunk_domain::state::CachedWorkflowState;
use hunk_domain::state::ReviewCompareSelectionState;

#[test]
fn app_state_defaults_workspace_state_to_empty() {
    let state = AppState::default();
    assert!(state.workspace_project_paths.is_empty());
    assert_eq!(state.active_workspace_project_path, None);
    assert_eq!(state.preferred_project_open_target_id, None);
    assert!(state.last_workspace_target_by_repo.is_empty());
    assert!(state.review_compare_selection_by_repo.is_empty());
    assert!(state.ai_bookmarked_thread_ids.is_empty());
    assert!(state.ai_workspace_mad_max.is_empty());
    assert!(state.ai_workspace_include_hidden_models.is_empty());
    assert!(state.ai_workspace_session_overrides.is_empty());
    assert!(state.ai_thread_session_overrides.is_empty());
    assert!(state.git_workflow_cache_by_repo.is_empty());
    assert!(state.git_recent_commits_cache_by_repo.is_empty());
}

#[test]
fn app_state_parses_without_workspace_fields() {
    let raw = "";
    let state: AppState = toml::from_str(raw).expect("state without fields should parse");
    assert!(state.workspace_project_paths.is_empty());
    assert_eq!(state.active_workspace_project_path, None);
    assert_eq!(state.preferred_project_open_target_id, None);
    assert!(state.last_workspace_target_by_repo.is_empty());
    assert!(state.review_compare_selection_by_repo.is_empty());
    assert!(state.ai_bookmarked_thread_ids.is_empty());
    assert!(state.ai_workspace_mad_max.is_empty());
    assert!(state.ai_workspace_include_hidden_models.is_empty());
    assert!(state.ai_workspace_session_overrides.is_empty());
    assert!(state.ai_thread_session_overrides.is_empty());
    assert!(state.git_workflow_cache_by_repo.is_empty());
    assert!(state.git_recent_commits_cache_by_repo.is_empty());
}

#[test]
fn app_state_migrates_legacy_last_project_path_into_workspace_state() {
    let raw = r#"
last_project_path = "/tmp/hunk-repo"
"#;
    let mut state: AppState = toml::from_str(raw).expect("legacy state should parse");

    state.normalize_workspace_state();

    assert_eq!(
        state.workspace_project_paths,
        vec![PathBuf::from("/tmp/hunk-repo")]
    );
    assert_eq!(
        state.active_workspace_project_path,
        Some(PathBuf::from("/tmp/hunk-repo"))
    );
    assert_eq!(state.legacy_last_project_path, None);
}

#[test]
fn ai_thread_session_state_preferred_defaults_pin_model_and_effort() {
    let defaults = AiThreadSessionState::preferred_defaults();

    assert_eq!(defaults.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(defaults.effort.as_deref(), Some("high"));
    assert_eq!(
        defaults.collaboration_mode,
        AiCollaborationModeSelection::Default
    );
    assert_eq!(defaults.service_tier, None);
}

#[test]
fn app_state_round_trips_workspace_fields() {
    let state = AppState {
        legacy_last_project_path: None,
        workspace_project_paths: vec![
            PathBuf::from("/tmp/hunk-repo"),
            PathBuf::from("/tmp/hunk-repo-b"),
        ],
        active_workspace_project_path: Some(PathBuf::from("/tmp/hunk-repo-b")),
        preferred_project_open_target_id: Some("zed".to_string()),
        last_workspace_target_by_repo: [(
            "/tmp/hunk-repo".to_string(),
            "worktree:feature".to_string(),
        )]
        .into_iter()
        .collect(),
        review_compare_selection_by_repo: [(
            "/tmp/hunk-repo".to_string(),
            ReviewCompareSelectionState {
                left_source_id: Some("branch:main".to_string()),
                right_source_id: Some("worktree:feature".to_string()),
            },
        )]
        .into_iter()
        .collect(),
        ai_bookmarked_thread_ids: ["thread-1".to_string(), "thread-2".to_string()]
            .into_iter()
            .collect(),
        ai_workspace_mad_max: [("/tmp/hunk-repo".to_string(), true)].into_iter().collect(),
        ai_workspace_include_hidden_models: [("/tmp/hunk-repo".to_string(), true)]
            .into_iter()
            .collect(),
        ai_workspace_session_overrides: [(
            "/tmp/hunk-repo".to_string(),
            AiThreadSessionState {
                model: Some("gpt-5-codex".to_string()),
                effort: Some("high".to_string()),
                collaboration_mode: AiCollaborationModeSelection::Plan,
                service_tier: Some(AiServiceTierSelection::Fast),
            },
        )]
        .into_iter()
        .collect(),
        ai_thread_session_overrides: [(
            "thread-1".to_string(),
            AiThreadSessionState {
                model: Some("gpt-5.4".to_string()),
                effort: Some("high".to_string()),
                collaboration_mode: AiCollaborationModeSelection::Default,
                service_tier: None,
            },
        )]
        .into_iter()
        .collect(),
        git_workflow_cache_by_repo: [(
            "/tmp/hunk-repo".to_string(),
            CachedWorkflowState {
                root: Some(PathBuf::from("/tmp/hunk-repo")),
                branch_name: "main".to_string(),
                branch_has_upstream: true,
                branch_ahead_count: 2,
                branch_behind_count: 1,
                branches: vec![CachedLocalBranchState {
                    name: "main".to_string(),
                    is_current: true,
                    is_remote_tracking: false,
                    tip_unix_time: Some(1_711_111_111),
                    attached_workspace_target_id: Some("primary".to_string()),
                    attached_workspace_target_root: Some(PathBuf::from("/tmp/hunk-repo")),
                    attached_workspace_target_label: Some("Primary Checkout".to_string()),
                }],
                files: vec![CachedChangedFileState {
                    path: "src/main.rs".to_string(),
                    status_tag: "M".to_string(),
                    staged: false,
                    unstaged: true,
                    untracked: false,
                }],
                last_commit_subject: Some("cached".to_string()),
                cached_unix_time: 1_711_111_111,
            },
        )]
        .into_iter()
        .collect(),
        git_recent_commits_cache_by_repo: [(
            "/tmp/hunk-repo".to_string(),
            CachedRecentCommitsState {
                root: Some(PathBuf::from("/tmp/hunk-repo")),
                head_ref_name: Some("refs/heads/main".to_string()),
                head_commit_id: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
                base_tip_id: None,
                commits: vec![CachedRecentCommitState {
                    commit_id: "0123456789abcdef0123456789abcdef01234567".to_string(),
                    subject: "recent".to_string(),
                    committed_unix_time: Some(1_711_111_222),
                }],
                cached_unix_time: 1_711_111_222,
            },
        )]
        .into_iter()
        .collect(),
    };

    let raw = toml::to_string(&state).expect("state should serialize");
    let loaded: AppState = toml::from_str(&raw).expect("state should deserialize");

    assert_eq!(loaded, state);
}

#[test]
fn normalize_workspace_state_promotes_active_project_when_list_is_empty() {
    let mut state = AppState {
        active_workspace_project_path: Some(PathBuf::from("/tmp/hunk-repo")),
        ..AppState::default()
    };

    state.normalize_workspace_state();

    assert_eq!(
        state.workspace_project_paths,
        vec![PathBuf::from("/tmp/hunk-repo")]
    );
    assert_eq!(
        state.active_workspace_project_path,
        Some(PathBuf::from("/tmp/hunk-repo"))
    );
    assert_eq!(
        state.active_project_path(),
        Some(&PathBuf::from("/tmp/hunk-repo"))
    );
}

#[test]
fn normalize_workspace_state_dedupes_and_repairs_active_project() {
    let mut state = AppState {
        workspace_project_paths: vec![
            PathBuf::from("/tmp/hunk-repo-a"),
            PathBuf::from("/tmp/hunk-repo-a"),
            PathBuf::from("/tmp/hunk-repo-b"),
        ],
        active_workspace_project_path: Some(PathBuf::from("/tmp/hunk-repo-a")),
        ..AppState::default()
    };

    state.normalize_workspace_state();

    assert_eq!(
        state.workspace_project_paths,
        vec![
            PathBuf::from("/tmp/hunk-repo-a"),
            PathBuf::from("/tmp/hunk-repo-b"),
        ]
    );
    assert_eq!(
        state.active_workspace_project_path,
        Some(PathBuf::from("/tmp/hunk-repo-a"))
    );
}

#[test]
fn normalize_workspace_state_promotes_active_project_into_membership() {
    let mut state = AppState {
        workspace_project_paths: vec![PathBuf::from("/tmp/hunk-repo-a")],
        active_workspace_project_path: Some(PathBuf::from("/tmp/hunk-repo-b")),
        ..AppState::default()
    };

    state.normalize_workspace_state();

    assert_eq!(
        state.workspace_project_paths,
        vec![
            PathBuf::from("/tmp/hunk-repo-a"),
            PathBuf::from("/tmp/hunk-repo-b"),
        ]
    );
    assert_eq!(
        state.active_workspace_project_path,
        Some(PathBuf::from("/tmp/hunk-repo-b"))
    );
}

#[test]
fn activate_workspace_project_appends_and_selects_project() {
    let mut state = AppState {
        workspace_project_paths: vec![PathBuf::from("/tmp/hunk-repo-a")],
        active_workspace_project_path: Some(PathBuf::from("/tmp/hunk-repo-a")),
        ..AppState::default()
    };

    let changed = state.activate_workspace_project(PathBuf::from("/tmp/hunk-repo-b"));

    assert!(changed);
    assert_eq!(
        state.workspace_project_paths,
        vec![
            PathBuf::from("/tmp/hunk-repo-a"),
            PathBuf::from("/tmp/hunk-repo-b"),
        ]
    );
    assert_eq!(
        state.active_workspace_project_path,
        Some(PathBuf::from("/tmp/hunk-repo-b"))
    );
}

#[test]
fn activate_workspace_project_selects_existing_without_duplication() {
    let mut state = AppState {
        workspace_project_paths: vec![
            PathBuf::from("/tmp/hunk-repo-a"),
            PathBuf::from("/tmp/hunk-repo-b"),
        ],
        active_workspace_project_path: Some(PathBuf::from("/tmp/hunk-repo-a")),
        ..AppState::default()
    };

    let changed = state.activate_workspace_project(PathBuf::from("/tmp/hunk-repo-b"));

    assert!(changed);
    assert_eq!(
        state.workspace_project_paths,
        vec![
            PathBuf::from("/tmp/hunk-repo-a"),
            PathBuf::from("/tmp/hunk-repo-b"),
        ]
    );
    assert_eq!(
        state.active_workspace_project_path,
        Some(PathBuf::from("/tmp/hunk-repo-b"))
    );
}

#[test]
fn remove_workspace_project_selects_next_project_when_active_removed() {
    let mut state = AppState {
        workspace_project_paths: vec![
            PathBuf::from("/tmp/hunk-repo-a"),
            PathBuf::from("/tmp/hunk-repo-b"),
            PathBuf::from("/tmp/hunk-repo-c"),
        ],
        active_workspace_project_path: Some(PathBuf::from("/tmp/hunk-repo-b")),
        ..AppState::default()
    };

    let changed = state.remove_workspace_project(PathBuf::from("/tmp/hunk-repo-b").as_path());

    assert!(changed);
    assert_eq!(
        state.workspace_project_paths,
        vec![
            PathBuf::from("/tmp/hunk-repo-a"),
            PathBuf::from("/tmp/hunk-repo-c"),
        ]
    );
    assert_eq!(
        state.active_workspace_project_path,
        Some(PathBuf::from("/tmp/hunk-repo-c"))
    );
}

#[test]
fn remove_workspace_project_selects_previous_when_last_active_removed() {
    let mut state = AppState {
        workspace_project_paths: vec![
            PathBuf::from("/tmp/hunk-repo-a"),
            PathBuf::from("/tmp/hunk-repo-b"),
        ],
        active_workspace_project_path: Some(PathBuf::from("/tmp/hunk-repo-b")),
        ..AppState::default()
    };

    let changed = state.remove_workspace_project(PathBuf::from("/tmp/hunk-repo-b").as_path());

    assert!(changed);
    assert_eq!(
        state.workspace_project_paths,
        vec![PathBuf::from("/tmp/hunk-repo-a")]
    );
    assert_eq!(
        state.active_workspace_project_path,
        Some(PathBuf::from("/tmp/hunk-repo-a"))
    );
}

#[test]
fn remove_workspace_project_clears_active_when_last_project_removed() {
    let mut state = AppState {
        workspace_project_paths: vec![PathBuf::from("/tmp/hunk-repo-a")],
        active_workspace_project_path: Some(PathBuf::from("/tmp/hunk-repo-a")),
        ..AppState::default()
    };

    let changed = state.remove_workspace_project(PathBuf::from("/tmp/hunk-repo-a").as_path());

    assert!(changed);
    assert!(state.workspace_project_paths.is_empty());
    assert_eq!(state.active_workspace_project_path, None);
}
