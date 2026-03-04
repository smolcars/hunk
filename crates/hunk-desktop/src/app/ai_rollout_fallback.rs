use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;

use serde_json::Value;

const SESSIONS_SUBDIR: &str = "sessions";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolloutFallbackItem {
    pub kind: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolloutFallbackTurn {
    pub turn_id: String,
    pub completed: bool,
    pub items: Vec<RolloutFallbackItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MutableFallbackTurn {
    id: String,
    completed: bool,
    items: Vec<RolloutFallbackItem>,
}

pub fn find_rollout_path_for_thread(
    codex_home: &Path,
    thread_id: &str,
) -> io::Result<Option<PathBuf>> {
    let sessions_dir = codex_home.join(SESSIONS_SUBDIR);
    if !sessions_dir.is_dir() {
        return Ok(None);
    }

    let mut stack = vec![sessions_dir];
    let suffix = format!("-{thread_id}.jsonl");
    let mut latest_match: Option<PathBuf> = None;

    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                continue;
            };
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }

            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if file_name.starts_with("rollout-") && file_name.ends_with(suffix.as_str()) {
                let should_replace = latest_match
                    .as_ref()
                    .and_then(|current| current.file_name())
                    .and_then(|name| name.to_str())
                    .is_none_or(|current| file_name > current);
                if should_replace {
                    latest_match = Some(path);
                }
            }
        }
    }

    Ok(latest_match)
}

pub fn parse_rollout_fallback(path: &Path) -> io::Result<Vec<RolloutFallbackTurn>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    Ok(parse_rollout_fallback_from_reader(reader))
}

fn parse_rollout_fallback_from_reader<R: BufRead>(reader: R) -> Vec<RolloutFallbackTurn> {
    let mut turns = Vec::<MutableFallbackTurn>::new();
    let mut turn_indices = HashMap::<String, usize>::new();
    let mut current_turn_id: Option<String> = None;
    let mut implicit_turn_counter = 1usize;

    for line in reader.lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if value.get("type").and_then(Value::as_str) != Some("event_msg") {
            continue;
        }

        let payload = value.get("payload").and_then(Value::as_object);
        let Some(payload) = payload else {
            continue;
        };

        let event_type = payload
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match event_type {
            "task_started" => {
                let Some(turn_id) = payload.get("turn_id").and_then(Value::as_str) else {
                    continue;
                };
                let turn_id = turn_id.to_string();
                ensure_turn(&mut turns, &mut turn_indices, turn_id.as_str());
                current_turn_id = Some(turn_id);
            }
            "task_complete" => {
                let Some(turn_id) = payload.get("turn_id").and_then(Value::as_str) else {
                    continue;
                };
                let turn_id = turn_id.to_string();
                let turn_index = ensure_turn(&mut turns, &mut turn_indices, turn_id.as_str());
                turns[turn_index].completed = true;
                if let Some(last_agent_message) =
                    payload.get("last_agent_message").and_then(Value::as_str)
                {
                    append_item_if_new(
                        &mut turns[turn_index].items,
                        "agentMessage",
                        last_agent_message,
                    );
                }
                if current_turn_id.as_deref() == Some(turn_id.as_str()) {
                    current_turn_id = None;
                }
            }
            "turn_aborted" => {
                let turn_id = payload
                    .get("turn_id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .or_else(|| current_turn_id.clone());
                let Some(turn_id) = turn_id else {
                    continue;
                };
                let turn_index = ensure_turn(&mut turns, &mut turn_indices, turn_id.as_str());
                turns[turn_index].completed = true;
                if current_turn_id.as_deref() == Some(turn_id.as_str()) {
                    current_turn_id = None;
                }
            }
            "user_message" => {
                let Some(message) = payload.get("message").and_then(Value::as_str) else {
                    continue;
                };
                let turn_id = resolve_turn_id_for_item(
                    &mut turns,
                    &mut turn_indices,
                    &mut current_turn_id,
                    &mut implicit_turn_counter,
                );
                let turn_index = ensure_turn(&mut turns, &mut turn_indices, turn_id.as_str());
                append_item_if_new(&mut turns[turn_index].items, "userMessage", message);
            }
            "agent_message" => {
                let Some(message) = payload.get("message").and_then(Value::as_str) else {
                    continue;
                };
                let turn_id = resolve_turn_id_for_item(
                    &mut turns,
                    &mut turn_indices,
                    &mut current_turn_id,
                    &mut implicit_turn_counter,
                );
                let turn_index = ensure_turn(&mut turns, &mut turn_indices, turn_id.as_str());
                append_item_if_new(&mut turns[turn_index].items, "agentMessage", message);
            }
            "agent_reasoning" => {
                let Some(message) = payload.get("text").and_then(Value::as_str) else {
                    continue;
                };
                let turn_id = resolve_turn_id_for_item(
                    &mut turns,
                    &mut turn_indices,
                    &mut current_turn_id,
                    &mut implicit_turn_counter,
                );
                let turn_index = ensure_turn(&mut turns, &mut turn_indices, turn_id.as_str());
                append_item_if_new(&mut turns[turn_index].items, "reasoning", message);
            }
            _ => {}
        }
    }

    turns
        .into_iter()
        .map(|turn| RolloutFallbackTurn {
            turn_id: turn.id,
            completed: turn.completed,
            items: turn.items,
        })
        .collect()
}

