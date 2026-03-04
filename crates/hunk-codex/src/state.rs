use std::collections::BTreeMap;
use std::collections::BTreeSet;

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
    pub updated_at: i64,
    pub last_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnSummary {
    pub id: String,
    pub thread_id: String,
    pub status: TurnStatus,
    pub last_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemSummary {
    pub id: String,
    pub turn_id: String,
    pub kind: String,
    pub status: ItemStatus,
    pub content: String,
    pub last_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReducerEvent {
    ThreadStarted {
        thread_id: String,
        cwd: String,
        title: Option<String>,
        updated_at: Option<i64>,
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
        turn_id: String,
    },
    ItemStarted {
        turn_id: String,
        item_id: String,
        kind: String,
    },
    ItemDelta {
        item_id: String,
        delta: String,
    },
    ItemCompleted {
        item_id: String,
    },
    TurnDiffUpdated {
        turn_id: String,
        diff: String,
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
    pub turns: BTreeMap<String, TurnSummary>,
    pub items: BTreeMap<String, ItemSummary>,
    pub turn_diffs: BTreeMap<String, String>,
    pub server_requests: BTreeMap<String, ServerRequestSummary>,
    pub active_thread_by_cwd: BTreeMap<String, String>,
    seen_dedupe_keys: BTreeSet<String>,
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
                updated_at,
            } => {
                let thread =
                    self.threads
                        .entry(thread_id.clone())
                        .or_insert_with(|| ThreadSummary {
                            id: thread_id,
                            cwd: cwd.clone(),
                            title: title.clone(),
                            status: ThreadLifecycleStatus::Idle,
                            updated_at: 0,
                            last_sequence: 0,
                        });

                if sequence < thread.last_sequence {
                    return ApplyOutcome::Stale;
                }

                thread.cwd = cwd;
                thread.title = title;
                if let Some(updated_at) = updated_at {
                    thread.updated_at = updated_at;
                }
                thread.last_sequence = sequence;
                ApplyOutcome::Applied
            }
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
                self.ensure_thread_exists(&thread_id);
                let turn = self
                    .turns
                    .entry(turn_id.clone())
                    .or_insert_with(|| TurnSummary {
                        id: turn_id,
                        thread_id,
                        status: TurnStatus::InProgress,
                        last_sequence: 0,
                    });

                if sequence < turn.last_sequence {
                    return ApplyOutcome::Stale;
                }

                turn.status = TurnStatus::InProgress;
                turn.last_sequence = sequence;
                let thread_id = turn.thread_id.clone();
                self.mark_thread_active(thread_id.as_str(), sequence);
                ApplyOutcome::Applied
            }
            ReducerEvent::TurnCompleted { turn_id } => {
                let turn = self
                    .turns
                    .entry(turn_id.clone())
                    .or_insert_with(|| TurnSummary {
                        id: turn_id,
                        thread_id: String::new(),
                        status: TurnStatus::Completed,
                        last_sequence: 0,
                    });

                if sequence < turn.last_sequence {
                    return ApplyOutcome::Stale;
                }

                turn.status = TurnStatus::Completed;
                turn.last_sequence = sequence;
                let thread_id = turn.thread_id.clone();
                self.mark_thread_idle_if_no_in_progress(thread_id.as_str(), sequence);
                ApplyOutcome::Applied
            }
            ReducerEvent::ItemStarted {
                turn_id,
                item_id,
                kind,
            } => {
                self.ensure_turn_exists(&turn_id);
                let item = self
                    .items
                    .entry(item_id.clone())
                    .or_insert_with(|| ItemSummary {
                        id: item_id,
                        turn_id: turn_id.clone(),
                        kind: kind.clone(),
                        status: ItemStatus::Started,
                        content: String::new(),
                        last_sequence: 0,
                    });

                if sequence < item.last_sequence {
                    return ApplyOutcome::Stale;
                }

                item.turn_id = turn_id;
                item.kind = kind;
                item.status = ItemStatus::Started;
                item.last_sequence = sequence;
                ApplyOutcome::Applied
            }
            ReducerEvent::ItemDelta { item_id, delta } => {
                let item = self
                    .items
                    .entry(item_id.clone())
                    .or_insert_with(|| ItemSummary {
                        id: item_id,
                        turn_id: String::new(),
                        kind: "unknown".to_string(),
                        status: ItemStatus::Streaming,
                        content: String::new(),
                        last_sequence: 0,
                    });

                if sequence < item.last_sequence {
                    return ApplyOutcome::Stale;
                }

                item.status = ItemStatus::Streaming;
                item.content.push_str(&delta);
                item.last_sequence = sequence;
                ApplyOutcome::Applied
            }
            ReducerEvent::ItemCompleted { item_id } => {
                let item = self
                    .items
                    .entry(item_id.clone())
                    .or_insert_with(|| ItemSummary {
                        id: item_id,
                        turn_id: String::new(),
                        kind: "unknown".to_string(),
                        status: ItemStatus::Completed,
                        content: String::new(),
                        last_sequence: 0,
                    });

                if sequence < item.last_sequence {
                    return ApplyOutcome::Stale;
                }

                item.status = ItemStatus::Completed;
                item.last_sequence = sequence;
                ApplyOutcome::Applied
            }
            ReducerEvent::TurnDiffUpdated { turn_id, diff } => {
                self.turn_diffs.insert(turn_id, diff);
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
        let thread = self
            .threads
            .entry(thread_id.clone())
            .or_insert_with(|| ThreadSummary {
                id: thread_id,
                cwd: String::new(),
                title: None,
                status,
                updated_at: 0,
                last_sequence: 0,
            });

        if sequence < thread.last_sequence {
            return ApplyOutcome::Stale;
        }

        thread.status = status;
        thread.last_sequence = sequence;
        ApplyOutcome::Applied
    }

    fn ensure_thread_exists(&mut self, thread_id: &str) {
        self.threads
            .entry(thread_id.to_string())
            .or_insert_with(|| ThreadSummary {
                id: thread_id.to_string(),
                cwd: String::new(),
                title: None,
                status: ThreadLifecycleStatus::Idle,
                updated_at: 0,
                last_sequence: 0,
            });
    }

    fn ensure_turn_exists(&mut self, turn_id: &str) {
        self.turns
            .entry(turn_id.to_string())
            .or_insert_with(|| TurnSummary {
                id: turn_id.to_string(),
                thread_id: String::new(),
                status: TurnStatus::InProgress,
                last_sequence: 0,
            });
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
