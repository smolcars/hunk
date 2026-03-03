use std::path::PathBuf;

use hunk_domain::state::AppState;

#[test]
fn app_state_defaults_last_project_path_to_none() {
    let state = AppState::default();
    assert_eq!(state.last_project_path, None);
}

#[test]
fn app_state_parses_without_last_project_path_field() {
    let raw = "";
    let state: AppState = toml::from_str(raw).expect("state without fields should parse");
    assert_eq!(state.last_project_path, None);
}

#[test]
fn app_state_round_trips_last_project_path() {
    let state = AppState {
        last_project_path: Some(PathBuf::from("/tmp/hunk-repo")),
    };

    let raw = toml::to_string(&state).expect("state should serialize");
    let loaded: AppState = toml::from_str(&raw).expect("state should deserialize");

    assert_eq!(loaded, state);
}
