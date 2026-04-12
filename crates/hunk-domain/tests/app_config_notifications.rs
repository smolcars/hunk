use hunk_domain::config::AppConfig;

#[test]
fn desktop_notification_defaults_are_enabled() {
    let config = AppConfig::default();
    assert!(config.desktop_notifications.enabled);
    assert!(config.desktop_notifications.only_when_unfocused);
    assert!(config.desktop_notifications.ai.agent_finished);
    assert!(config.desktop_notifications.ai.plan_ready);
    assert!(config.desktop_notifications.ai.user_input_required);
    assert!(config.desktop_notifications.ai.approval_required);
}

#[test]
fn desktop_notification_config_round_trips_from_toml() {
    let config = toml::from_str::<AppConfig>(
        r#"
theme = "system"
reduce_motion = false
show_fps_counter = true
auto_update_enabled = true

[desktop_notifications]
enabled = true
only_when_unfocused = false

[desktop_notifications.ai]
agent_finished = false
plan_ready = true
user_input_required = true
approval_required = false
"#,
    )
    .expect("config should parse");

    assert!(config.desktop_notifications.enabled);
    assert!(!config.desktop_notifications.only_when_unfocused);
    assert!(!config.desktop_notifications.ai.agent_finished);
    assert!(config.desktop_notifications.ai.plan_ready);
    assert!(config.desktop_notifications.ai.user_input_required);
    assert!(!config.desktop_notifications.ai.approval_required);

    let serialized = toml::to_string_pretty(&config).expect("config should serialize");
    assert!(serialized.contains("[desktop_notifications]"));
    assert!(serialized.contains("[desktop_notifications.ai]"));
}
