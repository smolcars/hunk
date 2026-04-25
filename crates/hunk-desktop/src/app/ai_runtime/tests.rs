#[cfg(test)]
mod ai_tests {
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Duration;

    use hunk_codex::protocol::AccountLoginCompletedNotification;
    use hunk_codex::protocol::AskForApproval;
    use hunk_codex::protocol::ModeKind;
    use hunk_codex::protocol::RateLimitSnapshot;
    use hunk_codex::protocol::RateLimitWindow;
    use hunk_codex::protocol::RequestId;
    use hunk_codex::protocol::SandboxMode;
    use hunk_codex::protocol::SandboxPolicy;
    use hunk_codex::protocol::ServerNotification;
    use hunk_codex::protocol::SkillsChangedNotification;
    use hunk_codex::protocol::ThreadStartParams;
    use hunk_codex::protocol::TurnStartParams;
    use hunk_codex::protocol::UserInput;
    use hunk_codex::protocol::ServiceTier;
    use hunk_codex::errors::CodexIntegrationError;
    use hunk_codex::state::AiState;
    use hunk_codex::state::ReducerEvent;
    use hunk_codex::state::StreamEvent;
    use hunk_domain::state::AiServiceTierSelection;

    use crate::app::AiPromptSkillReference;

    use super::AiApprovalDecision;
    use super::AiWorkerCommand;
    use super::AiWorkerEventPayload;
    use super::AiWorkerStartConfig;
    use super::AiTurnSessionOverrides;
    use super::apply_login_completed_state;
    use super::apply_thread_start_policy;
    use super::apply_browser_thread_start_context;
    use super::apply_thread_start_session_overrides;
    use super::apply_turn_start_policy;
    use super::collaboration_mode_for_turn;
    use super::command_can_retry_after_reconnect;
    use super::dispatch_ai_worker_result;
    use super::reconnect_backoff;
    use super::is_transient_rollout_load_error;
    use super::is_missing_thread_rollout_error;
    use super::map_command_approval_decision;
    use super::map_file_change_approval_decision;
    use super::notification_refresh_flags;
    use super::panic_payload_message;
    use super::pending_steer_with_state_baseline;
    use super::prompt_user_input_items;
    use super::preferred_rate_limit_snapshot;
    use super::request_id_key;
    use super::retry_transient_rollout_load;
    use super::selected_ai_service_tier;
    use super::should_attempt_runtime_reconnect;
    use super::should_retry_stale_turn_after_steer_error;
    use super::thread_missing_item_turn_ids;

    #[test]
    fn thread_policy_defaults_to_on_request_when_not_mad_max() {
        let mut params = ThreadStartParams::default();
        apply_thread_start_policy(false, &mut params);
        assert_eq!(params.approval_policy, Some(AskForApproval::OnRequest));
        assert_eq!(params.sandbox, Some(SandboxMode::WorkspaceWrite));
    }

    #[test]
    fn worker_start_config_uses_cwd_as_workspace_key() {
        let config = AiWorkerStartConfig::new(
            std::path::PathBuf::from("/repo/worktrees/task-a"),
            std::path::PathBuf::from("/bin/codex"),
            std::path::PathBuf::from("/tmp/codex-home"),
        );
        assert_eq!(config.workspace_key, "/repo/worktrees/task-a");
    }

    #[test]
    fn thread_policy_resets_danger_settings_when_not_mad_max() {
        let mut params = ThreadStartParams {
            approval_policy: Some(AskForApproval::Never),
            sandbox: Some(SandboxMode::DangerFullAccess),
            ..ThreadStartParams::default()
        };
        apply_thread_start_policy(false, &mut params);
        assert_eq!(params.approval_policy, Some(AskForApproval::OnRequest));
        assert_eq!(params.sandbox, Some(SandboxMode::WorkspaceWrite));
    }

    #[test]
    fn thread_policy_switches_to_never_and_danger_in_mad_max() {
        let mut params = ThreadStartParams::default();
        apply_thread_start_policy(true, &mut params);
        assert_eq!(params.approval_policy, Some(AskForApproval::Never));
        assert_eq!(params.sandbox, Some(SandboxMode::DangerFullAccess));
    }

