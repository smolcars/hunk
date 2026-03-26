use super::theme::hunk_opacity;
use gpui::{
    AnyElement, App, IntoElement as _, ParentElement as _, SharedString, Styled as _, div,
    prelude::FluentBuilder as _,
};
use gpui_component::{ActiveTheme as _, StyledExt as _, h_flex, v_flex};
use hunk_git::worktree::{WorkspaceTargetKind, WorkspaceTargetSummary};

use super::hunk_picker::{HunkPickerDelegate, HunkPickerItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceTargetPickerItem {
    title: SharedString,
    value: String,
    normalized_search_text: String,
    detail: SharedString,
    branch_detail: SharedString,
    is_active: bool,
    managed: bool,
    kind: WorkspaceTargetKind,
}

impl WorkspaceTargetPickerItem {
    fn from_target(target: &WorkspaceTargetSummary) -> Self {
        let detail = workspace_target_detail_label(target);
        let search_text = workspace_target_search_text(target);
        let branch_detail = workspace_target_branch_label(target);

        Self {
            title: SharedString::from(target.display_name.clone()),
            value: target.id.clone(),
            normalized_search_text: normalize_workspace_target_key(search_text.as_str()),
            detail: SharedString::from(detail),
            branch_detail: SharedString::from(branch_detail),
            is_active: target.is_active,
            managed: target.managed,
            kind: target.kind,
        }
    }
}

impl HunkPickerItem for WorkspaceTargetPickerItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn render(&self, cx: &mut App) -> AnyElement {
        let is_dark = cx.theme().mode.is_dark();
        let title_color = hunk_opacity(cx.theme().foreground, is_dark, 0.98, 0.98);
        let detail_color = hunk_opacity(cx.theme().foreground, is_dark, 0.74, 0.82);
        let branch_color = hunk_opacity(cx.theme().foreground, is_dark, 0.82, 0.88);
        let badge_text = hunk_opacity(cx.theme().foreground, is_dark, 0.72, 0.82);
        let badge_border = hunk_opacity(cx.theme().border, is_dark, 0.92, 0.78);
        let active_border = hunk_opacity(cx.theme().border, is_dark, 0.98, 0.86);
        let active_background = hunk_opacity(cx.theme().secondary, is_dark, 0.72, 0.82);

        h_flex()
            .w_full()
            .items_start()
            .justify_between()
            .gap_2()
            .child(
                v_flex()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        div()
                            .truncate()
                            .text_color(title_color)
                            .child(self.title.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(detail_color)
                            .child(self.detail.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(branch_color)
                            .child(self.branch_detail.clone()),
                    ),
            )
            .child(
                h_flex()
                    .flex_none()
                    .items_start()
                    .gap_1()
                    .flex_wrap()
                    .pt_0p5()
                    .when(self.managed, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .px_1p5()
                                .py_0p5()
                                .rounded_full()
                                .border_1()
                                .border_color(badge_border)
                                .text_color(badge_text)
                                .child("Managed"),
                        )
                    })
                    .when(self.kind == WorkspaceTargetKind::PrimaryCheckout, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .px_1p5()
                                .py_0p5()
                                .rounded_full()
                                .border_1()
                                .border_color(badge_border)
                                .text_color(badge_text)
                                .child("Primary"),
                        )
                    })
                    .when(self.is_active, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .px_1p5()
                                .py_0p5()
                                .rounded_full()
                                .border_1()
                                .border_color(active_border)
                                .bg(active_background)
                                .text_color(title_color)
                                .child("Active"),
                        )
                    }),
            )
            .into_any_element()
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WorkspaceTargetPickerDelegate {
    items: Vec<WorkspaceTargetPickerItem>,
    matched_items: Vec<WorkspaceTargetPickerItem>,
}

impl WorkspaceTargetPickerDelegate {
    fn new(items: Vec<WorkspaceTargetPickerItem>) -> Self {
        Self {
            matched_items: items.clone(),
            items,
        }
    }
}

impl HunkPickerDelegate for WorkspaceTargetPickerDelegate {
    type Item = WorkspaceTargetPickerItem;

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
        self.matched_items = matched_workspace_target_items(&self.items, query);
    }
}

pub(crate) fn build_workspace_target_picker_delegate(
    targets: &[WorkspaceTargetSummary],
) -> WorkspaceTargetPickerDelegate {
    let items = targets
        .iter()
        .map(WorkspaceTargetPickerItem::from_target)
        .collect::<Vec<_>>();
    WorkspaceTargetPickerDelegate::new(items)
}

pub(crate) fn workspace_target_picker_selected_index(
    targets: &[WorkspaceTargetSummary],
    active_target_id: Option<&str>,
) -> Option<usize> {
    active_target_id.and_then(|active_target_id| {
        targets
            .iter()
            .position(|target| target.id == active_target_id)
    })
}

