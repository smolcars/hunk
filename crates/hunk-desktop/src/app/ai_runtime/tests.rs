#[cfg(test)]
mod ai_tests {
    use std::collections::HashMap;

    use codex_app_server_protocol::AccountLoginCompletedNotification;
    use codex_app_server_protocol::AskForApproval;
    use codex_app_server_protocol::RateLimitSnapshot;
    use codex_app_server_protocol::RateLimitWindow;
    use codex_app_server_protocol::RequestId;
    use codex_app_server_protocol::SandboxMode;
    use codex_app_server_protocol::SandboxPolicy;
    use codex_app_server_protocol::ThreadStartParams;
    use codex_app_server_protocol::TurnStartParams;
    use codex_protocol::config_types::ServiceTier;
    use hunk_codex::errors::CodexIntegrationError;
    use hunk_codex::state::AiState;
    use hunk_codex::state::ReducerEvent;
    use hunk_codex::state::StreamEvent;
    use hunk_domain::state::AiServiceTierSelection;

    use super::AiApprovalDecision;
    use super::AiWorkerCommand;
    use super::AiTurnSessionOverrides;
    use super::apply_login_completed_state;
    use super::apply_thread_start_policy;
    use super::apply_thread_start_session_overrides;
    use super::apply_turn_start_policy;
    use super::command_can_retry_after_reconnect;
    use super::reconnect_backoff;
    use super::map_command_approval_decision;
    use super::map_file_change_approval_decision;
    use super::preferred_rate_limit_snapshot;
    use super::request_id_key;
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
            codex_app_server_protocol::CommandExecutionApprovalDecision::Accept
        );
        assert_eq!(
            map_file_change_approval_decision(AiApprovalDecision::Decline),
            codex_app_server_protocol::FileChangeApprovalDecision::Decline
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
    fn stale_turn_retry_helper_matches_jsonrpc_server_errors_only() {
        assert!(should_retry_stale_turn_after_steer_error(
            &CodexIntegrationError::JsonRpcServerError {
                code: -32602,
                message: "expected_turn_id is stale".to_string(),
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
            &AiWorkerCommand::SelectThread {
                thread_id: "thread-1".to_string(),
            }
        ));
        assert!(!command_can_retry_after_reconnect(
            &AiWorkerCommand::SendPrompt {
                thread_id: "thread-1".to_string(),
                prompt: Some("continue".to_string()),
                local_image_paths: Vec::new(),
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
}
