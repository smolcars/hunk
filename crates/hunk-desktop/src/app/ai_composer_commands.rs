use hunk_domain::state::AiCollaborationModeSelection;

use crate::app::ActivePrefixedToken;

use super::fuzzy_match::subsequence_match_score;

const AI_COMPOSER_SLASH_COMMANDS: [AiComposerSlashCommandItem; 6] = [
    AiComposerSlashCommandItem::new(
        AiComposerSlashCommandKind::Code,
        "code",
        "Code",
        "Switch to standard coding mode.",
    ),
    AiComposerSlashCommandItem::new(
        AiComposerSlashCommandKind::Plan,
        "plan",
        "Plan",
        "Switch to planning mode before coding.",
    ),
    AiComposerSlashCommandItem::new(
        AiComposerSlashCommandKind::Review,
        "review",
        "Review",
        "Switch the composer into diff review mode.",
    ),
    AiComposerSlashCommandItem::new(
        AiComposerSlashCommandKind::Usage,
        "usage",
        "Usage",
        "Show remaining 5h and 7d usage limits.",
    ),
    AiComposerSlashCommandItem::new(
        AiComposerSlashCommandKind::Login,
        "login",
        "Login",
        "Start ChatGPT login for this workspace.",
    ),
    AiComposerSlashCommandItem::new(
        AiComposerSlashCommandKind::Logout,
        "logout",
        "Logout",
        "Disconnect the current account.",
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiComposerSlashCommandKind {
    Code,
    Plan,
    Review,
    Usage,
    Login,
    Logout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AiComposerSlashCommandItem {
    pub(crate) kind: AiComposerSlashCommandKind,
    pub(crate) name: &'static str,
    pub(crate) label: &'static str,
    pub(crate) description: &'static str,
}

impl AiComposerSlashCommandItem {
    const fn new(
        kind: AiComposerSlashCommandKind,
        name: &'static str,
        label: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            kind,
            name,
            label,
            description,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiComposerSlashCommandMenuState {
    pub(crate) query: String,
    pub(crate) replace_range: std::ops::Range<usize>,
    pub(crate) items: Vec<AiComposerSlashCommandItem>,
}

pub(crate) fn active_slash_command_token(
    text: &str,
    cursor_offset: usize,
) -> Option<ActivePrefixedToken> {
    let safe_cursor = clamp_to_char_boundary(text, cursor_offset);
    let leading_ws_len = text
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .map(char::len_utf8)
        .sum::<usize>();
    if safe_cursor < leading_ws_len {
        return None;
    }

    let token_end = leading_ws_len
        + text[leading_ws_len..]
            .char_indices()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(index, _)| index)
            .unwrap_or(text.len().saturating_sub(leading_ws_len));
    if token_end <= leading_ws_len || safe_cursor > token_end {
        return None;
    }

    let token = &text[leading_ws_len..token_end];
    if !token.starts_with('/') {
        return None;
    }

    let query = &token['/'.len_utf8()..];
    if !query.is_empty()
        && query
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '-'))
    {
        return None;
    }

    Some(ActivePrefixedToken {
        query: query.to_string(),
        replace_range: leading_ws_len..token_end,
    })
}

pub(crate) fn slash_command_menu_state(
    text: &str,
    cursor_offset: usize,
) -> Option<AiComposerSlashCommandMenuState> {
    let active_token = active_slash_command_token(text, cursor_offset)?;
    let items = matched_slash_commands(active_token.query.as_str());
    if items.is_empty() {
        return None;
    }

    Some(AiComposerSlashCommandMenuState {
        query: active_token.query,
        replace_range: active_token.replace_range,
        items,
    })
}

pub(crate) fn ai_composer_mode_label(
    review_mode_active: bool,
    collaboration_mode: AiCollaborationModeSelection,
) -> &'static str {
    if review_mode_active {
        "Review"
    } else {
        collaboration_mode.label()
    }
}

pub(crate) fn prompt_after_accepting_slash_command(
    prompt: &str,
    replace_range: &std::ops::Range<usize>,
) -> String {
    let mut next = String::with_capacity(prompt.len().saturating_sub(replace_range.len()));
    next.push_str(&prompt[..replace_range.start]);
    next.push_str(&prompt[replace_range.end..]);
    next.trim_start().to_string()
}

pub(crate) fn slash_command_items() -> &'static [AiComposerSlashCommandItem] {
    &AI_COMPOSER_SLASH_COMMANDS
}

