fn settings_shortcut_input(
    value: &[String],
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut Context<DiffViewer>,
) -> Entity<InputState> {
    cx.new(|cx| {
        let mut state = InputState::new(window, cx).placeholder(placeholder);
        state.set_value(shortcut_lines(value), window, cx);
        state
    })
}

fn read_shortcut_input(input: &Entity<InputState>, cx: &Context<DiffViewer>) -> Vec<String> {
    parse_shortcut_lines(input.read(cx).value().as_ref())
}

fn validate_shortcut_list(action: &str, shortcuts: &[String]) -> Result<(), String> {
    for shortcut in shortcuts {
        for keystroke in shortcut.split_whitespace() {
            if let Err(err) = gpui::Keystroke::parse(keystroke) {
                return Err(format!("{action}: invalid shortcut `{shortcut}` ({err})"));
            }
        }
    }
    Ok(())
}

fn validate_keyboard_shortcuts(shortcuts: &KeyboardShortcuts) -> Result<(), String> {
    validate_shortcut_list("Select Next Line", &shortcuts.select_next_line)?;
    validate_shortcut_list("Select Previous Line", &shortcuts.select_previous_line)?;
    validate_shortcut_list(
        "Extend Selection Down",
        &shortcuts.extend_selection_next_line,
    )?;
    validate_shortcut_list(
        "Extend Selection Up",
        &shortcuts.extend_selection_previous_line,
    )?;
    validate_shortcut_list("Copy Selection", &shortcuts.copy_selection)?;
    validate_shortcut_list("Select All Diff Rows", &shortcuts.select_all_diff_rows)?;
    validate_shortcut_list("Next Hunk", &shortcuts.next_hunk)?;
    validate_shortcut_list("Previous Hunk", &shortcuts.previous_hunk)?;
    validate_shortcut_list("Next File", &shortcuts.next_file)?;
    validate_shortcut_list("Previous File", &shortcuts.previous_file)?;
    validate_shortcut_list("View Review File", &shortcuts.view_current_review_file)?;
    validate_shortcut_list("Toggle File Tree", &shortcuts.toggle_sidebar_tree)?;
    validate_shortcut_list("Switch to Files View", &shortcuts.switch_to_files_view)?;
    validate_shortcut_list("Switch to Review View", &shortcuts.switch_to_review_view)?;
    validate_shortcut_list("Switch to Git View", &shortcuts.switch_to_git_view)?;
    validate_shortcut_list("Switch to AI View", &shortcuts.switch_to_ai_view)?;
    validate_shortcut_list("Toggle Terminal", &shortcuts.toggle_ai_terminal_drawer)?;
    validate_shortcut_list("Open Project", &shortcuts.open_project)?;
    validate_shortcut_list("Save Current File", &shortcuts.save_current_file)?;
    validate_shortcut_list("Next Editor Tab", &shortcuts.next_editor_tab)?;
    validate_shortcut_list("Previous Editor Tab", &shortcuts.previous_editor_tab)?;
    validate_shortcut_list("Close Editor Tab", &shortcuts.close_editor_tab)?;
    validate_shortcut_list("Open Settings", &shortcuts.open_settings)?;
    validate_shortcut_list("Quit App", &shortcuts.quit_app)?;
    validate_shortcut_list("Tree: New File", &shortcuts.repo_tree_new_file)?;
    validate_shortcut_list("Tree: New Folder", &shortcuts.repo_tree_new_folder)?;
    validate_shortcut_list("Tree: Rename File", &shortcuts.repo_tree_rename_file)?;
    Ok(())
}

impl DiffViewer {
    pub(super) const fn reduced_motion_enabled(&self) -> bool {
        self.config.reduce_motion
    }

    pub(super) fn animation_duration_ms(&self, default_ms: u64) -> std::time::Duration {
        if self.reduced_motion_enabled() {
            std::time::Duration::ZERO
        } else {
            std::time::Duration::from_millis(default_ms)
        }
    }

