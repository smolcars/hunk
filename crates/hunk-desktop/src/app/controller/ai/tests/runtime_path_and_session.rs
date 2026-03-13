    #[test]
    fn requested_branch_name_for_local_thread_skips_generation() {
        let fallback = "ai/local/fallback".to_string();
        let requested = requested_branch_name_for_new_thread(fallback.clone());

        assert_eq!(requested, fallback);
    }

    #[test]
    fn requested_branch_name_for_worktree_skips_generation() {
        let fallback = "ai/worktree/fallback".to_string();
        let requested = requested_branch_name_for_new_thread(fallback.clone());

        assert_eq!(requested, fallback);
    }

    #[test]
    fn requested_branch_name_for_worktree_returns_fallback_name() {
        let fallback = "ai/worktree/fallback".to_string();
        let requested = requested_branch_name_for_new_thread(fallback.clone());

        assert_eq!(requested, fallback);
    }

    #[test]
    fn background_branch_name_for_local_thread_skips_generation() {
        let generated = background_branch_name_for_new_thread(
            AiNewThreadStartMode::Local,
            "ai/local/current",
            || panic!("local thread starts should not generate background branch names"),
        );

        assert_eq!(generated, None);
    }

    #[test]
    fn background_branch_name_for_worktree_uses_generated_branch_when_available() {
        let generated = background_branch_name_for_new_thread(
            AiNewThreadStartMode::Worktree,
            "ai/worktree/current",
            || Some("ai/worktree/generated".to_string()),
        );

        assert_eq!(generated.as_deref(), Some("ai/worktree/generated"));
    }

    #[test]
    fn background_branch_name_for_worktree_skips_matching_generation() {
        let generated = background_branch_name_for_new_thread(
            AiNewThreadStartMode::Worktree,
            "ai/worktree/current",
            || Some("ai/worktree/current".to_string()),
        );

        assert_eq!(generated, None);
    }

    #[test]
    fn background_branch_name_for_worktree_skips_missing_generation() {
        let generated = background_branch_name_for_new_thread(
            AiNewThreadStartMode::Worktree,
            "ai/worktree/current",
            || None,
        );

        assert_eq!(generated, None);
    }

    #[test]
    fn review_compare_selection_ids_for_workspace_root_prefers_base_branch_over_worktree_branch() {
        let mut primary = workspace_target(
            "primary",
            WorkspaceTargetKind::PrimaryCheckout,
            "/repo",
            "Primary Checkout",
        );
        primary.branch_name = "main".to_string();

        let mut worktree = workspace_target(
            "worktree:task-1",
            WorkspaceTargetKind::LinkedWorktree,
            "/repo/worktrees/task-1",
            "task-1",
        );
        worktree.branch_name = "feature/task-1".to_string();

        let workspace_targets = vec![primary.clone(), worktree.clone()];
        let sources = vec![
            ReviewCompareSourceOption::from_workspace_target(&primary),
            ReviewCompareSourceOption::from_workspace_target(&worktree),
            ReviewCompareSourceOption::from_branch(&local_branch("main", false)),
            ReviewCompareSourceOption::from_branch(&local_branch("feature/task-1", true)),
        ];

        assert_eq!(
            review_compare_selection_ids_for_workspace_root(
                &sources,
                &workspace_targets,
                std::path::Path::new("/repo/worktrees/task-1"),
                Some("main"),
                Some("main"),
            ),
            Some((Some("branch:main".to_string()), Some(sources[1].id.clone()))),
        );
    }

    #[test]
    fn review_compare_selection_ids_for_workspace_root_falls_back_to_worktree_branch() {
        let mut primary = workspace_target(
            "primary",
            WorkspaceTargetKind::PrimaryCheckout,
            "/repo",
            "Primary Checkout",
        );
        primary.branch_name = "main".to_string();

        let mut worktree = workspace_target(
            "worktree:task-1",
            WorkspaceTargetKind::LinkedWorktree,
            "/repo/worktrees/task-1",
            "task-1",
        );
        worktree.branch_name = "feature/task-1".to_string();

        let workspace_targets = vec![primary.clone(), worktree.clone()];
        let sources = vec![
            ReviewCompareSourceOption::from_workspace_target(&primary),
            ReviewCompareSourceOption::from_workspace_target(&worktree),
            ReviewCompareSourceOption::from_branch(&local_branch("feature/task-1", true)),
        ];

        assert_eq!(
            review_compare_selection_ids_for_workspace_root(
                &sources,
                &workspace_targets,
                std::path::Path::new("/repo/worktrees/task-1"),
                Some("release/1.0"),
                Some("main"),
            ),
            Some((
                Some("branch:feature/task-1".to_string()),
                Some(sources[1].id.clone()),
            )),
        );
    }

    #[test]
    fn workspace_mad_max_mode_defaults_to_true_when_missing() {
        let state = AppState::default();
        assert!(workspace_mad_max_mode(&state, Some("/repo")));
        assert!(workspace_mad_max_mode(&state, None));
    }

    #[test]
    fn workspace_mad_max_mode_reads_per_workspace_flags() {
        let state = AppState {
            last_project_path: None,
            last_workspace_target_by_repo: Default::default(),
            review_compare_selection_by_repo: Default::default(),
            ai_workspace_mad_max: [
                ("/repo-a".to_string(), false),
                ("/repo-b".to_string(), false),
            ]
            .into_iter()
            .collect(),
            ai_workspace_include_hidden_models: Default::default(),
            ai_workspace_session_overrides: Default::default(),
            git_workflow_cache: None,
            git_recent_commits_cache: None,
        };
        assert!(!workspace_mad_max_mode(&state, Some("/repo-a")));
        assert!(!workspace_mad_max_mode(&state, Some("/repo-b")));
        assert!(workspace_mad_max_mode(&state, Some("/repo-c")));
    }

    #[test]
    fn seed_ai_workspace_preferences_keeps_mad_max_enabled_by_default() {
        let mut state = AppState::default();
        seed_ai_workspace_preferences(&mut state, "/repo-b", true, true);

        assert!(workspace_mad_max_mode(&state, Some("/repo-b")));
        assert!(
            !state.ai_workspace_mad_max.contains_key("/repo-b"),
            "default-on Mad Max should not require a stored override"
        );
    }

    #[test]
    fn workspace_include_hidden_models_defaults_to_true_when_missing() {
        let state = AppState::default();
        assert!(workspace_include_hidden_models(&state, Some("/repo")));
        assert!(workspace_include_hidden_models(&state, None));
    }

    #[test]
    fn workspace_include_hidden_models_reads_per_workspace_flags() {
        let state = AppState {
            last_project_path: None,
            last_workspace_target_by_repo: Default::default(),
            review_compare_selection_by_repo: Default::default(),
            ai_workspace_mad_max: Default::default(),
            ai_workspace_include_hidden_models: [
                ("/repo-a".to_string(), true),
                ("/repo-b".to_string(), false),
            ]
            .into_iter()
            .collect(),
            ai_workspace_session_overrides: Default::default(),
            git_workflow_cache: None,
            git_recent_commits_cache: None,
        };
        assert!(workspace_include_hidden_models(&state, Some("/repo-a")));
        assert!(!workspace_include_hidden_models(&state, Some("/repo-b")));
        assert!(workspace_include_hidden_models(&state, Some("/repo-c")));
    }

    #[test]
    fn resolved_ai_workspace_cwd_prefers_repo_root_when_paths_are_related() {
        let project = PathBuf::from("/repo/subdir");
        let repo_root = PathBuf::from("/repo");
        assert_eq!(
            resolved_ai_workspace_cwd(Some(project.as_path()), Some(repo_root.as_path())),
            Some(repo_root),
        );
    }

    #[test]
    fn resolved_ai_workspace_cwd_prefers_selected_project_when_repo_root_is_stale() {
        let project = PathBuf::from("/repo-b");
        let stale_repo_root = PathBuf::from("/repo-a");
        assert_eq!(
            resolved_ai_workspace_cwd(Some(project.as_path()), Some(stale_repo_root.as_path())),
            Some(project),
        );
    }

    #[test]
    fn ai_completion_reload_workspace_root_targets_destination_workspace() {
        assert_eq!(
            ai_completion_reload_workspace_root(Some("/repo/worktrees/task-1")),
            Some(PathBuf::from("/repo/worktrees/task-1")),
        );
        assert_eq!(ai_completion_reload_workspace_root(None), None);
    }

    #[test]
    fn drain_ai_worker_events_preserves_final_fatal_before_disconnect() {
        let (event_tx, event_rx) = mpsc::channel();
        event_tx
            .send(crate::app::ai_runtime::AiWorkerEvent {
                workspace_key: "/repo-a".to_string(),
                payload: crate::app::ai_runtime::AiWorkerEventPayload::Fatal("boom".to_string()),
            })
            .expect("fatal event should send");
        drop(event_tx);

        let (events, disconnected) = drain_ai_worker_events(&event_rx);
        assert!(disconnected);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events.first(),
            Some(crate::app::ai_runtime::AiWorkerEvent { workspace_key, payload: crate::app::ai_runtime::AiWorkerEventPayload::Fatal(message) })
                if workspace_key == "/repo-a" && message == "boom"
        ));
    }

    #[test]
    fn normalized_thread_session_state_drops_empty_entries() {
        assert_eq!(
            normalized_thread_session_state(AiThreadSessionState::default()),
            None
        );
    }

    #[test]
    fn normalized_thread_session_state_preserves_selected_overrides() {
        let session = AiThreadSessionState {
            model: Some("gpt-5-codex".to_string()),
            effort: Some("high".to_string()),
            collaboration_mode: AiCollaborationModeSelection::Plan,
            service_tier: Some(AiServiceTierSelection::Fast),
        };
        assert_eq!(
            normalized_thread_session_state(session.clone()),
            Some(session),
        );
    }

    #[test]
    fn normalized_thread_session_state_drops_standard_service_tier_only() {
        let session = AiThreadSessionState {
            model: None,
            effort: None,
            collaboration_mode: AiCollaborationModeSelection::Default,
            service_tier: Some(AiServiceTierSelection::Standard),
        };
        assert_eq!(normalized_thread_session_state(session), None);
    }

    #[test]
    fn normalized_thread_session_state_strips_standard_service_tier_from_overrides() {
        let session = AiThreadSessionState {
            model: Some("gpt-5-codex".to_string()),
            effort: None,
            collaboration_mode: AiCollaborationModeSelection::Default,
            service_tier: Some(AiServiceTierSelection::Standard),
        };
        assert_eq!(
            normalized_thread_session_state(session),
            Some(AiThreadSessionState {
                model: Some("gpt-5-codex".to_string()),
                effort: None,
                collaboration_mode: AiCollaborationModeSelection::Default,
                service_tier: None,
            })
        );
    }

    #[test]
    fn normalized_thread_session_state_drops_default_collaboration_mode() {
        let session = AiThreadSessionState {
            model: None,
            effort: None,
            collaboration_mode: AiCollaborationModeSelection::Default,
            service_tier: None,
        };
        assert_eq!(normalized_thread_session_state(session), None);
    }

    #[test]
    fn normalized_ai_session_selection_preserves_server_default_when_unset() {
        let models = vec![ai_model(
            "gpt-5.3-codex",
            "5.3 Codex",
            true,
            &[ReasoningEffort::Low, ReasoningEffort::Medium],
            ReasoningEffort::Medium,
        )];

        assert_eq!(
            normalized_ai_session_selection(models.as_slice(), None, None),
            (None, None),
        );
    }

    #[test]
    fn normalized_ai_session_selection_preserves_selection_while_models_load() {
        assert_eq!(
            normalized_ai_session_selection(
                &[],
                Some("gpt-5.3-codex".to_string()),
                Some("high".to_string()),
            ),
            (
                Some("gpt-5.3-codex".to_string()),
                Some("high".to_string()),
            ),
        );
    }

    #[test]
    fn normalized_ai_session_selection_preserves_model_default_effort_when_unset() {
        let models = vec![ai_model(
            "gpt-5.3-codex",
            "5.3 Codex",
            true,
            &[ReasoningEffort::Low, ReasoningEffort::Medium],
            ReasoningEffort::Medium,
        )];

        assert_eq!(
            normalized_ai_session_selection(
                models.as_slice(),
                Some("gpt-5.3-codex".to_string()),
                None,
            ),
            (Some("gpt-5.3-codex".to_string()), None),
        );
    }

    #[test]
    fn command_name_without_path_detection_is_stable() {
        assert!(is_command_name_without_path(std::path::Path::new("codex")));
        assert!(!is_command_name_without_path(std::path::Path::new("./codex")));
        assert!(!is_command_name_without_path(std::path::Path::new("/usr/bin/codex")));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_command_resolution_prefers_spawnable_launcher_on_path() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hunk-codex-cmd-{unique}"));
        let bin_dir = root.join("bin");
        std::fs::create_dir_all(&bin_dir).expect("bin dir should be created");
        std::fs::write(bin_dir.join("codex"), "#!/bin/sh\n").expect("unix shim should be written");
        let launcher_path = bin_dir.join("codex.cmd");
        std::fs::write(&launcher_path, "@echo off\r\n").expect("fake launcher should be written");

        let resolved = resolve_windows_command_path_from_env(
            std::path::Path::new("codex"),
            Some(std::env::join_paths([bin_dir.as_path()]).expect("path should join")),
            Some(OsString::from(".COM;.EXE;.BAT;.CMD")),
        );

        assert_eq!(
            resolved.map(|path| path.to_string_lossy().to_ascii_lowercase()),
            Some(launcher_path.to_string_lossy().to_ascii_lowercase()),
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_explicit_command_path_prefers_adjacent_launcher_over_unix_shim() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hunk-codex-explicit-{unique}"));
        std::fs::create_dir_all(&root).expect("root dir should be created");
        let unix_shim_path = root.join("codex");
        std::fs::write(&unix_shim_path, "#!/bin/sh\n").expect("unix shim should be written");
        let launcher_path = root.join("codex.cmd");
        std::fs::write(&launcher_path, "@echo off\r\n").expect("fake launcher should be written");

        let resolved = resolve_windows_command_path(unix_shim_path.as_path());

        assert_eq!(
            resolved.map(|path| path.to_string_lossy().to_ascii_lowercase()),
            Some(launcher_path.to_string_lossy().to_ascii_lowercase()),
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn bundled_codex_resolution_picks_existing_runtime_candidate() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hunk-codex-runtime-{unique}"));
        let exe_dir = root.join("bin");
        std::fs::create_dir_all(&exe_dir).expect("exe dir should be created");
        let exe_path = exe_dir.join("hunk");
        std::fs::write(&exe_path, "").expect("fake exe should be written");

        let runtime_path = exe_dir
            .join("codex-runtime")
            .join(codex_runtime_platform_dir())
            .join(if cfg!(target_os = "windows") {
                "codex.cmd"
            } else {
                codex_runtime_binary_name()
            });
        std::fs::create_dir_all(
            runtime_path
                .parent()
                .expect("runtime parent should exist"),
        )
        .expect("runtime dir should be created");
        write_fake_codex_launcher(runtime_path.as_path());

        let resolved = resolve_bundled_codex_executable_from_exe(exe_path.as_path());
        assert_eq!(resolved, Some(runtime_path));

        let candidates = bundled_codex_executable_candidates(exe_path.as_path());
        #[cfg(target_os = "windows")]
        assert!(candidates.iter().any(|candidate| {
            candidate.ends_with(
                PathBuf::from("codex-runtime")
                    .join(codex_runtime_platform_dir())
                    .join("codex.cmd"),
            )
        }));
        #[cfg(not(target_os = "windows"))]
        assert!(candidates.iter().any(|candidate| {
            candidate.ends_with(
                PathBuf::from("codex-runtime")
                    .join(codex_runtime_platform_dir())
                    .join(codex_runtime_binary_name()),
            )
        }));

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn bundled_codex_resolution_prefers_windows_cmd_over_exe() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hunk-codex-runtime-bad-{unique}"));
        let exe_dir = root.join("bin");
        std::fs::create_dir_all(&exe_dir).expect("exe dir should be created");
        let exe_path = exe_dir.join("hunk");
        std::fs::write(&exe_path, "").expect("fake exe should be written");

        let runtime_path = exe_dir
            .join("codex-runtime")
            .join(codex_runtime_platform_dir())
            .join("codex.exe");
        let launcher_path = exe_dir
            .join("codex-runtime")
            .join(codex_runtime_platform_dir())
            .join("codex.cmd");
        std::fs::create_dir_all(
            runtime_path
                .parent()
                .expect("runtime parent should exist"),
        )
        .expect("runtime dir should be created");
        write_fake_windows_pe(runtime_path.as_path());
        write_fake_codex_launcher(launcher_path.as_path());

        let resolved = resolve_bundled_codex_executable_from_exe(exe_path.as_path());
        assert_eq!(resolved, Some(launcher_path));

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn bundled_codex_resolution_falls_back_to_resources_runtime_candidate() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hunk-codex-runtime-resources-{unique}"));
        std::fs::create_dir_all(&root).expect("root dir should be created");
        let exe_path = root.join("hunk");
        std::fs::write(&exe_path, "").expect("fake exe should be written");

        let runtime_path = root
            .join("Resources")
            .join("codex-runtime")
            .join(codex_runtime_platform_dir())
            .join(if cfg!(target_os = "windows") {
                "codex.cmd"
            } else {
                codex_runtime_binary_name()
            });
        std::fs::create_dir_all(
            runtime_path
                .parent()
                .expect("runtime parent should exist"),
        )
        .expect("runtime dir should be created");
        write_fake_codex_launcher(runtime_path.as_path());

        let resolved = resolve_bundled_codex_executable_from_exe(exe_path.as_path());
        assert_eq!(resolved, Some(runtime_path.clone()));

        let candidates = bundled_codex_executable_candidates(exe_path.as_path());
        #[cfg(target_os = "windows")]
        assert!(candidates.iter().any(|candidate| {
            candidate.ends_with(
                PathBuf::from("Resources")
                    .join("codex-runtime")
                    .join(codex_runtime_platform_dir())
                    .join("codex.cmd"),
            )
        }));
        #[cfg(not(target_os = "windows"))]
        assert!(candidates.iter().any(|candidate| {
            candidate.ends_with(
                PathBuf::from("Resources")
                    .join("codex-runtime")
                    .join(codex_runtime_platform_dir())
                    .join(codex_runtime_binary_name()),
            )
        }));

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn bundled_codex_resolution_falls_back_to_packager_linux_resource_candidate() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hunk-codex-runtime-lib-{unique}"));
        let exe_path = root.join("usr").join("bin").join("hunk_desktop");
        std::fs::create_dir_all(
            exe_path
                .parent()
                .expect("exe parent directory should exist"),
        )
        .expect("exe dir should be created");
        std::fs::write(&exe_path, "").expect("fake exe should be written");

        let runtime_path = root
            .join("usr")
            .join("lib")
            .join("hunk_desktop")
            .join("codex-runtime")
            .join(codex_runtime_platform_dir())
            .join(codex_runtime_binary_name());
        std::fs::create_dir_all(
            runtime_path
                .parent()
                .expect("runtime parent should exist"),
        )
        .expect("runtime dir should be created");
        write_fake_codex_launcher(runtime_path.as_path());

        let resolved = resolve_bundled_codex_executable_from_exe(exe_path.as_path());
        assert_eq!(resolved, Some(runtime_path.clone()));

        let candidates = bundled_codex_executable_candidates(exe_path.as_path());
        assert!(candidates.iter().any(|candidate| {
            candidate.ends_with(
                PathBuf::from("lib")
                    .join("hunk_desktop")
                    .join("codex-runtime")
                    .join(codex_runtime_platform_dir())
                    .join(codex_runtime_binary_name()),
            )
        }));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn normalized_user_input_answers_defaults_to_first_option_or_blank() {
        let request = AiPendingUserInputRequest {
            request_id: "req-1".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            item_id: "item-1".to_string(),
            questions: vec![
                AiPendingUserInputQuestion {
                    id: "q-option".to_string(),
                    header: "Header".to_string(),
                    question: "Pick one".to_string(),
                    is_other: false,
                    is_secret: false,
                    options: vec![
                        AiPendingUserInputQuestionOption {
                            label: "first".to_string(),
                            description: "first option".to_string(),
                        },
                        AiPendingUserInputQuestionOption {
                            label: "second".to_string(),
                            description: "second option".to_string(),
                        },
                    ],
                },
                AiPendingUserInputQuestion {
                    id: "q-empty".to_string(),
                    header: "Free text".to_string(),
                    question: "Enter value".to_string(),
                    is_other: true,
                    is_secret: false,
                    options: Vec::new(),
                },
            ],
        };

        let answers = normalized_user_input_answers(&request, None);
        assert_eq!(
            answers.get("q-option"),
            Some(&vec!["first".to_string()])
        );
        assert_eq!(answers.get("q-empty"), Some(&vec![String::new()]));
    }

    #[test]
    fn normalized_user_input_answers_preserves_existing_answers() {
        let request = AiPendingUserInputRequest {
            request_id: "req-2".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            item_id: "item-2".to_string(),
            questions: vec![AiPendingUserInputQuestion {
                id: "q-option".to_string(),
                header: "Header".to_string(),
                question: "Pick one".to_string(),
                is_other: false,
                is_secret: false,
                options: vec![AiPendingUserInputQuestionOption {
                    label: "default".to_string(),
                    description: "default option".to_string(),
                }],
            }],
        };
        let previous = [("q-option".to_string(), vec!["custom".to_string()])]
            .into_iter()
            .collect();

        let answers = normalized_user_input_answers(&request, Some(&previous));
        assert_eq!(
            answers.get("q-option"),
            Some(&vec!["custom".to_string()])
        );
    }

    #[test]
    fn supported_image_path_check_is_case_insensitive() {
        assert!(is_supported_ai_image_path(std::path::Path::new("image.PNG")));
        assert!(is_supported_ai_image_path(std::path::Path::new("shot.JpEg")));
        assert!(is_supported_ai_image_path(std::path::Path::new("anim.GIF")));
    }

    #[test]
    fn unsupported_image_path_without_extension_is_rejected() {
        assert!(!is_supported_ai_image_path(std::path::Path::new("image")));
        assert!(!is_supported_ai_image_path(std::path::Path::new("archive.zip")));
    }

    #[test]
    fn attachment_status_message_reports_only_problem_cases() {
        assert_eq!(ai_attachment_status_message(1, 1), None);
        assert_eq!(ai_attachment_status_message(3, 3), None);
        assert_eq!(
            ai_attachment_status_message(3, 1),
            Some("Attached 1 image. Skipped 2 unsupported or duplicate files.".to_string())
        );
        assert_eq!(
            ai_attachment_status_message(1, 0),
            Some("File is not a supported image or is already attached.".to_string())
        );
        assert_eq!(
            ai_attachment_status_message(2, 0),
            Some("No files were supported images or were already attached.".to_string())
        );
    }