    #[test]
    fn browser_thread_start_context_adds_browser_tools_when_enabled() {
        let mut params = ThreadStartParams::default();
        apply_browser_thread_start_context(&mut params);

        let tools = params
            .dynamic_tools
            .as_ref()
            .expect("browser tools should be present");
        assert!(
            tools
                .iter()
                .any(|tool| {
                    tool.namespace.as_deref()
                        == Some(hunk_codex::browser_tools::BROWSER_TOOL_NAMESPACE)
                        && tool.name == hunk_codex::browser_tools::BROWSER_SNAPSHOT_TOOL
                })
        );
        assert!(
            params
                .developer_instructions
                .as_deref()
                .is_some_and(|instructions| instructions.contains("hunk_browser.snapshot"))
        );
    }

    #[test]
    fn pending_steer_with_state_baseline_uses_the_latest_refreshed_turn_sequence() {
        let mut state = AiState::default();
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 8,
            dedupe_key: None,
            payload: ReducerEvent::ItemStarted {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                item_id: "item-older".to_string(),
                kind: "userMessage".to_string(),
            },
        });
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 9,
            dedupe_key: None,
            payload: ReducerEvent::ItemDelta {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                item_id: "item-older".to_string(),
                delta: "same follow-up".to_string(),
            },
        });
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 10,
            dedupe_key: None,
            payload: ReducerEvent::ItemCompleted {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                item_id: "item-older".to_string(),
            },
        });
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 12,
            dedupe_key: None,
            payload: ReducerEvent::TurnStarted {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
            },
        });

        let pending = pending_steer_with_state_baseline(
            &state,
            "thread-1".to_string(),
            "turn-1".to_string(),
            Some("same follow-up"),
            &[],
            &[],
            &[],
        );

        assert_eq!(pending.accepted_after_sequence, 12);
    }

    #[test]
    fn service_tier_selection_maps_to_protocol_override() {
        assert_eq!(
            selected_ai_service_tier(AiServiceTierSelection::Standard),
            Some(None)
        );
        assert_eq!(
            selected_ai_service_tier(AiServiceTierSelection::Fast),
            Some(Some(ServiceTier::Fast))
        );
        assert_eq!(
            selected_ai_service_tier(AiServiceTierSelection::Flex),
            Some(Some(ServiceTier::Flex))
        );
    }

    #[test]
    fn thread_start_session_overrides_apply_explicit_standard_service_tier() {
        let mut params = ThreadStartParams::default();
        apply_thread_start_session_overrides(
            &AiTurnSessionOverrides {
                service_tier: AiServiceTierSelection::Standard,
                ..AiTurnSessionOverrides::default()
            },
            &mut params,
        );
        assert_eq!(params.service_tier, Some(None));
    }

    #[test]
    fn turn_policy_switches_to_never_and_danger_in_mad_max() {
        let mut params = TurnStartParams::default();
        apply_turn_start_policy(true, &mut params);
        assert_eq!(params.approval_policy, Some(AskForApproval::Never));
        assert_eq!(params.sandbox_policy, Some(SandboxPolicy::DangerFullAccess));
    }

    #[test]
    fn turn_policy_defaults_to_on_request_and_workspace_write_when_not_mad_max() {
        let mut params = TurnStartParams::default();
        apply_turn_start_policy(false, &mut params);
        assert_eq!(params.approval_policy, Some(AskForApproval::OnRequest));
        assert_eq!(
            params.sandbox_policy,
            Some(super::non_mad_max_turn_sandbox_policy())
        );
    }

    #[test]
    fn turn_policy_resets_danger_settings_when_not_mad_max() {
        let mut params = TurnStartParams {
            thread_id: "thread-1".to_string(),
            approval_policy: Some(AskForApproval::Never),
            sandbox_policy: Some(SandboxPolicy::DangerFullAccess),
            ..TurnStartParams::default()
        };
        apply_turn_start_policy(false, &mut params);
        assert_eq!(params.approval_policy, Some(AskForApproval::OnRequest));
        assert_eq!(
            params.sandbox_policy,
            Some(super::non_mad_max_turn_sandbox_policy())
        );
    }

    #[test]
    fn approval_decision_mapping_is_stable() {
        assert_eq!(
            map_command_approval_decision(AiApprovalDecision::Accept),
            hunk_codex::protocol::CommandExecutionApprovalDecision::Accept
        );
        assert_eq!(
            map_file_change_approval_decision(AiApprovalDecision::Decline),
            hunk_codex::protocol::FileChangeApprovalDecision::Decline
        );
    }

    #[test]
    fn notification_refresh_flags_reload_skills_on_skills_changed() {
        let flags = notification_refresh_flags(&[ServerNotification::SkillsChanged(
            SkillsChangedNotification {},
        )]);

        assert!(flags.refresh_skills);
        assert!(!flags.refresh_account);
        assert!(!flags.refresh_rate_limits);
    }

    #[test]
    fn prompt_user_input_items_appends_structured_skills_after_text() {
        let inputs = prompt_user_input_items(
            Some("Use $gpui and $gpui-component"),
            &[PathBuf::from("/tmp/diagram.png")],
            &[selected_skill("gpui"), selected_skill("gpui-component")],
        );

        assert_eq!(
            inputs,
            vec![
                UserInput::LocalImage {
                    path: PathBuf::from("/tmp/diagram.png"),
                },
                UserInput::Text {
                    text: "Use $gpui and $gpui-component".to_string(),
                    text_elements: Vec::new(),
                },
                UserInput::Skill {
                    name: "gpui".to_string(),
                    path: PathBuf::from("/skills/gpui/SKILL.md"),
                },
                UserInput::Skill {
                    name: "gpui-component".to_string(),
                    path: PathBuf::from("/skills/gpui-component/SKILL.md"),
                },
            ]
        );
    }

    #[test]
    fn prompt_user_input_items_leaves_unresolved_skill_text_alone() {
        let inputs = prompt_user_input_items(Some("Use $missing"), &[], &[]);

        assert_eq!(
            inputs,
            vec![UserInput::Text {
                text: "Use $missing".to_string(),
                text_elements: Vec::new(),
            }]
        );
    }

    #[test]
    fn collaboration_mode_for_turn_preserves_explicit_reasoning_effort() {
        let mode = collaboration_mode_for_turn(
            ModeKind::Plan,
            Some("gpt-5.4".to_string()),
            Some(hunk_codex::protocol::ReasoningEffort::High),
            None,
        )
        .expect("collaboration mode should resolve");

        assert_eq!(mode.mode, ModeKind::Plan);
        assert_eq!(mode.settings.model, "gpt-5.4");
        assert_eq!(
            mode.settings.reasoning_effort,
            Some(hunk_codex::protocol::ReasoningEffort::High)
        );
    }

    fn rate_limit_snapshot(limit_id: Option<&str>, used_percent: i32) -> RateLimitSnapshot {
        RateLimitSnapshot {
            limit_id: limit_id.map(ToOwned::to_owned),
            limit_name: limit_id.map(ToOwned::to_owned),
            primary: Some(RateLimitWindow {
                used_percent,
                window_duration_mins: Some(300),
                resets_at: Some(1_700_000_000),
            }),
            secondary: Some(RateLimitWindow {
                used_percent: used_percent.saturating_add(10),
                window_duration_mins: Some(10_080),
                resets_at: Some(1_700_100_000),
            }),
            credits: None,
            plan_type: None,
            rate_limit_reached_type: None,
        }
    }

    fn selected_skill(name: &str) -> AiPromptSkillReference {
        AiPromptSkillReference {
            name: name.to_string(),
            path: PathBuf::from(format!("/skills/{name}/SKILL.md")),
        }
    }

    #[test]
    fn preferred_rate_limit_snapshot_prefers_codex_bucket() {
        let mut snapshots = HashMap::new();
        snapshots.insert(
            "codex_other".to_string(),
            rate_limit_snapshot(Some("codex_other"), 4),
        );
        snapshots.insert("codex".to_string(), rate_limit_snapshot(Some("codex"), 35));

        let selected =
            preferred_rate_limit_snapshot(&snapshots, None).expect("a snapshot should be selected");
        assert_eq!(selected.limit_id.as_deref(), Some("codex"));
        assert_eq!(
            selected.primary.as_ref().map(|window| window.used_percent),
            Some(35)
        );
    }

    #[test]
    fn preferred_rate_limit_snapshot_falls_back_when_codex_missing() {
        let mut snapshots = HashMap::new();
        snapshots.insert(
            "codex_other".to_string(),
            rate_limit_snapshot(Some("codex_other"), 7),
        );

        let fallback = rate_limit_snapshot(Some("fallback"), 22);
        let selected = preferred_rate_limit_snapshot(&snapshots, Some(&fallback))
            .expect("fallback snapshot should be selected");
        assert_eq!(selected.limit_id.as_deref(), Some("fallback"));
        assert_eq!(
            selected.primary.as_ref().map(|window| window.used_percent),
            Some(22)
        );
    }

    #[test]
    fn login_completion_clears_pending_state_on_success() {
        let mut pending_login_id = Some("login-1".to_string());
        let mut pending_auth_url = Some("https://auth.example/login".to_string());
        let message = apply_login_completed_state(
            &mut pending_login_id,
            &mut pending_auth_url,
            &AccountLoginCompletedNotification {
                login_id: Some("login-1".to_string()),
                success: true,
                error: None,
            },
        );

        assert_eq!(message, "ChatGPT login completed.");
        assert_eq!(pending_login_id, None);
        assert_eq!(pending_auth_url, None);
    }

    #[test]
    fn login_completion_failure_prefers_server_error_message() {
        let mut pending_login_id = Some("login-2".to_string());
        let mut pending_auth_url = Some("https://auth.example/login".to_string());
        let message = apply_login_completed_state(
            &mut pending_login_id,
            &mut pending_auth_url,
            &AccountLoginCompletedNotification {
                login_id: Some("login-2".to_string()),
                success: false,
                error: Some("token expired".to_string()),
            },
        );

        assert_eq!(message, "ChatGPT login failed: token expired");
        assert_eq!(pending_login_id, None);
        assert_eq!(pending_auth_url, None);
    }

    #[test]
    fn stale_turn_retry_helper_matches_retryable_turn_steer_server_errors() {
        assert!(should_retry_stale_turn_after_steer_error(
            &CodexIntegrationError::JsonRpcServerError {
                code: -32602,
                message: "expected_turn_id is stale".to_string(),
            }
        ));
        assert!(should_retry_stale_turn_after_steer_error(
            &CodexIntegrationError::JsonRpcServerError {
                code: -32600,
                message: "expected active turn id `turn-1` but found `turn-2`".to_string(),
            }
        ));
        assert!(should_retry_stale_turn_after_steer_error(
            &CodexIntegrationError::JsonRpcServerError {
                code: -32600,
                message: "no active turn to steer".to_string(),
            }
        ));
        assert!(!should_retry_stale_turn_after_steer_error(
            &CodexIntegrationError::JsonRpcServerError {
                code: -32602,
                message: "invalid model".to_string(),
            }
        ));
        assert!(!should_retry_stale_turn_after_steer_error(
            &CodexIntegrationError::WebSocketTransport("closed".to_string())
        ));
    }

    #[test]
    fn transient_rollout_error_helper_matches_empty_rollout_server_errors_only() {
        assert!(is_transient_rollout_load_error(
            &CodexIntegrationError::JsonRpcServerError {
                code: -32603,
                message: "failed to load rollout '/tmp/rollout.jsonl' for thread thread-1: rollout at /tmp/rollout.jsonl is empty".to_string(),
            }
        ));
        assert!(!is_transient_rollout_load_error(
            &CodexIntegrationError::JsonRpcServerError {
                code: -32603,
                message: "failed to load rollout '/tmp/rollout.jsonl': permission denied"
                    .to_string(),
            }
        ));
        assert!(!is_transient_rollout_load_error(
            &CodexIntegrationError::JsonRpcServerError {
                code: -32602,
                message: "failed to load rollout '/tmp/rollout.jsonl' for thread thread-1: rollout at /tmp/rollout.jsonl is empty".to_string(),
            }
        ));
    }

    #[test]
    fn missing_thread_rollout_error_helper_matches_missing_rollout_server_errors_only() {
        assert!(is_missing_thread_rollout_error(
            &CodexIntegrationError::JsonRpcServerError {
                code: -32600,
                message: "no rollout found for thread id thread-1".to_string(),
            }
        ));
        assert!(!is_missing_thread_rollout_error(
            &CodexIntegrationError::JsonRpcServerError {
                code: -32603,
                message: "no rollout found for thread id thread-1".to_string(),
            }
        ));
        assert!(!is_missing_thread_rollout_error(
            &CodexIntegrationError::JsonRpcServerError {
                code: -32600,
                message: "failed to load rollout '/tmp/rollout.jsonl'".to_string(),
            }
        ));
    }

    #[test]
    fn transient_rollout_retry_retries_until_success() {
        let attempts = Cell::new(0usize);
        let result = retry_transient_rollout_load(2, Duration::ZERO, || {
            attempts.set(attempts.get().saturating_add(1));
            if attempts.get() < 3 {
                return Err(CodexIntegrationError::JsonRpcServerError {
                    code: -32603,
                    message:
                        "failed to load rollout '/tmp/rollout.jsonl' for thread thread-1: rollout at /tmp/rollout.jsonl is empty"
                            .to_string(),
                });
            }
            Ok("loaded")
        })
        .expect("retry helper should succeed after transient rollout load errors");

        assert_eq!(result, "loaded");
        assert_eq!(attempts.get(), 3);
    }

    #[test]
    fn transient_rollout_retry_does_not_retry_non_rollout_errors() {
        let attempts = Cell::new(0usize);
        let error = retry_transient_rollout_load(3, Duration::ZERO, || {
            attempts.set(attempts.get().saturating_add(1));
            Result::<(), CodexIntegrationError>::Err(CodexIntegrationError::JsonRpcServerError {
                code: -32602,
                message: "invalid params".to_string(),
            })
        })
        .expect_err("non-rollout errors should not be retried");

        assert_eq!(attempts.get(), 1);
        match error {
            CodexIntegrationError::JsonRpcServerError { code, message } => {
                assert_eq!(code, -32602);
                assert_eq!(message, "invalid params");
            }
            other => panic!("expected json-rpc error, got {other:?}"),
        }
    }

    #[test]
    fn request_id_keys_are_type_stable() {
        assert_eq!(
            request_id_key(&RequestId::Integer(7)),
            "int:7".to_string()
        );
        assert_eq!(
            request_id_key(&RequestId::String("7".to_string())),
            "str:7".to_string()
        );
    }

    #[test]
    fn thread_rollout_fallback_is_needed_when_turns_exist_without_items() {
        let mut state = AiState::default();
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 1,
            dedupe_key: None,
            payload: ReducerEvent::TurnStarted {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
            },
        });
        let missing_turns = thread_missing_item_turn_ids(&state, "thread-1");
        assert_eq!(
            missing_turns.into_iter().collect::<Vec<_>>(),
            vec!["turn-1".to_string()]
        );
    }

    #[test]
    fn thread_rollout_fallback_is_not_needed_when_items_exist() {
        let mut state = AiState::default();
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 1,
            dedupe_key: None,
            payload: ReducerEvent::TurnStarted {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
            },
        });
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 2,
            dedupe_key: None,
            payload: ReducerEvent::ItemStarted {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                item_id: "item-1".to_string(),
                kind: "agentMessage".to_string(),
            },
        });
        assert!(thread_missing_item_turn_ids(&state, "thread-1").is_empty());
    }

    #[test]
    fn thread_rollout_fallback_targets_only_turns_missing_items() {
        let mut state = AiState::default();
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 1,
            dedupe_key: None,
            payload: ReducerEvent::TurnStarted {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
            },
        });
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 2,
            dedupe_key: None,
            payload: ReducerEvent::TurnStarted {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-2".to_string(),
            },
        });
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 3,
            dedupe_key: None,
            payload: ReducerEvent::ItemStarted {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-2".to_string(),
                item_id: "item-2".to_string(),
                kind: "agentMessage".to_string(),
            },
        });

        let missing_turns = thread_missing_item_turn_ids(&state, "thread-1");
        assert_eq!(
            missing_turns.into_iter().collect::<Vec<_>>(),
            vec!["turn-1".to_string()]
        );
    }

    #[test]
    fn thread_rollout_fallback_ignores_items_from_other_threads_with_same_turn_id() {
        let mut state = AiState::default();
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 1,
            dedupe_key: None,
            payload: ReducerEvent::TurnStarted {
                thread_id: "thread-a".to_string(),
                turn_id: "turn-shared".to_string(),
            },
        });
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 2,
            dedupe_key: None,
            payload: ReducerEvent::TurnStarted {
                thread_id: "thread-b".to_string(),
                turn_id: "turn-shared".to_string(),
            },
        });
        let _ = state.apply_stream_event(StreamEvent {
            sequence: 3,
            dedupe_key: None,
            payload: ReducerEvent::ItemStarted {
                thread_id: "thread-b".to_string(),
                turn_id: "turn-shared".to_string(),
                item_id: "item-1".to_string(),
                kind: "agentMessage".to_string(),
            },
        });

        let missing_turns = thread_missing_item_turn_ids(&state, "thread-a");
        assert_eq!(
            missing_turns.into_iter().collect::<Vec<_>>(),
            vec!["turn-shared".to_string()]
        );
    }

    #[test]
    fn reconnect_policy_retries_only_safe_read_like_commands() {
        assert!(command_can_retry_after_reconnect(
            &AiWorkerCommand::RefreshThreads
        ));
        assert!(command_can_retry_after_reconnect(
            &AiWorkerCommand::RefreshThreadMetadata {
                thread_id: "thread-1".to_string(),
            }
        ));
        assert!(command_can_retry_after_reconnect(
            &AiWorkerCommand::SelectThread {
                thread_id: "thread-1".to_string(),
            }
        ));
        assert!(!command_can_retry_after_reconnect(
            &AiWorkerCommand::SendPrompt {
                thread_id: "thread-1".to_string(),
                prompt: Some("continue".to_string()),
                local_image_paths: Vec::new(),
                selected_skills: Vec::new(),
                skill_bindings: Vec::new(),
                session_overrides: AiTurnSessionOverrides::default(),
            }
        ));
        assert!(!command_can_retry_after_reconnect(
            &AiWorkerCommand::ResolveApproval {
                request_id: "request-1".to_string(),
                decision: AiApprovalDecision::Accept,
            }
        ));
    }

    #[test]
    fn reconnect_policy_only_treats_transport_errors_as_recoverable() {
        assert!(should_attempt_runtime_reconnect(
            &CodexIntegrationError::WebSocketTransport("socket closed".to_string())
        ));
        assert!(should_attempt_runtime_reconnect(
            &CodexIntegrationError::HostProcessIo(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "connection reset by peer",
            ))
        ));
        assert!(!should_attempt_runtime_reconnect(
            &CodexIntegrationError::JsonRpcServerError {
                code: -32602,
                message: "invalid params".to_string(),
            }
        ));
    }

    #[test]
    fn reconnect_backoff_grows_and_caps() {
        assert_eq!(reconnect_backoff(1), std::time::Duration::from_millis(250));
        assert_eq!(reconnect_backoff(2), std::time::Duration::from_millis(500));
        assert_eq!(reconnect_backoff(3), std::time::Duration::from_millis(1_000));
        assert_eq!(reconnect_backoff(9), std::time::Duration::from_millis(64_000));
    }

    #[test]
    fn panic_payload_message_prefers_string_payloads() {
        assert_eq!(panic_payload_message(Box::new("boom")), "boom");
        assert_eq!(panic_payload_message(Box::new("kaboom".to_string())), "kaboom");
    }

    #[test]
    fn dispatch_ai_worker_result_reports_panics_as_fatal_events() {
        let (event_tx, event_rx) = mpsc::channel();

        dispatch_ai_worker_result(Err(Box::new("panic payload")), "/repo-a", &event_tx);

        let event = event_rx.recv().expect("panic event should be emitted");
        assert_eq!(event.workspace_key, "/repo-a");
        match event.payload {
            AiWorkerEventPayload::Fatal(message) => {
                assert_eq!(message, "AI worker panicked: panic payload");
            }
            other => panic!("expected fatal event, got {other:?}"),
        }
    }

}
