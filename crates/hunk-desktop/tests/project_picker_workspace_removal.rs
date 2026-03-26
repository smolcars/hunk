#[allow(dead_code)]
mod hunk_picker {
    use gpui::{AnyElement, App, SharedString};

    pub(crate) trait HunkPickerItem: Clone {
        type Value: Clone + PartialEq;

        fn title(&self) -> SharedString;
        fn value(&self) -> &Self::Value;
        fn display_title(&self) -> Option<AnyElement> {
            None
        }
        fn render(&self, cx: &mut App) -> AnyElement;
    }

    pub(crate) trait HunkPickerDelegate: Clone + Default + 'static {
        type Item: HunkPickerItem;

        fn items_count(&self) -> usize;
        fn item(&self, ix: usize) -> Option<&Self::Item>;
        fn position<V>(&self, value: &V) -> Option<usize>
        where
            Self::Item: HunkPickerItem<Value = V>,
            V: PartialEq;
        fn perform_search(&mut self, query: &str);
    }
}

#[allow(dead_code)]
#[path = "../src/app/project_picker.rs"]
mod project_picker;

use std::path::PathBuf;

use hunk_domain::state::AppState;
use hunk_picker::{HunkPickerDelegate, HunkPickerItem};
use project_picker::{build_project_picker_delegate, project_picker_selected_index};

#[test]
fn project_picker_delegate_drops_removed_active_project_and_selects_next_project() {
    let removed_project = PathBuf::from("/tmp/hunk-repo-a");
    let next_project = PathBuf::from("/tmp/hunk-repo-b");
    let remaining_project = PathBuf::from("/tmp/hunk-repo-c");
    let mut state = AppState {
        workspace_project_paths: vec![
            removed_project.clone(),
            next_project.clone(),
            remaining_project.clone(),
        ],
        active_workspace_project_path: Some(removed_project.clone()),
        ..AppState::default()
    };

    assert!(state.remove_workspace_project(removed_project.as_path()));
    assert_eq!(
        state.active_workspace_project_path.as_ref(),
        Some(&next_project)
    );

    let active_project_path = state.active_project_path().map(PathBuf::as_path);
    let delegate = build_project_picker_delegate(
        state.workspace_project_paths.as_slice(),
        active_project_path,
    );

    assert_eq!(delegate.items_count(), 2);
    let values = (0..delegate.items_count())
        .filter_map(|row| delegate.item(row))
        .map(|item| item.value().clone())
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        vec![
            next_project.to_string_lossy().to_string(),
            remaining_project.to_string_lossy().to_string(),
        ]
    );

    let selected_index = project_picker_selected_index(
        state.workspace_project_paths.as_slice(),
        active_project_path,
    );
    assert_eq!(selected_index, Some(0));
}