    pub(super) fn open_settings_action(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_draft.is_some() {
            self.close_settings_and_refocus(window, cx);
        } else {
            self.open_settings(window, cx);
        }
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_draft.is_some() {
            return;
        }

        let shortcuts = SettingsShortcutInputs {
            select_next_line: settings_shortcut_input(
                &self.config.keyboard_shortcuts.select_next_line,
                "Comma-separated shortcuts, e.g. down, j",
                window,
                cx,
            ),
            select_previous_line: settings_shortcut_input(
                &self.config.keyboard_shortcuts.select_previous_line,
                "Comma-separated shortcuts, e.g. up, k",
                window,
                cx,
            ),
            extend_selection_next_line: settings_shortcut_input(
                &self.config.keyboard_shortcuts.extend_selection_next_line,
                "Comma-separated shortcuts, e.g. shift-down",
                window,
                cx,
            ),
            extend_selection_previous_line: settings_shortcut_input(
                &self.config.keyboard_shortcuts.extend_selection_previous_line,
                "Comma-separated shortcuts, e.g. shift-up",
                window,
                cx,
            ),
            copy_selection: settings_shortcut_input(
                &self.config.keyboard_shortcuts.copy_selection,
                "Comma-separated shortcuts, e.g. cmd-c, ctrl-c",
                window,
                cx,
            ),
            select_all_diff_rows: settings_shortcut_input(
                &self.config.keyboard_shortcuts.select_all_diff_rows,
                "Comma-separated shortcuts, e.g. cmd-a, ctrl-a",
                window,
                cx,
            ),
            next_hunk: settings_shortcut_input(
                &self.config.keyboard_shortcuts.next_hunk,
                "Comma-separated shortcuts, e.g. f7",
                window,
                cx,
            ),
            previous_hunk: settings_shortcut_input(
                &self.config.keyboard_shortcuts.previous_hunk,
                "Comma-separated shortcuts, e.g. shift-f7",
                window,
                cx,
            ),
            next_file: settings_shortcut_input(
                &self.config.keyboard_shortcuts.next_file,
                "Comma-separated shortcuts, e.g. alt-down",
                window,
                cx,
            ),
            previous_file: settings_shortcut_input(
                &self.config.keyboard_shortcuts.previous_file,
                "Comma-separated shortcuts, e.g. alt-up",
                window,
                cx,
            ),
            view_current_review_file: settings_shortcut_input(
                &self.config.keyboard_shortcuts.view_current_review_file,
                "Comma-separated shortcuts, e.g. g space",
                window,
                cx,
            ),
            toggle_sidebar_tree: settings_shortcut_input(
                &self.config.keyboard_shortcuts.toggle_sidebar_tree,
                "Comma-separated shortcuts, e.g. cmd-b, ctrl-b",
                window,
                cx,
            ),
            switch_to_files_view: settings_shortcut_input(
                &self.config.keyboard_shortcuts.switch_to_files_view,
                "Comma-separated shortcuts, e.g. cmd-1, ctrl-1",
                window,
                cx,
            ),
            switch_to_review_view: settings_shortcut_input(
                &self.config.keyboard_shortcuts.switch_to_review_view,
                "Comma-separated shortcuts, e.g. cmd-2, ctrl-2",
                window,
                cx,
            ),
            switch_to_git_view: settings_shortcut_input(
                &self.config.keyboard_shortcuts.switch_to_git_view,
                "Comma-separated shortcuts, e.g. cmd-3, ctrl-3",
                window,
                cx,
            ),
            toggle_ai_terminal_drawer: settings_shortcut_input(
                &self.config.keyboard_shortcuts.toggle_ai_terminal_drawer,
                "Comma-separated shortcuts, e.g. cmd-j, ctrl-j",
                window,
                cx,
            ),
            open_project: settings_shortcut_input(
                &self.config.keyboard_shortcuts.open_project,
                "Comma-separated shortcuts, e.g. cmd-shift-o, ctrl-shift-o",
                window,
                cx,
            ),
            save_current_file: settings_shortcut_input(
                &self.config.keyboard_shortcuts.save_current_file,
                "Comma-separated shortcuts, e.g. cmd-s, ctrl-s",
                window,
                cx,
            ),
            next_editor_tab: settings_shortcut_input(
                &self.config.keyboard_shortcuts.next_editor_tab,
                "Comma-separated shortcuts, e.g. cmd-}, ctrl-shift-]",
                window,
                cx,
            ),
            previous_editor_tab: settings_shortcut_input(
                &self.config.keyboard_shortcuts.previous_editor_tab,
                "Comma-separated shortcuts, e.g. cmd-{, ctrl-shift-[",
                window,
                cx,
            ),
            close_editor_tab: settings_shortcut_input(
                &self.config.keyboard_shortcuts.close_editor_tab,
                "Comma-separated shortcuts, e.g. cmd-w, ctrl-w",
                window,
                cx,
            ),
            open_settings: settings_shortcut_input(
                &self.config.keyboard_shortcuts.open_settings,
                "Comma-separated shortcuts, e.g. cmd-, , ctrl-,",
                window,
                cx,
            ),
            quit_app: settings_shortcut_input(
                &self.config.keyboard_shortcuts.quit_app,
                "Comma-separated shortcuts, e.g. cmd-q",
                window,
                cx,
            ),
            repo_tree_new_file: settings_shortcut_input(
                &self.config.keyboard_shortcuts.repo_tree_new_file,
                "Comma-separated shortcuts, e.g. %",
                window,
                cx,
            ),
            repo_tree_new_folder: settings_shortcut_input(
                &self.config.keyboard_shortcuts.repo_tree_new_folder,
                "Comma-separated shortcuts, e.g. d",
                window,
                cx,
            ),
            repo_tree_rename_file: settings_shortcut_input(
                &self.config.keyboard_shortcuts.repo_tree_rename_file,
                "Comma-separated shortcuts, e.g. shift-r",
                window,
                cx,
            ),
        };

        let terminal = SettingsTerminalState {
            shell_choice: settings_terminal_shell_choice(&self.config.terminal.shell),
            custom_program: settings_terminal_input(
                settings_terminal_custom_program(&self.config.terminal.shell).as_str(),
                terminal_custom_placeholder(),
                window,
                cx,
            ),
            original_shell: self.config.terminal.shell.clone(),
            inherit_login_environment: self.config.terminal.inherit_login_environment,
            hydrate_app_environment_on_launch: self
                .config
                .terminal
                .hydrate_app_environment_on_launch,
        };

        self.settings_draft = Some(SettingsDraft {
            category: SettingsCategory::Ui,
            theme: self.config.theme,
            reduce_motion: self.config.reduce_motion,
            show_fps_counter: self.config.show_fps_counter,
            auto_update_enabled: self.config.auto_update_enabled,
            terminal,
            shortcuts,
            error_message: None,
        });
        cx.notify();
    }

