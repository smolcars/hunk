use gpui::{AnyElement, App, IntoElement as _, ParentElement as _, SharedString, Styled as _, div};
use gpui_component::{ActiveTheme as _, StyledExt as _, h_flex, v_flex};
use hunk_git::git::LocalBranch;

use super::fuzzy_match::{is_match_boundary, segment_prefix_position, subsequence_match_score};
use super::hunk_picker::{HunkPickerDelegate, HunkPickerItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BranchPickerItem {
    name: SharedString,
    value: String,
    normalized_name: String,
    detail: SharedString,
    tip_unix_time: Option<i64>,
    is_current: bool,
}

impl BranchPickerItem {
    fn from_branch(branch: &LocalBranch) -> Self {
        Self {
            name: SharedString::from(branch.name.clone()),
            value: branch.name.clone(),
            normalized_name: normalize_branch_key(branch.name.as_str()),
            detail: SharedString::from(branch_detail_label(branch)),
            tip_unix_time: branch.tip_unix_time,
            is_current: branch.is_current,
        }
    }
}

impl HunkPickerItem for BranchPickerItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.name.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn render(&self, cx: &mut App) -> AnyElement {
        let detail_color = cx.theme().muted_foreground;
        let current_color = cx.theme().foreground;

        let mut row = h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                v_flex()
                    .min_w_0()
                    .gap_0p5()
                    .child(div().truncate().child(self.name.clone()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(detail_color)
                            .child(self.detail.clone()),
                    ),
            );

        if self.is_current {
            row = row.child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(current_color)
                    .child("Current"),
            );
        }

        row.into_any_element()
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BranchPickerDelegate {
    items: Vec<BranchPickerItem>,
    matched_items: Vec<BranchPickerItem>,
}

impl BranchPickerDelegate {
    fn new(items: Vec<BranchPickerItem>) -> Self {
        Self {
            matched_items: items.clone(),
            items,
        }
    }
}

impl HunkPickerDelegate for BranchPickerDelegate {
    type Item = BranchPickerItem;

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
        self.matched_items = matched_branch_items(&self.items, query);
    }
}

pub(crate) fn build_branch_picker_delegate(branches: &[LocalBranch]) -> BranchPickerDelegate {
    let items = branches
        .iter()
        .map(BranchPickerItem::from_branch)
        .collect::<Vec<_>>();
    BranchPickerDelegate::new(items)
}

pub(crate) fn branch_picker_selected_index(
    branches: &[LocalBranch],
    active_branch_name: Option<&str>,
) -> Option<usize> {
    if let Some(active_branch_name) = active_branch_name {
        return branches
            .iter()
            .position(|branch| branch.name == active_branch_name);
    }

    branches.iter().position(|branch| branch.is_current)
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn matched_branch_names(branches: &[LocalBranch], query: &str) -> Vec<String> {
    matched_branch_items(
        &branches
            .iter()
            .map(BranchPickerItem::from_branch)
            .collect::<Vec<_>>(),
        query,
    )
    .into_iter()
    .map(|item| item.value)
    .collect()
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn branch_detail_labels(branches: &[LocalBranch]) -> Vec<String> {
    branches.iter().map(branch_detail_label).collect()
}

pub(crate) fn branch_match_score(query: &str, candidate: &str) -> Option<i32> {
    let query = normalize_branch_key(query);
    if query.is_empty() {
        return Some(0);
    }

    let candidate = normalize_branch_key(candidate);
    if candidate.is_empty() {
        return None;
    }

    let mut best_score = None;

    if candidate == query {
        best_score = Some(10_000);
    }

    if candidate.starts_with(query.as_str()) {
        let score = 8_000 - (candidate.len() as i32 - query.len() as i32).max(0);
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    if let Some(position) = candidate.find(query.as_str()) {
        let boundary_bonus =
            if position == 0 || is_match_boundary(candidate.as_bytes()[position - 1]) {
                200
            } else {
                0
            };
        let score = 6_000 + boundary_bonus
            - (position as i32 * 12)
            - (candidate.len() as i32 - query.len() as i32);
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    if let Some(position) = segment_prefix_position(candidate.as_str(), query.as_str()) {
        let score =
            7_000 - (position as i32 * 8) - (candidate.len() as i32 - query.len() as i32).max(0);
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    if let Some(score) = subsequence_match_score(candidate.as_str(), query.as_str()) {
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    best_score
}

fn matched_branch_items(items: &[BranchPickerItem], query: &str) -> Vec<BranchPickerItem> {
    let query = normalize_branch_key(query);
    if query.is_empty() {
        return items.to_vec();
    }

    let mut ranked = items
        .iter()
        .filter_map(|item| {
            branch_match_score(query.as_str(), item.normalized_name.as_str()).map(|score| {
                (
                    score,
                    item.is_current,
                    item.tip_unix_time.unwrap_or(i64::MIN),
                    item.value.as_str(),
                    item.clone(),
                )
            })
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| left.3.cmp(right.3))
    });

    ranked.into_iter().map(|(_, _, _, _, item)| item).collect()
}

fn branch_detail_label(branch: &LocalBranch) -> String {
    let relative_time = relative_time_label(branch.tip_unix_time);
    match (
        branch.is_current,
        branch.attached_workspace_target_label.as_deref(),
    ) {
        (false, Some(workspace_target_label)) => {
            format!("Checked out in {workspace_target_label} • {relative_time}")
        }
        _ => relative_time,
    }
}

fn normalize_branch_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn relative_time_label(unix_time: Option<i64>) -> String {
    let Some(unix_time) = unix_time else {
        return "unknown".to_string();
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(unix_time);

    let elapsed = now.saturating_sub(unix_time).max(0);

    if elapsed < 60 {
        format!("{}s ago", elapsed)
    } else if elapsed < 60 * 60 {
        format!("{}m ago", elapsed / 60)
    } else if elapsed < 60 * 60 * 24 {
        format!("{}h ago", elapsed / (60 * 60))
    } else {
        format!("{}d ago", elapsed / (60 * 60 * 24))
    }
}
