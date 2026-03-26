use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, App, IntoElement as _, ParentElement as _, SharedString, Styled as _, div,
    prelude::FluentBuilder as _,
};
use gpui_component::{ActiveTheme as _, StyledExt as _, h_flex, v_flex};

use super::hunk_picker::{HunkPickerDelegate, HunkPickerItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectPickerItem {
    title: SharedString,
    value: String,
    normalized_search_text: String,
    is_active: bool,
}

impl ProjectPickerItem {
    fn from_project_path(project_path: &Path, active_project_path: Option<&Path>) -> Self {
        let title = project_display_name(project_path);
        let detail = project_path.display().to_string();
        let search_text = format!("{title} {detail}");

        Self {
            title: SharedString::from(title),
            value: project_path.to_string_lossy().to_string(),
            normalized_search_text: search_text.to_lowercase(),
            is_active: active_project_path == Some(project_path),
        }
    }
}

impl HunkPickerItem for ProjectPickerItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn render(&self, cx: &mut App) -> AnyElement {
        let active_color = cx.theme().foreground;

        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                v_flex()
                    .min_w_0()
                    .child(div().truncate().child(self.title.clone())),
            )
            .when(self.is_active, |this| {
                this.child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(active_color)
                        .child("Active"),
                )
            })
            .into_any_element()
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ProjectPickerDelegate {
    items: Vec<ProjectPickerItem>,
    matched_items: Vec<ProjectPickerItem>,
}

impl ProjectPickerDelegate {
    fn new(items: Vec<ProjectPickerItem>) -> Self {
        Self {
            matched_items: items.clone(),
            items,
        }
    }
}

impl HunkPickerDelegate for ProjectPickerDelegate {
    type Item = ProjectPickerItem;

    fn items_count(&self) -> usize {
        self.matched_items.len()
    }

    fn item(&self, ix: usize) -> Option<&Self::Item> {
        self.matched_items.get(ix)
    }

    fn position<V>(&self, value: &V) -> Option<usize>
    where
        Self::Item: HunkPickerItem<Value = V>,
        V: PartialEq,
    {
        self.matched_items
            .iter()
            .position(|item| item.value() == value)
    }

    fn perform_search(&mut self, query: &str) {
        let query = query.trim().to_lowercase();
        self.matched_items = if query.is_empty() {
            self.items.clone()
        } else {
            self.items
                .iter()
                .filter(|item| item.normalized_search_text.contains(query.as_str()))
                .cloned()
                .collect()
        };
    }
}

pub(crate) fn build_project_picker_delegate(
    project_paths: &[PathBuf],
    active_project_path: Option<&Path>,
) -> ProjectPickerDelegate {
    let items = project_paths
        .iter()
        .map(|project_path| {
            ProjectPickerItem::from_project_path(project_path.as_path(), active_project_path)
        })
        .collect::<Vec<_>>();
    ProjectPickerDelegate::new(items)
}

pub(crate) fn project_picker_selected_index(
    project_paths: &[PathBuf],
    active_project_path: Option<&Path>,
) -> Option<usize> {
    active_project_path.and_then(|active_project_path| {
        project_paths
            .iter()
            .position(|project_path| project_path.as_path() == active_project_path)
    })
}

pub(crate) fn project_display_name(project_path: &Path) -> String {
    project_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| project_path.display().to_string())
}
