fn store_visible_terminal_state<S: Clone>(
    states: &mut BTreeMap<String, S>,
    owner_key: Option<&str>,
    state: S,
) {
    let Some(owner_key) = owner_key else {
        return;
    };

    states.insert(owner_key.to_string(), state);
}

fn restore_visible_terminal_state<S: Clone + Default>(
    states: &BTreeMap<String, S>,
    owner_key: Option<&str>,
) -> S {
    owner_key
        .and_then(|owner_key| states.get(owner_key).cloned())
        .unwrap_or_default()
}

fn terminal_runtime_tab_key(owner_key: &str, tab_id: TerminalTabId) -> String {
    format!("{owner_key}\u{1f}{tab_id}")
}

fn park_visible_terminal_runtime<R>(
    owner_key: Option<&str>,
    visible_runtime: &mut Option<R>,
    event_task: &mut Task<()>,
    hidden_runtimes: &mut BTreeMap<String, ParkedTerminalRuntimeHandle<R>>,
) {
    let Some(owner_key) = owner_key else {
        return;
    };
    let Some(runtime) = visible_runtime.take() else {
        return;
    };

    let parked_event_task = std::mem::replace(event_task, Task::ready(()));
    hidden_runtimes.insert(
        owner_key.to_string(),
        ParkedTerminalRuntimeHandle {
            runtime,
            event_task: parked_event_task,
        },
    );
}

fn promote_hidden_terminal_runtime<R>(
    owner_key: &str,
    visible_runtime: &mut Option<R>,
    event_task: &mut Task<()>,
    hidden_runtimes: &mut BTreeMap<String, ParkedTerminalRuntimeHandle<R>>,
) -> bool {
    let Some(hidden) = hidden_runtimes.remove(owner_key) else {
        return false;
    };

    *visible_runtime = Some(hidden.runtime);
    *event_task = hidden.event_task;
    true
}

#[cfg(test)]
mod terminal_runtime_store_tests {
    use super::{
        ParkedTerminalRuntimeHandle, Task, park_visible_terminal_runtime,
        promote_hidden_terminal_runtime, restore_visible_terminal_state,
        store_visible_terminal_state,
    };
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    struct TestState {
        label: &'static str,
    }

    #[test]
    fn visible_terminal_state_round_trips_by_owner_key() {
        let mut states = BTreeMap::new();
        store_visible_terminal_state(
            &mut states,
            Some("thread-1"),
            TestState { label: "saved" },
        );

        assert_eq!(
            restore_visible_terminal_state::<TestState>(&states, Some("thread-1")),
            TestState { label: "saved" }
        );
        assert_eq!(
            restore_visible_terminal_state::<TestState>(&states, Some("thread-2")),
            TestState::default()
        );
    }

    #[test]
    fn park_visible_terminal_runtime_moves_runtime_into_hidden_map() {
        let mut visible_runtime = Some(7_u8);
        let mut event_task = Task::ready(());
        let mut hidden_runtimes = BTreeMap::new();

        park_visible_terminal_runtime(
            Some("thread-1"),
            &mut visible_runtime,
            &mut event_task,
            &mut hidden_runtimes,
        );

        assert_eq!(visible_runtime, None);
        assert_eq!(hidden_runtimes.len(), 1);
        assert_eq!(hidden_runtimes.remove("thread-1").map(|hidden| hidden.runtime), Some(7));
    }

    #[test]
    fn promote_hidden_terminal_runtime_restores_visible_runtime() {
        let mut visible_runtime = None;
        let mut event_task = Task::ready(());
        let mut hidden_runtimes = BTreeMap::from([(
            "thread-1".to_string(),
            ParkedTerminalRuntimeHandle {
                runtime: 11_u8,
                event_task: Task::ready(()),
            },
        )]);

        assert!(promote_hidden_terminal_runtime(
            "thread-1",
            &mut visible_runtime,
            &mut event_task,
            &mut hidden_runtimes,
        ));

        assert_eq!(visible_runtime, Some(11));
        assert!(hidden_runtimes.is_empty());
    }

    #[test]
    fn promote_hidden_terminal_runtime_returns_false_when_missing() {
        let mut visible_runtime = None::<u8>;
        let mut event_task = Task::ready(());
        let mut hidden_runtimes = BTreeMap::new();

        assert!(!promote_hidden_terminal_runtime(
            "missing",
            &mut visible_runtime,
            &mut event_task,
            &mut hidden_runtimes,
        ));
        assert_eq!(visible_runtime, None);
    }

}
