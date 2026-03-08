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

#[test]
fn app_state_defaults_last_project_path_to_none() {
    let state = AppState::default();
    assert_eq!(state.last_project_path, None);
    assert!(state.ai_workspace_mad_max.is_empty());
    assert!(state.ai_workspace_include_hidden_models.is_empty());
    assert!(state.ai_workspace_session_overrides.is_empty());
    assert!(state.git_workflow_cache.is_none());
    assert!(state.git_recent_commits_cache.is_none());
}

#[test]
fn app_state_parses_without_last_project_path_field() {
    let raw = "";
    let state: AppState = toml::from_str(raw).expect("state without fields should parse");
    assert_eq!(state.last_project_path, None);
    assert!(state.ai_workspace_mad_max.is_empty());
    assert!(state.ai_workspace_include_hidden_models.is_empty());
    assert!(state.ai_workspace_session_overrides.is_empty());
    assert!(state.git_workflow_cache.is_none());
    assert!(state.git_recent_commits_cache.is_none());
}

#[test]
fn app_state_round_trips_last_project_path() {
    let state = AppState {
        last_project_path: Some(PathBuf::from("/tmp/hunk-repo")),
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
        git_workflow_cache: Some(CachedWorkflowState {
            root: Some(PathBuf::from("/tmp/hunk-repo")),
            branch_name: "main".to_string(),
            branch_has_upstream: true,
            branch_ahead_count: 2,
            branch_behind_count: 1,
            branches: vec![CachedLocalBranchState {
                name: "main".to_string(),
                is_current: true,
                tip_unix_time: Some(1_711_111_111),
            }],
            files: vec![CachedChangedFileState {
                path: "src/main.rs".to_string(),
                status_tag: "M".to_string(),
                staged: false,
                untracked: false,
            }],
            last_commit_subject: Some("cached".to_string()),
            cached_unix_time: 1_711_111_111,
        }),
        git_recent_commits_cache: Some(CachedRecentCommitsState {
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
        }),
    };

    let raw = toml::to_string(&state).expect("state should serialize");
    let loaded: AppState = toml::from_str(&raw).expect("state should deserialize");

    assert_eq!(loaded, state);
}
