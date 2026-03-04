use std::path::PathBuf;

use hunk_domain::state::AiThreadSessionState;
use hunk_domain::state::AppState;

#[test]
fn app_state_defaults_last_project_path_to_none() {
    let state = AppState::default();
    assert_eq!(state.last_project_path, None);
    assert!(state.ai_workspace_mad_max.is_empty());
    assert!(state.ai_workspace_include_hidden_models.is_empty());
    assert!(state.ai_thread_session_overrides.is_empty());
}

#[test]
fn app_state_parses_without_last_project_path_field() {
    let raw = "";
    let state: AppState = toml::from_str(raw).expect("state without fields should parse");
    assert_eq!(state.last_project_path, None);
    assert!(state.ai_workspace_mad_max.is_empty());
    assert!(state.ai_workspace_include_hidden_models.is_empty());
    assert!(state.ai_thread_session_overrides.is_empty());
}

#[test]
fn app_state_round_trips_last_project_path() {
    let state = AppState {
        last_project_path: Some(PathBuf::from("/tmp/hunk-repo")),
        ai_workspace_mad_max: [("/tmp/hunk-repo".to_string(), true)].into_iter().collect(),
        ai_workspace_include_hidden_models: [("/tmp/hunk-repo".to_string(), true)]
            .into_iter()
            .collect(),
        ai_thread_session_overrides: [(
            "/tmp/hunk-repo".to_string(),
            [(
                "thread-1".to_string(),
                AiThreadSessionState {
                    model: Some("gpt-5-codex".to_string()),
                    effort: Some("high".to_string()),
                    collaboration_mode: Some("Plan".to_string()),
                },
            )]
            .into_iter()
            .collect(),
        )]
        .into_iter()
        .collect(),
    };

    let raw = toml::to_string(&state).expect("state should serialize");
    let loaded: AppState = toml::from_str(&raw).expect("state should deserialize");

    assert_eq!(loaded, state);
}
