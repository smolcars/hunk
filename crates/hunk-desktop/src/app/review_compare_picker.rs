use std::path::PathBuf;

use gpui::{
    App, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Task, Window, div,
};
use gpui_component::{
    ActiveTheme as _, IndexPath,
    select::{SelectDelegate, SelectItem},
    v_flex,
};
use hunk_git::compare::{compare_branch_source_id, compare_workspace_target_source_id};
use hunk_git::git::LocalBranch;
use hunk_git::worktree::{WorkspaceTargetKind, WorkspaceTargetSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReviewCompareSourceKind {
    WorkspaceTarget,
    Branch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewCompareSourceOption {
    pub id: String,
    pub kind: ReviewCompareSourceKind,
    pub display_name: String,
    pub detail: String,
    pub workspace_target_id: Option<String>,
    pub workspace_root: Option<PathBuf>,
    pub branch_name: Option<String>,
}

impl ReviewCompareSourceOption {
    pub(crate) fn from_workspace_target(target: &WorkspaceTargetSummary) -> Self {
        let detail = match target.kind {
            WorkspaceTargetKind::PrimaryCheckout => {
                format!("Primary checkout • {}", target.branch_name)
            }
            WorkspaceTargetKind::LinkedWorktree
                if target.managed
                    && is_detached_workspace_target_branch(target.branch_name.as_str()) =>
            {
                "Managed worktree • detached".to_string()
            }
            WorkspaceTargetKind::LinkedWorktree if target.managed => {
                format!("Managed worktree • {}", target.name)
            }
            WorkspaceTargetKind::LinkedWorktree
                if is_detached_workspace_target_branch(target.branch_name.as_str()) =>
            {
                "Linked worktree • detached".to_string()
            }
            WorkspaceTargetKind::LinkedWorktree => {
                format!("Linked worktree • {}", target.name)
            }
        };
        Self {
            id: compare_workspace_target_source_id(target.id.as_str()),
            kind: ReviewCompareSourceKind::WorkspaceTarget,
            display_name: target.display_name.clone(),
            detail,
            workspace_target_id: Some(target.id.clone()),
            workspace_root: Some(target.root.clone()),
            branch_name: Some(target.branch_name.clone()),
        }
    }

    pub(crate) fn from_branch(branch: &LocalBranch) -> Self {
        Self {
            id: compare_branch_source_id(branch.name.as_str()),
            kind: ReviewCompareSourceKind::Branch,
            display_name: branch.name.clone(),
            detail: if branch.is_current {
                "Local branch • checked out".to_string()
            } else {
                "Local branch".to_string()
            },
            workspace_target_id: None,
            workspace_root: None,
            branch_name: Some(branch.name.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewComparePickerItem {
    title: SharedString,
    value: String,
    normalized_title: String,
    detail: SharedString,
}

impl ReviewComparePickerItem {
    fn from_option(option: &ReviewCompareSourceOption) -> Self {
        Self {
            title: SharedString::from(option.display_name.clone()),
            value: option.id.clone(),
            normalized_title: option.display_name.trim().to_lowercase(),
            detail: SharedString::from(option.detail.clone()),
        }
    }
}

impl SelectItem for ReviewComparePickerItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn render(&self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap_0p5()
            .child(div().truncate().child(self.title.clone()))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.detail.clone()),
            )
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ReviewComparePickerDelegate {
    items: Vec<ReviewComparePickerItem>,
    matched_items: Vec<ReviewComparePickerItem>,
}

impl ReviewComparePickerDelegate {
    fn new(items: Vec<ReviewComparePickerItem>) -> Self {
        Self {
            matched_items: items.clone(),
            items,
        }
    }
}

impl SelectDelegate for ReviewComparePickerDelegate {
    type Item = ReviewComparePickerItem;

    fn items_count(&self, _: usize) -> usize {
        self.matched_items.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&Self::Item> {
        self.matched_items.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        Self::Item: SelectItem<Value = V>,
        V: PartialEq,
    {
        self.matched_items
            .iter()
            .position(|item| item.value() == value)
            .map(|row| IndexPath::default().row(row))
    }

    fn perform_search(
        &mut self,
        query: &str,
        _: &mut Window,
        _: &mut Context<gpui_component::select::SelectState<Self>>,
    ) -> Task<()> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            self.matched_items = self.items.clone();
        } else {
            self.matched_items = self
                .items
                .iter()
                .filter(|item| item.normalized_title.contains(query.as_str()))
                .cloned()
                .collect();
        }
        Task::ready(())
    }
}

pub(crate) fn build_review_compare_picker_delegate(
    options: &[ReviewCompareSourceOption],
) -> ReviewComparePickerDelegate {
    ReviewComparePickerDelegate::new(
        options
            .iter()
            .map(ReviewComparePickerItem::from_option)
            .collect(),
    )
}

pub(crate) fn review_compare_picker_selected_index(
    options: &[ReviewCompareSourceOption],
    selected_id: Option<&str>,
) -> Option<IndexPath> {
    selected_id.and_then(|selected_id| {
        options
            .iter()
            .position(|option| option.id == selected_id)
            .map(|row| IndexPath::default().row(row))
    })
}

fn is_detached_workspace_target_branch(branch_name: &str) -> bool {
    matches!(branch_name, "detached" | "unborn")
}