    pub(super) fn close_settings(&mut self, cx: &mut Context<Self>) {
        if self.settings_draft.is_none() {
            return;
        }
        self.settings_draft = None;
        cx.notify();
    }

    pub(super) fn close_settings_and_refocus(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_settings(cx);
        self.focus_handle.focus(window, cx);
    }

    pub(super) fn select_settings_category(
        &mut self,
        category: SettingsCategory,
        cx: &mut Context<Self>,
    ) {
        let Some(settings) = self.settings_draft.as_mut() else {
            return;
        };
        if settings.category == category {
            return;
        }
        settings.category = category;
        settings.error_message = None;
        cx.notify();
    }

    pub(super) fn set_settings_theme(
        &mut self,
        theme: ThemePreference,
        cx: &mut Context<Self>,
    ) {
        let Some(settings) = self.settings_draft.as_mut() else {
            return;
        };
        if settings.theme == theme {
            return;
        }
        settings.theme = theme;
        settings.error_message = None;
        cx.notify();
    }

    pub(super) fn set_settings_reduce_motion(
        &mut self,
        reduce_motion: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(settings) = self.settings_draft.as_mut() else {
            return;
        };
        if settings.reduce_motion == reduce_motion {
            return;
        }
        settings.reduce_motion = reduce_motion;
        settings.error_message = None;
        cx.notify();
    }