fn matched_workspace_target_items(
    items: &[WorkspaceTargetPickerItem],
    query: &str,
) -> Vec<WorkspaceTargetPickerItem> {
    let query = normalize_workspace_target_key(query);
    if query.is_empty() {
        return items.to_vec();
    }

    let mut matched = items
        .iter()
        .filter_map(|item| {
            workspace_target_match_score(query.as_str(), item.normalized_search_text.as_str())
                .map(|score| (score, item.clone()))
        })
        .collect::<Vec<_>>();
    matched.sort_by(|(left_score, left_item), (right_score, right_item)| {
        right_score
            .cmp(left_score)
            .then_with(|| left_item.title.cmp(&right_item.title))
    });
    matched.into_iter().map(|(_, item)| item).collect()
}

fn workspace_target_match_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    if candidate == query {
        return Some(10_000);
    }
    if candidate.starts_with(query) {
        return Some(8_000 - (candidate.len() as i32 - query.len() as i32).max(0));
    }
    candidate.find(query).map(|position| {
        6_000 - (position as i32 * 8) - (candidate.len() as i32 - query.len() as i32).max(0)
    })
}

fn normalize_workspace_target_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn workspace_target_detail_label(target: &WorkspaceTargetSummary) -> String {
    match target.kind {
        WorkspaceTargetKind::PrimaryCheckout => "Primary checkout".to_string(),
        WorkspaceTargetKind::LinkedWorktree if target.managed => {
            format!("Managed worktree • {}", target.name)
        }
        WorkspaceTargetKind::LinkedWorktree => format!("Linked worktree • {}", target.name),
    }
}

fn workspace_target_branch_label(target: &WorkspaceTargetSummary) -> String {
    if is_detached_workspace_target_branch(target.branch_name.as_str()) {
        "Detached HEAD".to_string()
    } else {
        format!("Branch {}", target.branch_name)
    }
}

fn workspace_target_search_text(target: &WorkspaceTargetSummary) -> String {
    match target.kind {
        WorkspaceTargetKind::PrimaryCheckout => {
            format!("{} {}", target.display_name, target.branch_name)
        }
        WorkspaceTargetKind::LinkedWorktree
            if is_detached_workspace_target_branch(target.branch_name.as_str()) =>
        {
            target.name.clone()
        }
        WorkspaceTargetKind::LinkedWorktree => target.branch_name.clone(),
    }
}

fn is_detached_workspace_target_branch(branch_name: &str) -> bool {
    matches!(branch_name, "detached" | "unborn")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn managed_target(name: &str, display_name: &str, branch_name: &str) -> WorkspaceTargetSummary {
        WorkspaceTargetSummary {
            id: format!("worktree:{name}"),
            kind: WorkspaceTargetKind::LinkedWorktree,
            root: std::path::PathBuf::from(format!("/tmp/{name}")),
            name: name.to_string(),
            display_name: display_name.to_string(),
            branch_name: branch_name.to_string(),
            managed: true,
            is_active: false,
        }
    }

    #[test]
    fn managed_worktree_searches_by_branch_name_when_attached() {
        let items = vec![WorkspaceTargetPickerItem::from_target(&managed_target(
            "worktree-3",
            "feature/faster-picker",
            "feature/faster-picker",
        ))];

        let by_branch = matched_workspace_target_items(&items, "feature/faster-picker");
        let by_name = matched_workspace_target_items(&items, "worktree-3");

        assert_eq!(by_branch.len(), 1);
        assert!(by_name.is_empty());
    }

    #[test]
    fn detached_worktree_searches_by_worktree_name() {
        let items = vec![WorkspaceTargetPickerItem::from_target(&managed_target(
            "worktree-4",
            "worktree-4",
            "detached",
        ))];

        let by_name = matched_workspace_target_items(&items, "worktree-4");

        assert_eq!(by_name.len(), 1);
    }

    #[test]
    fn primary_checkout_uses_separate_detail_and_branch_lines() {
        let target = WorkspaceTargetSummary {
            id: "primary".to_string(),
            kind: WorkspaceTargetKind::PrimaryCheckout,
            root: std::path::PathBuf::from("/tmp/repo"),
            name: "repo".to_string(),
            display_name: "Primary Checkout".to_string(),
            branch_name: "master".to_string(),
            managed: false,
            is_active: true,
        };

        let item = WorkspaceTargetPickerItem::from_target(&target);

        assert_eq!(item.detail.as_ref(), "Primary checkout");
        assert_eq!(item.branch_detail.as_ref(), "Branch master");
    }

    #[test]
    fn detached_managed_worktree_uses_detached_head_label() {
        let item = WorkspaceTargetPickerItem::from_target(&managed_target(
            "worktree-9",
            "worktree-9",
            "detached",
        ));

        assert_eq!(item.detail.as_ref(), "Managed worktree • worktree-9");
        assert_eq!(item.branch_detail.as_ref(), "Detached HEAD");
    }
}
