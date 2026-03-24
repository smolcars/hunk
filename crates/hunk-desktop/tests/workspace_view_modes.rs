#[path = "../src/app/workspace_view.rs"]
mod workspace_view;

use workspace_view::{
    SHORTCUT_CONTEXT_AI_WORKSPACE, SHORTCUT_CONTEXT_FILES_WORKSPACE,
    SHORTCUT_CONTEXT_GIT_WORKSPACE, SHORTCUT_CONTEXT_REVIEW_WORKSPACE,
    SHORTCUT_CONTEXT_SELECTABLE_WORKSPACE, SHORTCUT_CONTEXT_TREE_WORKSPACE, WorkspaceSwitchAction,
    WorkspaceViewMode,
};

#[test]
fn mode_switching_keeps_existing_tabs_and_adds_ai_as_fourth_tab() {
    let tabs = [
        WorkspaceViewMode::Files,
        WorkspaceViewMode::Diff,
        WorkspaceViewMode::GitWorkspace,
        WorkspaceViewMode::Ai,
    ];
    assert_eq!(tabs[0], WorkspaceViewMode::Files);
    assert_eq!(tabs[1], WorkspaceViewMode::Diff);
    assert_eq!(tabs[2], WorkspaceViewMode::GitWorkspace);
    assert_eq!(tabs[3], WorkspaceViewMode::Ai);
}

#[test]
fn ai_controller_switch_action_targets_ai_mode() {
    assert_eq!(
        WorkspaceSwitchAction::Ai.target_mode(),
        WorkspaceViewMode::Ai
    );
    assert_eq!(
        WorkspaceSwitchAction::Files.target_mode(),
        WorkspaceViewMode::Files
    );
    assert_eq!(
        WorkspaceSwitchAction::Review.target_mode(),
        WorkspaceViewMode::Diff
    );
    assert_eq!(
        WorkspaceSwitchAction::Git.target_mode(),
        WorkspaceViewMode::GitWorkspace
    );
}

#[test]
fn ai_mode_does_not_enable_sidebar_or_diff_stream() {
    assert!(!WorkspaceViewMode::Ai.supports_sidebar_tree());
    assert!(!WorkspaceViewMode::Ai.supports_diff_stream());
    assert!(WorkspaceViewMode::Files.supports_sidebar_tree());
    assert!(WorkspaceViewMode::Files.supports_diff_stream());
    assert!(WorkspaceViewMode::Diff.supports_sidebar_tree());
    assert!(WorkspaceViewMode::Diff.supports_diff_stream());
    assert!(!WorkspaceViewMode::GitWorkspace.supports_sidebar_tree());
    assert!(!WorkspaceViewMode::GitWorkspace.supports_diff_stream());
}

#[test]
fn workspace_modes_expose_distinct_shortcut_contexts() {
    assert_eq!(
        WorkspaceViewMode::Files.shortcut_context(),
        SHORTCUT_CONTEXT_FILES_WORKSPACE
    );
    assert_eq!(
        WorkspaceViewMode::Diff.shortcut_context(),
        SHORTCUT_CONTEXT_REVIEW_WORKSPACE
    );
    assert_eq!(
        WorkspaceViewMode::GitWorkspace.shortcut_context(),
        SHORTCUT_CONTEXT_GIT_WORKSPACE
    );
    assert_eq!(
        WorkspaceViewMode::Ai.shortcut_context(),
        SHORTCUT_CONTEXT_AI_WORKSPACE
    );
}

#[test]
fn root_key_contexts_include_only_the_scopes_each_workspace_needs() {
    assert_eq!(
        WorkspaceViewMode::Files.root_key_context(),
        "DiffViewer FilesWorkspace TreeWorkspace"
    );
    assert_eq!(
        WorkspaceViewMode::Diff.root_key_context(),
        "DiffViewer ReviewWorkspace TreeWorkspace SelectableWorkspace"
    );
    assert_eq!(
        WorkspaceViewMode::GitWorkspace.root_key_context(),
        "DiffViewer GitWorkspace"
    );
    assert_eq!(
        WorkspaceViewMode::Ai.root_key_context(),
        "DiffViewer AiWorkspace SelectableWorkspace"
    );

    assert!(
        WorkspaceViewMode::Files
            .root_key_context()
            .contains(SHORTCUT_CONTEXT_TREE_WORKSPACE)
    );
    assert!(
        !WorkspaceViewMode::Files
            .root_key_context()
            .contains(SHORTCUT_CONTEXT_SELECTABLE_WORKSPACE)
    );
    assert!(
        WorkspaceViewMode::Diff
            .root_key_context()
            .contains(SHORTCUT_CONTEXT_TREE_WORKSPACE)
    );
    assert!(
        WorkspaceViewMode::Diff
            .root_key_context()
            .contains(SHORTCUT_CONTEXT_SELECTABLE_WORKSPACE)
    );
    assert!(
        !WorkspaceViewMode::GitWorkspace
            .root_key_context()
            .contains(SHORTCUT_CONTEXT_TREE_WORKSPACE)
    );
    assert!(
        WorkspaceViewMode::Ai
            .root_key_context()
            .contains(SHORTCUT_CONTEXT_SELECTABLE_WORKSPACE)
    );
}
