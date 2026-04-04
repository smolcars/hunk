impl DiffViewer {
    fn request_selected_diff_reload(&mut self, cx: &mut Context<Self>) {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.request_review_compare_refresh(cx);
        }
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotStageALoadPath {
    WithFingerprintWithoutRefresh,
    IfChangedWithoutRefresh,
    WithFingerprintRefreshWorkingCopy,
    IfChangedRefreshWorkingCopy,
}

enum SnapshotRefreshStageA {
    Unchanged(RepoSnapshotFingerprint),
    Loaded {
        fingerprint: RepoSnapshotFingerprint,
        workflow: Box<WorkflowSnapshot>,
        loaded_without_refresh: bool,
    },
}

fn snapshot_stage_a_load_path(
    behavior: SnapshotRefreshBehavior,
    prefer_stale_first: bool,
) -> SnapshotStageALoadPath {
    match (behavior, prefer_stale_first) {
        (SnapshotRefreshBehavior::ReadOnly, true) => {
            SnapshotStageALoadPath::WithFingerprintWithoutRefresh
        }
        (SnapshotRefreshBehavior::ReadOnly, false) => {
            SnapshotStageALoadPath::IfChangedWithoutRefresh
        }
        (SnapshotRefreshBehavior::RefreshWorkingCopy, true) => {
            SnapshotStageALoadPath::WithFingerprintWithoutRefresh
        }
        (SnapshotRefreshBehavior::RefreshWorkingCopy, false) => {
            SnapshotStageALoadPath::IfChangedRefreshWorkingCopy
        }
    }
}

fn snapshot_stage_a_fallback_load_path(
    prefer_stale_first: bool,
) -> SnapshotStageALoadPath {
    if prefer_stale_first {
        SnapshotStageALoadPath::WithFingerprintRefreshWorkingCopy
    } else {
        SnapshotStageALoadPath::IfChangedRefreshWorkingCopy
    }
}

fn load_snapshot_stage_a_for_path(
    load_path: SnapshotStageALoadPath,
    source_dir: &std::path::Path,
    previous_fingerprint: Option<&RepoSnapshotFingerprint>,
) -> Result<SnapshotRefreshStageA> {
    match load_path {
        SnapshotStageALoadPath::WithFingerprintWithoutRefresh => {
            let (fingerprint, workflow) =
                load_workflow_snapshot_with_fingerprint_without_refresh(source_dir)?;
            Ok(SnapshotRefreshStageA::Loaded {
                fingerprint,
                workflow: Box::new(workflow),
                loaded_without_refresh: true,
            })
        }
        SnapshotStageALoadPath::IfChangedWithoutRefresh => {
            let (fingerprint, workflow) = load_workflow_snapshot_if_changed_without_refresh(
                source_dir,
                previous_fingerprint,
            )?;
            match workflow {
                Some(workflow) => Ok(SnapshotRefreshStageA::Loaded {
                    fingerprint,
                    workflow: Box::new(workflow),
                    loaded_without_refresh: true,
                }),
                None => Ok(SnapshotRefreshStageA::Unchanged(fingerprint)),
            }
        }
        SnapshotStageALoadPath::WithFingerprintRefreshWorkingCopy => {
            let (fingerprint, workflow) = load_workflow_snapshot_with_fingerprint(source_dir)?;
            Ok(SnapshotRefreshStageA::Loaded {
                fingerprint,
                workflow: Box::new(workflow),
                loaded_without_refresh: false,
            })
        }
        SnapshotStageALoadPath::IfChangedRefreshWorkingCopy => {
            let (fingerprint, workflow) =
                load_workflow_snapshot_if_changed(source_dir, previous_fingerprint)?;
            match workflow {
                Some(workflow) => Ok(SnapshotRefreshStageA::Loaded {
                    fingerprint,
                    workflow: Box::new(workflow),
                    loaded_without_refresh: false,
                }),
                None => Ok(SnapshotRefreshStageA::Unchanged(fingerprint)),
            }
        }
    }
}

fn should_send_ai_prompt_from_input_event(event: &InputEvent) -> bool {
    matches!(event, InputEvent::PressEnter { secondary: false })
}

#[cfg(test)]
mod ai_input_tests {
    use super::{
        SnapshotStageALoadPath, SnapshotRefreshBehavior, should_send_ai_prompt_from_input_event,
        snapshot_stage_a_fallback_load_path, snapshot_stage_a_load_path,
    };
    use gpui_component::input::InputEvent;

    #[test]
    fn enter_sends_prompt() {
        assert!(should_send_ai_prompt_from_input_event(&InputEvent::PressEnter {
            secondary: false,
        }));
    }

    #[test]
    fn secondary_enter_does_not_send_prompt() {
        assert!(!should_send_ai_prompt_from_input_event(
            &InputEvent::PressEnter { secondary: true }
        ));
    }

    #[test]
    fn non_enter_events_do_not_send_prompt() {
        assert!(!should_send_ai_prompt_from_input_event(&InputEvent::Change));
        assert!(!should_send_ai_prompt_from_input_event(&InputEvent::Focus));
        assert!(!should_send_ai_prompt_from_input_event(&InputEvent::Blur));
    }

    #[test]
    fn refresh_working_copy_uses_full_if_changed_path() {
        assert_eq!(
            snapshot_stage_a_load_path(SnapshotRefreshBehavior::RefreshWorkingCopy, false),
            SnapshotStageALoadPath::IfChangedRefreshWorkingCopy
        );
    }

    #[test]
    fn refresh_working_copy_fallback_keeps_full_refresh_path() {
        assert_eq!(
            snapshot_stage_a_fallback_load_path(false),
            SnapshotStageALoadPath::IfChangedRefreshWorkingCopy
        );
    }
}