    pub(super) fn set_settings_show_fps_counter(
        &mut self,
        show_fps_counter: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(settings) = self.settings_draft.as_mut() else {
            return;
        };
        if settings.show_fps_counter == show_fps_counter {
            return;
        }
        settings.show_fps_counter = show_fps_counter;
        settings.error_message = None;
        cx.notify();
    }

    pub(super) fn set_settings_auto_update_enabled(
        &mut self,
        auto_update_enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(settings) = self.settings_draft.as_mut() else {
            return;
        };
        if settings.auto_update_enabled == auto_update_enabled {
            return;
        }
        settings.auto_update_enabled = auto_update_enabled;
        settings.error_message = None;
        cx.notify();
    }

    pub(super) fn set_settings_terminal_shell_choice(
        &mut self,
        shell_choice: SettingsTerminalShellChoice,
        cx: &mut Context<Self>,
    ) {
        let Some(settings) = self.settings_draft.as_mut() else {
            return;
        };
        if settings.terminal.shell_choice == shell_choice {
            return;
        }
        settings.terminal.shell_choice = shell_choice;
        settings.error_message = None;
        cx.notify();
    }

    pub(super) fn set_settings_terminal_inherit_login_environment(
        &mut self,
        inherit_login_environment: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(settings) = self.settings_draft.as_mut() else {
            return;
        };
        if settings.terminal.inherit_login_environment == inherit_login_environment {
            return;
        }
        settings.terminal.inherit_login_environment = inherit_login_environment;
        settings.error_message = None;
        cx.notify();
    }

    pub(super) fn set_settings_terminal_hydrate_app_environment_on_launch(
        &mut self,
        hydrate_app_environment_on_launch: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(settings) = self.settings_draft.as_mut() else {
            return;
        };
        if settings.terminal.hydrate_app_environment_on_launch == hydrate_app_environment_on_launch {
            return;
        }
        settings.terminal.hydrate_app_environment_on_launch = hydrate_app_environment_on_launch;
        settings.error_message = None;
        cx.notify();
    }

