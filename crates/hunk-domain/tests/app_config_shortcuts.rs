use hunk_domain::config::{AppConfig, KeyboardShortcuts, ReviewProviderKind, ThemePreference};

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn app_config_defaults_include_existing_keyboard_shortcuts() {
    let config = AppConfig::default();

    assert_eq!(
        config.keyboard_shortcuts.select_next_line,
        strings(&["down"])
    );
    assert_eq!(
        config.keyboard_shortcuts.select_previous_line,
        strings(&["up"])
    );
    assert_eq!(
        config.keyboard_shortcuts.extend_selection_next_line,
        strings(&["shift-down"])
    );
    assert_eq!(
        config.keyboard_shortcuts.extend_selection_previous_line,
        strings(&["shift-up"])
    );
    assert_eq!(
        config.keyboard_shortcuts.copy_selection,
        strings(&["cmd-c", "ctrl-c"])
    );
    assert_eq!(
        config.keyboard_shortcuts.select_all_diff_rows,
        strings(&["cmd-a", "ctrl-a"])
    );
    assert_eq!(config.keyboard_shortcuts.next_hunk, strings(&["f7"]));
    assert_eq!(
        config.keyboard_shortcuts.previous_hunk,
        strings(&["shift-f7"])
    );
    assert_eq!(config.keyboard_shortcuts.next_file, strings(&["alt-down"]));
    assert_eq!(
        config.keyboard_shortcuts.previous_file,
        strings(&["alt-up"])
    );
    assert_eq!(
        config.keyboard_shortcuts.next_bookmark_revision,
        strings(&["alt-right"])
    );
    assert_eq!(
        config.keyboard_shortcuts.previous_bookmark_revision,
        strings(&["alt-left"])
    );
    assert_eq!(
        config.keyboard_shortcuts.toggle_sidebar_tree,
        strings(&["cmd-b", "ctrl-b"])
    );
    assert_eq!(
        config.keyboard_shortcuts.switch_to_files_view,
        strings(&["cmd-1", "ctrl-1"])
    );
    assert_eq!(
        config.keyboard_shortcuts.switch_to_review_view,
        strings(&["cmd-2", "ctrl-2"])
    );
    assert_eq!(
        config.keyboard_shortcuts.switch_to_graph_view,
        strings(&["cmd-3", "ctrl-3"])
    );
    assert_eq!(
        config.keyboard_shortcuts.open_project,
        strings(&["cmd-shift-o", "ctrl-shift-o"])
    );
    assert_eq!(
        config.keyboard_shortcuts.save_current_file,
        strings(&["cmd-s", "ctrl-s"])
    );
    assert_eq!(
        config.keyboard_shortcuts.open_settings,
        strings(&["cmd-,", "ctrl-,"])
    );
    assert_eq!(config.keyboard_shortcuts.quit_app, strings(&["cmd-q"]));
    assert!(
        config.review_provider_mappings.is_empty(),
        "review provider mappings should default to empty"
    );
    assert!(
        !config.reduce_motion,
        "reduced motion should default to disabled"
    );
}

#[test]
fn app_config_parses_without_keyboard_shortcuts_field() {
    let raw = r#"
theme = "dark"
"#;
    let config: AppConfig =
        toml::from_str(raw).expect("config without keyboard_shortcuts should parse");

    assert_eq!(config.theme, ThemePreference::Dark);
    assert_eq!(config.keyboard_shortcuts, KeyboardShortcuts::default());
    assert!(
        !config.reduce_motion,
        "configs missing reduce_motion should fall back to false"
    );
}

#[test]
fn app_config_applies_partial_shortcut_overrides() {
    let raw = r#"
[keyboard_shortcuts]
open_project = ["cmd-o", "ctrl-o"]
next_hunk = ["f8"]
"#;
    let config: AppConfig = toml::from_str(raw).expect("partial keyboard_shortcuts should parse");

    assert_eq!(
        config.keyboard_shortcuts.open_project,
        strings(&["cmd-o", "ctrl-o"])
    );
    assert_eq!(config.keyboard_shortcuts.next_hunk, strings(&["f8"]));
    assert_eq!(
        config.keyboard_shortcuts.save_current_file,
        strings(&["cmd-s", "ctrl-s"])
    );
    assert_eq!(
        config.keyboard_shortcuts.next_bookmark_revision,
        strings(&["alt-right"])
    );
    assert_eq!(
        config.keyboard_shortcuts.previous_bookmark_revision,
        strings(&["alt-left"])
    );
    assert_eq!(
        config.keyboard_shortcuts.toggle_sidebar_tree,
        strings(&["cmd-b", "ctrl-b"])
    );
    assert_eq!(
        config.keyboard_shortcuts.switch_to_files_view,
        strings(&["cmd-1", "ctrl-1"])
    );
    assert_eq!(
        config.keyboard_shortcuts.switch_to_review_view,
        strings(&["cmd-2", "ctrl-2"])
    );
    assert_eq!(
        config.keyboard_shortcuts.switch_to_graph_view,
        strings(&["cmd-3", "ctrl-3"])
    );
    assert_eq!(
        config.keyboard_shortcuts.open_settings,
        strings(&["cmd-,", "ctrl-,"])
    );
}

#[test]
fn app_config_allows_disabling_shortcuts_with_empty_list() {
    let raw = r#"
[keyboard_shortcuts]
quit_app = []
"#;
    let config: AppConfig = toml::from_str(raw).expect("empty shortcut list should parse");

    assert!(config.keyboard_shortcuts.quit_app.is_empty());
    assert_eq!(
        config.keyboard_shortcuts.open_project,
        strings(&["cmd-shift-o", "ctrl-shift-o"])
    );
    assert_eq!(
        config.keyboard_shortcuts.toggle_sidebar_tree,
        strings(&["cmd-b", "ctrl-b"])
    );
    assert_eq!(
        config.keyboard_shortcuts.switch_to_files_view,
        strings(&["cmd-1", "ctrl-1"])
    );
    assert_eq!(
        config.keyboard_shortcuts.switch_to_review_view,
        strings(&["cmd-2", "ctrl-2"])
    );
    assert_eq!(
        config.keyboard_shortcuts.switch_to_graph_view,
        strings(&["cmd-3", "ctrl-3"])
    );
    assert_eq!(
        config.keyboard_shortcuts.open_settings,
        strings(&["cmd-,", "ctrl-,"])
    );
}

#[test]
fn app_config_parses_review_provider_mappings() {
    let raw = r#"
[[review_provider_mappings]]
host = "git.company.internal"
provider = "gitlab"

[[review_provider_mappings]]
host = "*.forge.corp.example"
provider = "github"
"#;
    let config: AppConfig = toml::from_str(raw).expect("review provider mappings should parse");

    assert_eq!(config.review_provider_mappings.len(), 2);
    assert_eq!(
        config.review_provider_mappings[0].host,
        "git.company.internal"
    );
    assert_eq!(
        config.review_provider_mappings[0].provider,
        ReviewProviderKind::GitLab
    );
    assert_eq!(
        config.review_provider_mappings[1].host,
        "*.forge.corp.example"
    );
    assert_eq!(
        config.review_provider_mappings[1].provider,
        ReviewProviderKind::GitHub
    );
}
