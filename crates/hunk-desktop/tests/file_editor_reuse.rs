#[path = "../src/app/controller/editor_reuse.rs"]
mod editor_reuse;

use editor_reuse::{LoadedFileEditorReuseState, should_reuse_loaded_file_editor};

#[test]
fn loaded_file_editor_reuse_requires_matching_path_without_errors() {
    let matching_loaded = LoadedFileEditorReuseState {
        requested_path: "src/main.rs",
        current_editor_path: Some("src/main.rs"),
        editor_loading: false,
        editor_error: None,
        has_document: true,
    };
    assert!(should_reuse_loaded_file_editor(matching_loaded));

    let matching_in_flight = LoadedFileEditorReuseState {
        requested_path: "src/main.rs",
        current_editor_path: Some("src/main.rs"),
        editor_loading: true,
        editor_error: None,
        has_document: false,
    };
    assert!(should_reuse_loaded_file_editor(matching_in_flight));

    let different_path = LoadedFileEditorReuseState {
        requested_path: "src/lib.rs",
        ..matching_loaded
    };
    assert!(!should_reuse_loaded_file_editor(different_path));

    let failed_load = LoadedFileEditorReuseState {
        editor_error: Some("boom"),
        ..matching_loaded
    };
    assert!(!should_reuse_loaded_file_editor(failed_load));

    let empty_session = LoadedFileEditorReuseState {
        editor_loading: false,
        has_document: false,
        ..matching_loaded
    };
    assert!(!should_reuse_loaded_file_editor(empty_session));
}