fn matched_slash_commands(query: &str) -> Vec<AiComposerSlashCommandItem> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return slash_command_items().to_vec();
    }

    let normalized_query = trimmed.to_ascii_lowercase();
    let mut ranked = slash_command_items()
        .iter()
        .filter_map(|item| {
            let label_key = item.label.to_ascii_lowercase();
            let name_key = item.name.to_ascii_lowercase();
            let description_key = item.description.to_ascii_lowercase();
            let label_score =
                subsequence_match_score(label_key.as_str(), normalized_query.as_str());
            let name_score = subsequence_match_score(name_key.as_str(), normalized_query.as_str());
            let label_prefix = label_key.starts_with(normalized_query.as_str());
            let name_prefix = name_key.starts_with(normalized_query.as_str());
            let description_match = description_key.contains(normalized_query.as_str());
            if label_score.is_none() && name_score.is_none() && !description_match {
                return None;
            }
            Some((
                *item,
                label_score.unwrap_or(0),
                name_score.unwrap_or(0),
                label_prefix,
                name_prefix,
                description_match,
            ))
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| {
        right
            .3
            .cmp(&left.3)
            .then_with(|| right.4.cmp(&left.4))
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| right.5.cmp(&left.5))
            .then_with(|| left.0.name.cmp(right.0.name))
    });

    ranked.into_iter().map(|(item, ..)| item).collect()
}

fn clamp_to_char_boundary(text: &str, cursor_offset: usize) -> usize {
    let mut safe_cursor = cursor_offset.min(text.len());
    while safe_cursor > 0 && !text.is_char_boundary(safe_cursor) {
        safe_cursor = safe_cursor.saturating_sub(1);
    }
    safe_cursor
}

#[cfg(test)]
mod tests {
    use super::{
        AiComposerSlashCommandKind, active_slash_command_token, ai_composer_mode_label,
        prompt_after_accepting_slash_command, slash_command_menu_state,
    };
    use crate::app::ActivePrefixedToken;
    use hunk_domain::state::AiCollaborationModeSelection;

    #[test]
    fn slash_command_token_tracks_first_token_only() {
        assert_eq!(
            active_slash_command_token("/plan", 5),
            Some(ActivePrefixedToken {
                query: "plan".to_string(),
                replace_range: 0..5,
            })
        );
        assert_eq!(
            active_slash_command_token("   /review more", 10),
            Some(ActivePrefixedToken {
                query: "review".to_string(),
                replace_range: 3..10,
            })
        );
        assert_eq!(active_slash_command_token("use /plan", 9), None);
        assert_eq!(active_slash_command_token("/Volumes/hulk", 13), None);
    }

    #[test]
    fn slash_command_menu_matches_on_name_and_description() {
        let menu = slash_command_menu_state("/us", 3).expect("menu should exist");
        assert_eq!(menu.items[0].kind, AiComposerSlashCommandKind::Usage);

        let menu = slash_command_menu_state("/disconnect", 11).expect("menu should exist");
        assert_eq!(menu.items[0].kind, AiComposerSlashCommandKind::Logout);
    }

    #[test]
    fn accepting_slash_command_removes_command_token() {
        assert_eq!(prompt_after_accepting_slash_command("/plan", &(0..5)), "");
        assert_eq!(
            prompt_after_accepting_slash_command("/plan investigate this", &(0..5)),
            "investigate this"
        );
        assert_eq!(
            prompt_after_accepting_slash_command("   /review compare this diff", &(3..10)),
            "compare this diff"
        );
    }

    #[test]
    fn composer_mode_label_uses_code_for_default_and_review_override() {
        assert_eq!(
            ai_composer_mode_label(false, AiCollaborationModeSelection::Default),
            "Code"
        );
        assert_eq!(
            ai_composer_mode_label(false, AiCollaborationModeSelection::Plan),
            "Plan"
        );
        assert_eq!(
            ai_composer_mode_label(true, AiCollaborationModeSelection::Default),
            "Review"
        );
    }
}
