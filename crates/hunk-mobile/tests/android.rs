use hunk_mobile::{
    AndroidAction, AndroidKey, AndroidTapTarget, MobileError, MobileSession, MobileSessionId,
    classify_android_action, parse_adb_devices, parse_android_input_text, parse_avd_list,
    parse_ui_automator_snapshot,
};

#[test]
fn adb_devices_parser_extracts_emulator_metadata() {
    let devices = parse_adb_devices(
        r#"List of devices attached
emulator-5554 device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64 device:emu64a transport_id:1
0123456789 offline usb:338690048X
"#,
    );

    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].serial.as_str(), "emulator-5554");
    assert_eq!(devices[0].state, "device");
    assert!(devices[0].is_emulator);
    assert_eq!(
        devices[0].details.get("model").map(String::as_str),
        Some("sdk_gphone64_arm64")
    );
    assert_eq!(devices[1].serial.as_str(), "0123456789");
    assert_eq!(devices[1].state, "offline");
    assert!(!devices[1].is_emulator);
}

#[test]
fn avd_list_parser_ignores_blank_lines() {
    let avds = parse_avd_list("\nPixel_8_API_35\n\nMedium_Phone_API_36\n");

    assert_eq!(
        avds.iter().map(|avd| avd.name.as_str()).collect::<Vec<_>>(),
        vec!["Pixel_8_API_35", "Medium_Phone_API_36"]
    );
}

#[test]
fn ui_automator_snapshot_parser_indexes_visible_elements() {
    let snapshot = parse_ui_automator_snapshot(
        r#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<hierarchy rotation="0">
  <node index="0" text="" resource-id="" class="android.widget.FrameLayout" package="com.example" content-desc="" bounds="[0,0][1080,1920]" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" selected="false" checked="false">
    <node index="0" text="Welcome" resource-id="com.example:id/title" class="android.widget.TextView" package="com.example" content-desc="" bounds="[32,64][400,128]" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" selected="false" checked="false" />
    <node index="1" text="" resource-id="com.example:id/login" class="android.widget.Button" package="com.example" content-desc="Log in" bounds="[32,160][320,240]" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" selected="false" checked="false" />
    <node index="2" text="" resource-id="" class="android.view.View" package="com.example" content-desc="" bounds="[0,0][0,0]" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" selected="false" checked="false" />
  </node>
</hierarchy>"#,
        7,
    )
    .expect("snapshot should parse");

    assert_eq!(snapshot.epoch, 7);
    assert_eq!(snapshot.viewport.width, 1080);
    assert_eq!(snapshot.viewport.height, 1920);
    assert_eq!(snapshot.elements.len(), 2);
    assert_eq!(snapshot.elements[0].index, 0);
    assert_eq!(snapshot.elements[0].role, "text");
    assert_eq!(snapshot.elements[0].label, "Welcome");
    assert_eq!(snapshot.elements[1].index, 1);
    assert_eq!(snapshot.elements[1].role, "button");
    assert_eq!(snapshot.elements[1].label, "Log in");
    assert!(snapshot.elements[1].clickable);
    assert_eq!(snapshot.elements[1].rect.center().x, 176);
    assert_eq!(snapshot.elements[1].rect.center().y, 200);
}

#[test]
fn mobile_session_rejects_stale_snapshot_indexes() {
    let snapshot = parse_ui_automator_snapshot(
        r#"<hierarchy><node text="Tap" resource-id="" class="android.widget.Button" package="com.example" content-desc="" bounds="[0,0][100,100]" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" selected="false" checked="false" /></hierarchy>"#,
        3,
    )
    .expect("snapshot should parse");
    let mut session = MobileSession::new(MobileSessionId::new("thread"));
    session.replace_snapshot(snapshot);

    assert!(session.validate_snapshot_element(3, 0).is_ok());
    assert_eq!(
        session.validate_snapshot_element(2, 0),
        Err(MobileError::StaleSnapshot {
            expected: 3,
            received: 2,
        })
    );
    assert_eq!(
        session.validate_snapshot_element(3, 99),
        Err(MobileError::UnknownElementIndex(99))
    );
}

#[test]
fn android_input_text_encoder_handles_simple_shell_sensitive_text() {
    let encoded = parse_android_input_text("hello world & ok")
        .expect("simple text should encode")
        .encoded;

    assert_eq!(encoded, "hello%sworld%s\\&%sok");
    assert!(parse_android_input_text("line\nbreak").is_err());
}

#[test]
fn android_key_maps_common_names_to_keyevents() {
    assert_eq!(AndroidKey::Back.keyevent_arg().as_deref(), Ok("4"));
    assert_eq!(AndroidKey::Home.keyevent_arg().as_deref(), Ok("3"));
    assert_eq!(
        AndroidKey::Raw("KEYCODE_BACK".to_string())
            .keyevent_arg()
            .as_deref(),
        Ok("KEYCODE_BACK")
    );
    assert!(
        AndroidKey::Raw("bad key".to_string())
            .keyevent_arg()
            .is_err()
    );
}

#[test]
fn android_safety_prompts_for_likely_secret_text() {
    let action = AndroidAction::Type {
        snapshot_epoch: None,
        index: None,
        text: "sk-abc123456789".to_string(),
        clear: false,
    };

    assert!(matches!(
        classify_android_action(&action),
        hunk_mobile::MobileSafetyDecision::Prompt(_)
    ));
    assert!(matches!(
        classify_android_action(&AndroidAction::Tap {
            target: AndroidTapTarget::Point { x: 1, y: 2 },
        }),
        hunk_mobile::MobileSafetyDecision::Allow
    ));
}