    pub(super) fn save_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (
            theme,
            reduce_motion,
            show_fps_counter,
            auto_update_enabled,
            terminal,
            keyboard_shortcuts,
        ) = {
            let Some(settings) = self.settings_draft.as_mut() else {
                return;
            };

            let keyboard_shortcuts = KeyboardShortcuts {
                select_next_line: read_shortcut_input(&settings.shortcuts.select_next_line, cx),
                select_previous_line: read_shortcut_input(
                    &settings.shortcuts.select_previous_line,
                    cx,
                ),
                extend_selection_next_line: read_shortcut_input(
                    &settings.shortcuts.extend_selection_next_line,
                    cx,
                ),
                extend_selection_previous_line: read_shortcut_input(
                    &settings.shortcuts.extend_selection_previous_line,
                    cx,
                ),
                copy_selection: read_shortcut_input(&settings.shortcuts.copy_selection, cx),
                select_all_diff_rows: read_shortcut_input(
                    &settings.shortcuts.select_all_diff_rows,
                    cx,
                ),
                next_hunk: read_shortcut_input(&settings.shortcuts.next_hunk, cx),
                previous_hunk: read_shortcut_input(&settings.shortcuts.previous_hunk, cx),
                next_file: read_shortcut_input(&settings.shortcuts.next_file, cx),
                previous_file: read_shortcut_input(&settings.shortcuts.previous_file, cx),
                view_current_review_file: read_shortcut_input(
                    &settings.shortcuts.view_current_review_file,
                    cx,
                ),
                toggle_sidebar_tree: read_shortcut_input(
                    &settings.shortcuts.toggle_sidebar_tree,
                    cx,
                ),
                switch_to_files_view: read_shortcut_input(
                    &settings.shortcuts.switch_to_files_view,
                    cx,
                ),
                switch_to_review_view: read_shortcut_input(
                    &settings.shortcuts.switch_to_review_view,
                    cx,
                ),
                switch_to_git_view: read_shortcut_input(
                    &settings.shortcuts.switch_to_git_view,
                    cx,
                ),
                switch_to_ai_view: self.config.keyboard_shortcuts.switch_to_ai_view.clone(),
                toggle_ai_terminal_drawer: read_shortcut_input(
                    &settings.shortcuts.toggle_ai_terminal_drawer,
                    cx,
                ),
                open_project: read_shortcut_input(&settings.shortcuts.open_project, cx),
                save_current_file: read_shortcut_input(
                    &settings.shortcuts.save_current_file,
                    cx,
                ),
                next_editor_tab: read_shortcut_input(&settings.shortcuts.next_editor_tab, cx),
                previous_editor_tab: read_shortcut_input(
                    &settings.shortcuts.previous_editor_tab,
                    cx,
                ),
                close_editor_tab: read_shortcut_input(&settings.shortcuts.close_editor_tab, cx),
                open_settings: read_shortcut_input(&settings.shortcuts.open_settings, cx),
                quit_app: read_shortcut_input(&settings.shortcuts.quit_app, cx),
                repo_tree_new_file: read_shortcut_input(
                    &settings.shortcuts.repo_tree_new_file,
                    cx,
                ),
                repo_tree_new_folder: read_shortcut_input(
                    &settings.shortcuts.repo_tree_new_folder,
                    cx,
                ),
                repo_tree_rename_file: read_shortcut_input(
                    &settings.shortcuts.repo_tree_rename_file,
                    cx,
                ),
            };

            if let Err(err) = validate_keyboard_shortcuts(&keyboard_shortcuts) {
                settings.error_message = Some(err);
                cx.notify();
                return;
            }

            let terminal = match settings_terminal_config(&settings.terminal, cx) {
                Ok(terminal) => terminal,
                Err(err) => {
                    settings.error_message = Some(err);
                    cx.notify();
                    return;
                }
            };

            settings.error_message = None;
            (
                settings.theme,
                settings.reduce_motion,
                settings.show_fps_counter,
                settings.auto_update_enabled,
                terminal,
                keyboard_shortcuts,
            )
        };

        let keyboard_shortcuts_changed = self.config.keyboard_shortcuts != keyboard_shortcuts;
        let terminal_changed = self.config.terminal != terminal;
        let auto_update_changed = self.config.auto_update_enabled != auto_update_enabled;
        let terminal_requires_restart = self.config.terminal.hydrate_app_environment_on_launch
            != terminal.hydrate_app_environment_on_launch;

        self.config.theme = theme;
        self.config.reduce_motion = reduce_motion;
        self.config.show_fps_counter = show_fps_counter;
        self.config.auto_update_enabled = auto_update_enabled;
        self.config.terminal = terminal;
        self.config.keyboard_shortcuts = keyboard_shortcuts;
        self.apply_theme_preference(window, cx);
        self.restart_auto_refresh(cx);
        self.restart_periodic_update_checks(cx);
        self.persist_config();
        if auto_update_enabled {
            self.maybe_schedule_startup_update_check(cx);
        }

