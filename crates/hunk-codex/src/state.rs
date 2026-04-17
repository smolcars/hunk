use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::protocol::TokenUsageBreakdown;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThreadLifecycleStatus {
    Active,
    #[default]
    Idle,
    NotLoaded,
    Archived,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TurnStatus {
    #[default]
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemStatus {
    #[default]
    Started,
    Streaming,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerRequestDecision {
    Unknown,
    Accept,
    Decline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRequestSummary {
    pub request_id: String,
    pub item_id: Option<String>,
    pub decision: ServerRequestDecision,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSummary {
    pub id: String,
    pub cwd: String,
    pub title: Option<String>,
    pub status: ThreadLifecycleStatus,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TokenUsageBreakdownSummary {
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
}

impl From<TokenUsageBreakdown> for TokenUsageBreakdownSummary {
    fn from(value: TokenUsageBreakdown) -> Self {
        Self {
            total_tokens: value.total_tokens.max(0),
            input_tokens: value.input_tokens.max(0),
            cached_input_tokens: value.cached_input_tokens.max(0),
            output_tokens: value.output_tokens.max(0),
            reasoning_output_tokens: value.reasoning_output_tokens.max(0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThreadTokenUsageSummary {
    pub turn_id: String,
    pub total: TokenUsageBreakdownSummary,
    pub last: TokenUsageBreakdownSummary,
    pub model_context_window: Option<i64>,
    pub last_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnSummary {
    pub id: String,
    pub thread_id: String,
    pub collaboration_mode: Option<TurnCollaborationMode>,
    pub status: TurnStatus,
    pub last_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnCollaborationMode {
    Default,
    Plan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnPlanStepStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnPlanStepSummary {
    pub step: String,
    pub status: TurnPlanStepStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnPlanSummary {
    pub thread_id: String,
    pub turn_id: String,
    pub explanation: Option<String>,
    pub steps: Vec<TurnPlanStepSummary>,
    pub created_sequence: u64,
    pub last_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemSummary {
    pub id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub kind: String,
    pub status: ItemStatus,
    pub content: String,
    pub display_metadata: Option<ItemDisplayMetadata>,
    pub last_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDisplayMetadata {
    pub summary: Option<String>,
    pub details_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReducerEvent {
    ThreadStarted {
        thread_id: String,
        cwd: String,
        title: Option<String>,
        created_at: Option<i64>,
        updated_at: Option<i64>,
    },
    ThreadMetadataUpdated {
        thread_id: String,
        title: Option<String>,
        updated_at: Option<i64>,
    },
    ThreadTokenUsageUpdated {
        thread_id: String,
        turn_id: String,
        total: TokenUsageBreakdownSummary,
        last: TokenUsageBreakdownSummary,
        model_context_window: Option<i64>,
    },
    ThreadStatusChanged {
        thread_id: String,
        status: ThreadLifecycleStatus,
    },
    ThreadArchived {
        thread_id: String,
    },
    ThreadUnarchived {
        thread_id: String,
    },
    TurnStarted {
        thread_id: String,
        turn_id: String,
    },
    TurnCompleted {
        thread_id: String,
        turn_id: String,
    },
    TurnCollaborationModeUpdated {
        thread_id: String,
        turn_id: String,
        collaboration_mode: TurnCollaborationMode,
    },
    ItemStarted {
        thread_id: String,
        turn_id: String,
        item_id: String,
        kind: String,
    },
    ItemDelta {
        thread_id: String,
        turn_id: String,
        item_id: String,
        delta: String,
    },
    ItemCompleted {
        thread_id: String,
        turn_id: String,
        item_id: String,
    },
    ItemContentSet {
        thread_id: String,
        turn_id: String,
        item_id: String,
        content: String,
    },
    ItemDisplayMetadataUpdated {
        thread_id: String,
        turn_id: String,
        item_id: String,
        metadata: ItemDisplayMetadata,
    },
    TurnDiffUpdated {
        thread_id: String,
        turn_id: String,
        diff: String,
    },
    TurnPlanUpdated {
        thread_id: String,
        turn_id: String,
        explanation: Option<String>,
        steps: Vec<TurnPlanStepSummary>,
    },
    ServerRequestResolved {
        request_id: String,
        item_id: Option<String>,
        decision: ServerRequestDecision,
    },
    ActiveThreadSelected {
        cwd: String,
        thread_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamEvent {
    pub sequence: u64,
    pub dedupe_key: Option<String>,
    pub payload: ReducerEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied,
    Duplicate,
    Stale,
}

pub trait ActiveThreadStore {
    type Error;

    fn load_active_thread(&self, cwd: &str) -> Result<Option<String>, Self::Error>;
    fn save_active_thread(&mut self, cwd: &str, thread_id: &str) -> Result<(), Self::Error>;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AiState {
    pub threads: BTreeMap<String, ThreadSummary>,
    pub thread_token_usage: BTreeMap<String, ThreadTokenUsageSummary>,
    pub turns: BTreeMap<String, TurnSummary>,
    pub items: BTreeMap<String, ItemSummary>,
    pub turn_diffs: BTreeMap<String, String>,
    pub turn_plans: BTreeMap<String, TurnPlanSummary>,
    pub server_requests: BTreeMap<String, ServerRequestSummary>,
    pub active_thread_by_cwd: BTreeMap<String, String>,
    seen_dedupe_keys: BTreeSet<String>,
    turn_diff_sequences: BTreeMap<String, u64>,
}

const KEY_SEPARATOR: char = '\u{1f}';

pub fn turn_storage_key(thread_id: &str, turn_id: &str) -> String {
    format!("{thread_id}{KEY_SEPARATOR}{turn_id}")
}

pub fn item_storage_key(thread_id: &str, turn_id: &str, item_id: &str) -> String {
    format!("{thread_id}{KEY_SEPARATOR}{turn_id}{KEY_SEPARATOR}{item_id}")
}

impl AiState {
    pub fn apply_stream_events<I>(&mut self, events: I)
    where
        I: IntoIterator<Item = StreamEvent>,
    {
        let mut sorted: Vec<StreamEvent> = events.into_iter().collect();
        sorted.sort_by_key(|event| event.sequence);

        for event in sorted {
            let _ = self.apply_stream_event(event);
        }
    }

    pub fn apply_stream_event(&mut self, event: StreamEvent) -> ApplyOutcome {
        if let Some(dedupe_key) = event.dedupe_key {
            let is_new = self.seen_dedupe_keys.insert(dedupe_key);
            if !is_new {
                return ApplyOutcome::Duplicate;
            }
        }

        self.apply_reducer_event(event.sequence, event.payload)
    }

    pub fn set_active_thread_for_cwd(&mut self, cwd: String, thread_id: String) {
        self.active_thread_by_cwd.insert(cwd, thread_id);
    }

    pub fn active_thread_for_cwd(&self, cwd: &str) -> Option<&str> {
        self.active_thread_by_cwd.get(cwd).map(String::as_str)
    }

    pub fn turn_diff_sequence(&self, turn_key: &str) -> Option<u64> {
        self.turn_diff_sequences.get(turn_key).copied()
    }

    pub fn hydrate_active_thread_for_cwd<S>(
        &mut self,
        store: &S,
        cwd: &str,
    ) -> Result<Option<String>, S::Error>
    where
        S: ActiveThreadStore,
    {
        let loaded = store.load_active_thread(cwd)?;
        if let Some(thread_id) = loaded.as_ref() {
            self.active_thread_by_cwd
                .insert(cwd.to_string(), thread_id.clone());
        }
        Ok(loaded)
    }

    pub fn persist_active_thread_for_cwd<S>(
        &mut self,
        store: &mut S,
        cwd: String,
        thread_id: String,
    ) -> Result<(), S::Error>
    where
        S: ActiveThreadStore,
    {
        self.active_thread_by_cwd
            .insert(cwd.clone(), thread_id.clone());
        store.save_active_thread(&cwd, &thread_id)
    }

    fn apply_reducer_event(&mut self, sequence: u64, payload: ReducerEvent) -> ApplyOutcome {
        match payload {
            ReducerEvent::ThreadStarted {
                thread_id,
                cwd,
                title,
                created_at,
                updated_at,
            } => {
                let thread = self.ensure_thread_summary(thread_id.as_str());

                if sequence < thread.last_sequence {
                    return ApplyOutcome::Stale;
                }

                if thread.cwd.is_empty() {
                    thread.cwd = cwd;
                }
                thread.title = title;
                if let Some(created_at) = created_at {
                    thread.created_at = created_at;
                }
                if let Some(updated_at) = updated_at {
                    thread.updated_at = updated_at;
                }
                thread.last_sequence = sequence;
                ApplyOutcome::Applied
            }
            ReducerEvent::ThreadMetadataUpdated {
                thread_id,
                title,
                updated_at,
            } => self.apply_thread_metadata(sequence, thread_id, title, updated_at),
            ReducerEvent::ThreadTokenUsageUpdated {
                thread_id,
                turn_id,
                total,
                last,
                model_context_window,
            } => self.apply_thread_token_usage(
                sequence,
                thread_id,
                turn_id,
                total,
                last,
                model_context_window,
            ),
            ReducerEvent::ThreadStatusChanged { thread_id, status } => {
                self.apply_thread_status(sequence, thread_id, status)
            }
            ReducerEvent::ThreadArchived { thread_id } => {
                self.apply_thread_status(sequence, thread_id, ThreadLifecycleStatus::Archived)
            }
            ReducerEvent::ThreadUnarchived { thread_id } => {
                self.apply_thread_status(sequence, thread_id, ThreadLifecycleStatus::Idle)
            }
            ReducerEvent::TurnStarted { thread_id, turn_id } => {
                let turn = self.ensure_turn_summary(
                    thread_id.as_str(),
                    turn_id.as_str(),
                    TurnStatus::InProgress,
                );

                if sequence < turn.last_sequence {
                    return ApplyOutcome::Stale;
                }

                turn.status = TurnStatus::InProgress;
                turn.last_sequence = sequence;
                let thread_id = turn.thread_id.clone();
                self.mark_thread_active(thread_id.as_str(), sequence);
                ApplyOutcome::Applied
            }
            ReducerEvent::TurnCompleted { thread_id, turn_id } => {
                let turn = self.ensure_turn_summary(
                    thread_id.as_str(),
                    turn_id.as_str(),
                    TurnStatus::Completed,
                );

                if sequence < turn.last_sequence {
                    return ApplyOutcome::Stale;
                }

                turn.status = TurnStatus::Completed;
                turn.last_sequence = sequence;
                let thread_id = turn.thread_id.clone();
                self.mark_thread_idle_if_no_in_progress(thread_id.as_str(), sequence);
                ApplyOutcome::Applied
            }
            ReducerEvent::TurnCollaborationModeUpdated {
                thread_id,
                turn_id,
                collaboration_mode,
            } => {
                let turn = self.ensure_turn_summary(
                    thread_id.as_str(),
                    turn_id.as_str(),
                    TurnStatus::InProgress,
                );

                if sequence < turn.last_sequence {
                    return ApplyOutcome::Stale;
                }

                turn.collaboration_mode = Some(collaboration_mode);
                turn.last_sequence = sequence;
                ApplyOutcome::Applied
            }
            ReducerEvent::ItemStarted {
                thread_id,
                turn_id,
                item_id,
                kind,
            } => {
                let item = self.ensure_item_summary(
                    thread_id.as_str(),
                    turn_id.as_str(),
                    item_id.as_str(),
                    kind.as_str(),
                    ItemStatus::Started,
                );

                if sequence < item.last_sequence {
                    return ApplyOutcome::Stale;
                }

                item.thread_id = thread_id;
                item.turn_id = turn_id;
                item.kind = kind;
                item.status = ItemStatus::Started;
                item.last_sequence = sequence;
                ApplyOutcome::Applied
            }
            ReducerEvent::ItemDelta {
                thread_id,
                turn_id,
                item_id,
                delta,
            } => {
                let item = self.ensure_item_summary(
                    thread_id.as_str(),
                    turn_id.as_str(),
                    item_id.as_str(),
                    "unknown",
                    ItemStatus::Streaming,
                );

                if sequence < item.last_sequence {
                    return ApplyOutcome::Stale;
                }

                item.thread_id = thread_id;
                item.turn_id = turn_id;
                item.status = ItemStatus::Streaming;
                item.content.push_str(&delta);
                item.last_sequence = sequence;
                ApplyOutcome::Applied
            }
            ReducerEvent::ItemCompleted {
                thread_id,
                turn_id,
                item_id,
            } => {
                let item = self.ensure_item_summary(
                    thread_id.as_str(),
                    turn_id.as_str(),
                    item_id.as_str(),
                    "unknown",
                    ItemStatus::Completed,
                );

                if sequence < item.last_sequence {
                    return ApplyOutcome::Stale;
                }

                item.thread_id = thread_id;
                item.turn_id = turn_id;
                item.status = ItemStatus::Completed;
                item.last_sequence = sequence;
                ApplyOutcome::Applied
            }
            ReducerEvent::ItemContentSet {
                thread_id,
                turn_id,
                item_id,
                content,
            } => {
                let item = self.ensure_item_summary(
                    thread_id.as_str(),
                    turn_id.as_str(),
                    item_id.as_str(),
                    "unknown",
                    ItemStatus::Started,
                );

                if sequence < item.last_sequence {
                    return ApplyOutcome::Stale;
                }

                item.thread_id = thread_id;
                item.turn_id = turn_id;
                item.content = content;
                item.last_sequence = sequence;
                ApplyOutcome::Applied
            }
            ReducerEvent::ItemDisplayMetadataUpdated {
                thread_id,
                turn_id,
                item_id,
                metadata,
            } => {
                let item = self.ensure_item_summary(
                    thread_id.as_str(),
                    turn_id.as_str(),
                    item_id.as_str(),
                    "unknown",
                    ItemStatus::Started,
                );

                if sequence < item.last_sequence {
                    return ApplyOutcome::Stale;
                }

                item.thread_id = thread_id;
                item.turn_id = turn_id;
                item.display_metadata = normalize_item_display_metadata(metadata);
                item.last_sequence = sequence;
                ApplyOutcome::Applied
            }
            ReducerEvent::TurnDiffUpdated {
                thread_id,
                turn_id,
                diff,
            } => {
                let turn_key = turn_storage_key(thread_id.as_str(), turn_id.as_str());
                if self
                    .turn_diff_sequences
                    .get(turn_key.as_str())
                    .is_some_and(|last_sequence| sequence < *last_sequence)
                {
                    return ApplyOutcome::Stale;
                }

                self.turn_diffs.insert(turn_key, diff);
                self.turn_diff_sequences.insert(
                    turn_storage_key(thread_id.as_str(), turn_id.as_str()),
                    sequence,
                );
                ApplyOutcome::Applied
            }
            ReducerEvent::TurnPlanUpdated {
                thread_id,
                turn_id,
                explanation,
                steps,
            } => {
                self.ensure_turn_summary(
                    thread_id.as_str(),
                    turn_id.as_str(),
                    TurnStatus::InProgress,
                );
                let turn_key = turn_storage_key(thread_id.as_str(), turn_id.as_str());
                let explanation = explanation
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                let steps = steps
                    .into_iter()
                    .filter_map(|step| {
                        let text = step.step.trim().to_string();
                        (!text.is_empty()).then_some(TurnPlanStepSummary {
                            step: text,
                            status: step.status,
                        })
                    })
                    .collect::<Vec<_>>();
                match self.turn_plans.entry(turn_key) {
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        if sequence < entry.get().last_sequence {
                            return ApplyOutcome::Stale;
                        }

                        let plan = entry.get_mut();
                        plan.thread_id = thread_id;
                        plan.turn_id = turn_id;
                        plan.explanation = explanation;
                        plan.steps = steps;
                        plan.last_sequence = sequence;
                    }
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(TurnPlanSummary {
                            thread_id,
                            turn_id,
                            explanation,
                            steps,
                            created_sequence: sequence,
                            last_sequence: sequence,
                        });
                    }
                }
                ApplyOutcome::Applied
            }
            ReducerEvent::ServerRequestResolved {
                request_id,
                item_id,
                decision,
            } => {
                let request = self
                    .server_requests
                    .entry(request_id.clone())
                    .or_insert_with(|| ServerRequestSummary {
                        request_id,
                        item_id: item_id.clone(),
                        decision,
                        sequence,
                    });

                if sequence < request.sequence {
                    return ApplyOutcome::Stale;
                }

                request.item_id = item_id;
                if matches!(decision, ServerRequestDecision::Unknown)
                    && !matches!(request.decision, ServerRequestDecision::Unknown)
                {
                    request.sequence = sequence;
                    return ApplyOutcome::Applied;
                }
                request.decision = decision;
                request.sequence = sequence;
                ApplyOutcome::Applied
            }
            ReducerEvent::ActiveThreadSelected { cwd, thread_id } => {
                self.active_thread_by_cwd.insert(cwd, thread_id);
                ApplyOutcome::Applied
            }
        }
    }

    fn apply_thread_status(
        &mut self,
        sequence: u64,
        thread_id: String,
        status: ThreadLifecycleStatus,
    ) -> ApplyOutcome {
        let thread = self.ensure_thread_summary(thread_id.as_str());

        if sequence < thread.last_sequence {
            return ApplyOutcome::Stale;
        }

        thread.status = status;
        thread.last_sequence = sequence;
        if matches!(
            thread.status,
            ThreadLifecycleStatus::Archived | ThreadLifecycleStatus::Closed
        ) {
            self.active_thread_by_cwd
                .retain(|_, active_thread_id| active_thread_id != &thread_id);
        }
        ApplyOutcome::Applied
    }

    fn apply_thread_metadata(
        &mut self,
        sequence: u64,
        thread_id: String,
        title: Option<String>,
        updated_at: Option<i64>,
    ) -> ApplyOutcome {
        let thread = self.ensure_thread_summary(thread_id.as_str());

        if sequence < thread.last_sequence {
            return ApplyOutcome::Stale;
        }

        thread.title = title;
        if let Some(updated_at) = updated_at {
            thread.updated_at = updated_at;
        }
        thread.last_sequence = sequence;
        ApplyOutcome::Applied
    }

    fn apply_thread_token_usage(
        &mut self,
        sequence: u64,
        thread_id: String,
        turn_id: String,
        total: TokenUsageBreakdownSummary,
        last: TokenUsageBreakdownSummary,
        model_context_window: Option<i64>,
    ) -> ApplyOutcome {
        self.ensure_thread_summary(thread_id.as_str());
        let model_context_window = model_context_window.filter(|value| *value > 0);
        match self.thread_token_usage.entry(thread_id) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                if sequence < entry.get().last_sequence {
                    return ApplyOutcome::Stale;
                }

                let summary = entry.get_mut();
                summary.turn_id = turn_id;
                summary.total = total;
                summary.last = last;
                summary.model_context_window = model_context_window;
                summary.last_sequence = sequence;
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(ThreadTokenUsageSummary {
                    turn_id,
                    total,
                    last,
                    model_context_window,
                    last_sequence: sequence,
                });
            }
        }
        ApplyOutcome::Applied
    }

    fn ensure_thread_summary(&mut self, thread_id: &str) -> &mut ThreadSummary {
        self.threads
            .entry(thread_id.to_string())
            .or_insert_with(|| Self::new_thread_summary(thread_id))
    }

    fn ensure_turn_summary(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        status: TurnStatus,
    ) -> &mut TurnSummary {
        self.ensure_thread_summary(thread_id);
        let turn_key = turn_storage_key(thread_id, turn_id);
        self.turns
            .entry(turn_key)
            .or_insert_with(|| Self::new_turn_summary(thread_id, turn_id, status))
    }

    fn ensure_item_summary(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        item_id: &str,
        kind: &str,
        status: ItemStatus,
    ) -> &mut ItemSummary {
        self.ensure_turn_summary(thread_id, turn_id, TurnStatus::InProgress);
        let item_key = item_storage_key(thread_id, turn_id, item_id);
        self.items
            .entry(item_key)
            .or_insert_with(|| Self::new_item_summary(thread_id, turn_id, item_id, kind, status))
    }

    fn new_thread_summary(thread_id: &str) -> ThreadSummary {
        ThreadSummary {
            id: thread_id.to_string(),
            cwd: String::new(),
            title: None,
            status: ThreadLifecycleStatus::Idle,
            created_at: 0,
            updated_at: 0,
            last_sequence: 0,
        }
    }

    fn new_turn_summary(thread_id: &str, turn_id: &str, status: TurnStatus) -> TurnSummary {
        TurnSummary {
            id: turn_id.to_string(),
            thread_id: thread_id.to_string(),
            collaboration_mode: None,
            status,
            last_sequence: 0,
        }
    }

    fn new_item_summary(
        thread_id: &str,
        turn_id: &str,
        item_id: &str,
        kind: &str,
        status: ItemStatus,
    ) -> ItemSummary {
        ItemSummary {
            id: item_id.to_string(),
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            kind: kind.to_string(),
            status,
            content: String::new(),
            display_metadata: None,
            last_sequence: 0,
        }
    }

    fn mark_thread_active(&mut self, thread_id: &str, sequence: u64) {
        let Some(thread) = self.threads.get_mut(thread_id) else {
            return;
        };
        if matches!(
            thread.status,
            ThreadLifecycleStatus::Archived
                | ThreadLifecycleStatus::NotLoaded
                | ThreadLifecycleStatus::Closed
        ) || sequence < thread.last_sequence
        {
            return;
        }
        thread.status = ThreadLifecycleStatus::Active;
        thread.last_sequence = sequence;
    }

    fn mark_thread_idle_if_no_in_progress(&mut self, thread_id: &str, sequence: u64) {
        if thread_id.is_empty() {
            return;
        }
        let has_in_progress_turn = self
            .turns
            .values()
            .any(|turn| turn.thread_id == thread_id && turn.status == TurnStatus::InProgress);
        if has_in_progress_turn {
            return;
        }

        let Some(thread) = self.threads.get_mut(thread_id) else {
            return;
        };
        if !matches!(
            thread.status,
            ThreadLifecycleStatus::Active | ThreadLifecycleStatus::Idle
        ) || sequence < thread.last_sequence
        {
            return;
        }
        thread.status = ThreadLifecycleStatus::Idle;
        thread.last_sequence = sequence;
    }
}

fn normalize_item_display_metadata(metadata: ItemDisplayMetadata) -> Option<ItemDisplayMetadata> {
    let summary = metadata.summary.filter(|value| !value.trim().is_empty());
    let details_json = metadata
        .details_json
        .filter(|value| !value.trim().is_empty());
    if summary.is_none() && details_json.is_none() {
        return None;
    }
    Some(ItemDisplayMetadata {
        summary,
        details_json,
    })
}