fn ensure_turn(
    turns: &mut Vec<MutableFallbackTurn>,
    indices: &mut HashMap<String, usize>,
    turn_id: &str,
) -> usize {
    if let Some(index) = indices.get(turn_id).copied() {
        return index;
    }

    let index = turns.len();
    indices.insert(turn_id.to_string(), index);
    turns.push(MutableFallbackTurn {
        id: turn_id.to_string(),
        completed: false,
        items: Vec::new(),
    });
    index
}

fn resolve_turn_id_for_item(
    turns: &mut Vec<MutableFallbackTurn>,
    indices: &mut HashMap<String, usize>,
    current_turn_id: &mut Option<String>,
    implicit_turn_counter: &mut usize,
) -> String {
    if let Some(turn_id) = current_turn_id.as_ref() {
        return turn_id.clone();
    }

    if let Some(existing_turn) = turns.iter().rev().find(|turn| !turn.completed) {
        let turn_id = existing_turn.id.clone();
        *current_turn_id = Some(turn_id.clone());
        return turn_id;
    }

    let turn_id = format!("rollout-implicit-{}", *implicit_turn_counter);
    *implicit_turn_counter = implicit_turn_counter.saturating_add(1);
    let index = turns.len();
    turns.push(MutableFallbackTurn {
        id: turn_id.clone(),
        completed: false,
        items: Vec::new(),
    });
    indices.insert(turn_id.clone(), index);
    *current_turn_id = Some(turn_id.clone());
    turn_id
}

fn append_item_if_new(items: &mut Vec<RolloutFallbackItem>, kind: &str, content: &str) {
    let content = content.trim();
    if content.is_empty() {
        return;
    }

    if items
        .last()
        .is_some_and(|item| item.kind == kind && item.content == content)
    {
        return;
    }

    items.push(RolloutFallbackItem {
        kind: kind.to_string(),
        content: content.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::find_rollout_path_for_thread;
    use super::parse_rollout_fallback_from_reader;

    #[test]
    fn parser_reconstructs_turns_and_messages_from_event_stream() {
        let input = r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1"}}
{"type":"event_msg","payload":{"type":"user_message","message":"hi"}}
{"type":"event_msg","payload":{"type":"agent_message","message":"hello"}}
{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":"hello"}}
{"type":"event_msg","payload":{"type":"task_started","turn_id":"turn-2"}}
{"type":"event_msg","payload":{"type":"user_message","message":"run tests"}}
{"type":"event_msg","payload":{"type":"agent_reasoning","text":"thinking"}}
{"type":"event_msg","payload":{"type":"agent_message","message":"done"}}
{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-2","last_agent_message":"done"}}"#;

        let turns = parse_rollout_fallback_from_reader(input.as_bytes());

        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].turn_id, "turn-1");
        assert!(turns[0].completed);
        assert_eq!(turns[0].items.len(), 2);
        assert_eq!(turns[0].items[0].kind, "userMessage");
        assert_eq!(turns[0].items[0].content, "hi");
        assert_eq!(turns[0].items[1].kind, "agentMessage");
        assert_eq!(turns[0].items[1].content, "hello");

        assert_eq!(turns[1].turn_id, "turn-2");
        assert!(turns[1].completed);
        assert_eq!(turns[1].items.len(), 3);
        assert_eq!(turns[1].items[0].kind, "userMessage");
        assert_eq!(turns[1].items[1].kind, "reasoning");
        assert_eq!(turns[1].items[2].kind, "agentMessage");
    }

    #[test]
    fn rollout_path_lookup_finds_thread_specific_jsonl() {
        let unique = format!(
            "hunk-ai-rollout-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time should be available")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let sessions = root.join("sessions").join("2026").join("03").join("04");
        std::fs::create_dir_all(&sessions).expect("sessions directories should be created");
        let target = sessions.join("rollout-2026-03-04T12-00-00-thread-abc.jsonl");
        std::fs::write(&target, b"").expect("test rollout file should be created");

        let resolved = find_rollout_path_for_thread(root.as_path(), "thread-abc")
            .expect("lookup should succeed");
        assert_eq!(resolved, Some(target.clone()));

        std::fs::remove_file(target).expect("test rollout file should be removed");
        std::fs::remove_dir_all(root).expect("test root should be removed");
    }

    #[test]
    fn rollout_path_lookup_prefers_latest_rollout_for_same_thread() {
        let unique = format!(
            "hunk-ai-rollout-test-latest-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time should be available")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let sessions = root.join("sessions").join("2026").join("03").join("04");
        std::fs::create_dir_all(&sessions).expect("sessions directories should be created");
        let older = sessions.join("rollout-2026-03-04T10-00-00-thread-xyz.jsonl");
        let newer = sessions.join("rollout-2026-03-04T12-00-00-thread-xyz.jsonl");
        std::fs::write(&older, b"").expect("older rollout file should be created");
        std::fs::write(&newer, b"").expect("newer rollout file should be created");

        let resolved = find_rollout_path_for_thread(root.as_path(), "thread-xyz")
            .expect("lookup should succeed");
        assert_eq!(resolved, Some(newer.clone()));

        std::fs::remove_file(older).expect("older rollout file should be removed");
        std::fs::remove_file(newer).expect("newer rollout file should be removed");
        std::fs::remove_dir_all(root).expect("test root should be removed");
    }
}