        let saved_path = self
            .config_store
            .as_ref()
            .map(|store| store.path().display().to_string())
            .unwrap_or_else(|| "~/.hunkdiff/config.toml".to_string());
        let save_message = format!("Saved settings to {}.", saved_path);
        let follow_up =
            match (
                keyboard_shortcuts_changed,
                terminal_changed,
                terminal_requires_restart,
                auto_update_changed,
            ) {
                (true, true, true, _) => {
                    " Restart Hunk to reload keyboard shortcuts and startup terminal environment changes."
                }
                (true, true, false, _) => {
                    " Restart Hunk to reload keyboard shortcuts. Reopen the AI terminal to apply shell changes."
                }
                (true, false, _, _) => " Restart Hunk to reload keyboard shortcuts.",
                (false, true, true, _) => {
                    " Restart Hunk to apply startup terminal environment changes."
                }
                (false, true, false, _) => " Reopen the AI terminal to apply shell changes.",
                (false, false, _, true) => {
                    " Automatic update checks were updated."
                }
                (false, false, _, false) => "",
            };
        self.git_status_message = Some(format!("{save_message}{follow_up}"));
        gpui_component::WindowExt::push_notification(
            window,
            crate::app::notifications::success(save_message),
            cx,
        );

        cx.notify();
    }
}

#[cfg(test)]
mod settings_tests {
    use super::validate_shortcut_list;
    use crate::app::{
        SettingsTerminalShellChoice, settings_terminal_custom_program, settings_terminal_shell_choice,
        settings_terminal_shell_from_choice, terminal_shell_choice_program,
        terminal_shell_preserves_custom_arguments,
    };
    use hunk_domain::config::TerminalShell;

    #[test]
    fn validate_shortcut_list_accepts_key_sequences() {
        let shortcuts = vec!["g space".to_string(), "cmd-k left".to_string()];
        assert!(validate_shortcut_list("Test Action", &shortcuts).is_ok());
    }

    #[test]
    fn validate_shortcut_list_rejects_invalid_key_sequences() {
        let shortcuts = vec!["g not-a-key".to_string()];
        assert!(validate_shortcut_list("Test Action", &shortcuts).is_err());
    }

    #[test]
    fn terminal_shell_choice_maps_known_programs_to_presets() {
        assert_eq!(
            settings_terminal_shell_choice(&TerminalShell::Program("/bin/bash".to_string())),
            SettingsTerminalShellChoice::Bash
        );
        assert_eq!(
            settings_terminal_shell_choice(&TerminalShell::Program("pwsh.exe".to_string())),
            SettingsTerminalShellChoice::PowerShell
        );
        assert_eq!(
            settings_terminal_shell_choice(&TerminalShell::Program("cmd.exe".to_string())),
            SettingsTerminalShellChoice::CommandPrompt
        );
    }

    #[test]
    fn terminal_shell_choice_keeps_argument_variants_on_custom() {
        let shell = TerminalShell::WithArguments {
            program: "pwsh.exe".to_string(),
            args: vec!["-NoLogo".to_string()],
        };

        assert_eq!(
            settings_terminal_shell_choice(&shell),
            SettingsTerminalShellChoice::Custom
        );
        assert_eq!(settings_terminal_custom_program(&shell), "pwsh.exe");
        assert!(terminal_shell_preserves_custom_arguments(&shell));
    }

    #[test]
    fn terminal_shell_choice_program_returns_expected_presets() {
        assert_eq!(
            terminal_shell_choice_program(SettingsTerminalShellChoice::Bash),
            Some("/bin/bash")
        );
        assert_eq!(
            terminal_shell_choice_program(SettingsTerminalShellChoice::PowerShell),
            Some("pwsh.exe")
        );
        assert_eq!(
            terminal_shell_choice_program(SettingsTerminalShellChoice::System),
            None
        );
    }

    #[test]
    fn custom_shell_choice_preserves_with_arguments_when_program_is_unchanged() {
        let shell = TerminalShell::WithArguments {
            program: "pwsh.exe".to_string(),
            args: vec!["-NoLogo".to_string()],
        };

        assert_eq!(
            settings_terminal_shell_from_choice(
                SettingsTerminalShellChoice::Custom,
                "pwsh.exe",
                &shell,
            )
            .expect("custom terminal shell should build"),
            shell
        );
    }
}
