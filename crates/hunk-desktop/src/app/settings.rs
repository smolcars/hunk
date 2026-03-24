#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsCategory {
    Ui,
    Terminal,
    KeyboardShortcuts,
}

impl SettingsCategory {
    const ALL: [Self; 3] = [Self::Ui, Self::Terminal, Self::KeyboardShortcuts];

    fn title(self) -> &'static str {
        match self {
            Self::Ui => "UI",
            Self::Terminal => "Terminal",
            Self::KeyboardShortcuts => "Keyboard Shortcuts",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTerminalShellChoice {
    System,
    Bash,
    Zsh,
    Fish,
    PowerShell,
    WindowsPowerShell,
    CommandPrompt,
    Custom,
}

impl SettingsTerminalShellChoice {
    fn title(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Bash => "Bash",
            Self::Zsh => "Zsh",
            Self::Fish => "Fish",
            Self::PowerShell => "PowerShell",
            Self::WindowsPowerShell => "Windows PowerShell",
            Self::CommandPrompt => "Command Prompt",
            Self::Custom => "Custom",
        }
    }

    fn choices_for_current_platform() -> &'static [Self] {
        if cfg!(target_os = "windows") {
            &[
                Self::System,
                Self::PowerShell,
                Self::WindowsPowerShell,
                Self::CommandPrompt,
                Self::Custom,
            ]
        } else {
            &[Self::System, Self::Bash, Self::Zsh, Self::Fish, Self::Custom]
        }
    }
}

#[derive(Clone)]
struct SettingsTerminalState {
    shell_choice: SettingsTerminalShellChoice,
    custom_program: Entity<InputState>,
    original_shell: TerminalShell,
    inherit_login_environment: bool,
    hydrate_app_environment_on_launch: bool,
}

#[derive(Clone)]
struct SettingsShortcutRow {
    id: &'static str,
    label: &'static str,
    hint: &'static str,
    input_state: Entity<InputState>,
}

#[derive(Clone)]
struct SettingsShortcutInputs {
    select_next_line: Entity<InputState>,
    select_previous_line: Entity<InputState>,
    extend_selection_next_line: Entity<InputState>,
    extend_selection_previous_line: Entity<InputState>,
    copy_selection: Entity<InputState>,
    select_all_diff_rows: Entity<InputState>,
    next_hunk: Entity<InputState>,
    previous_hunk: Entity<InputState>,
    next_file: Entity<InputState>,
    previous_file: Entity<InputState>,
    view_current_review_file: Entity<InputState>,
    toggle_sidebar_tree: Entity<InputState>,
    switch_to_files_view: Entity<InputState>,
    switch_to_review_view: Entity<InputState>,
    switch_to_git_view: Entity<InputState>,
    toggle_ai_terminal_drawer: Entity<InputState>,
    open_project: Entity<InputState>,
    save_current_file: Entity<InputState>,
    open_settings: Entity<InputState>,
    quit_app: Entity<InputState>,
    repo_tree_new_file: Entity<InputState>,
    repo_tree_new_folder: Entity<InputState>,
    repo_tree_rename_file: Entity<InputState>,
}

impl SettingsShortcutInputs {
    fn rows(&self) -> Vec<SettingsShortcutRow> {
        vec![
            SettingsShortcutRow {
                id: "select-next-line",
                label: "Select Next Line",
                hint: "Moves selection down one diff row.",
                input_state: self.select_next_line.clone(),
            },
            SettingsShortcutRow {
                id: "select-previous-line",
                label: "Select Previous Line",
                hint: "Moves selection up one diff row.",
                input_state: self.select_previous_line.clone(),
            },
            SettingsShortcutRow {
                id: "extend-selection-next-line",
                label: "Extend Selection Down",
                hint: "Extends the multi-row selection downward.",
                input_state: self.extend_selection_next_line.clone(),
            },
            SettingsShortcutRow {
                id: "extend-selection-previous-line",
                label: "Extend Selection Up",
                hint: "Extends the multi-row selection upward.",
                input_state: self.extend_selection_previous_line.clone(),
            },
            SettingsShortcutRow {
                id: "copy-selection",
                label: "Copy Selection",
                hint: "Copies the selected diff rows.",
                input_state: self.copy_selection.clone(),
            },
            SettingsShortcutRow {
                id: "select-all-diff-rows",
                label: "Select All Diff Rows",
                hint: "Selects all rows in the current diff.",
                input_state: self.select_all_diff_rows.clone(),
            },
            SettingsShortcutRow {
                id: "next-hunk",
                label: "Next Hunk",
                hint: "Jumps to the next diff hunk.",
                input_state: self.next_hunk.clone(),
            },
            SettingsShortcutRow {
                id: "previous-hunk",
                label: "Previous Hunk",
                hint: "Jumps to the previous diff hunk.",
                input_state: self.previous_hunk.clone(),
            },
            SettingsShortcutRow {
                id: "next-file",
                label: "Next File",
                hint: "Moves to the next changed file.",
                input_state: self.next_file.clone(),
            },
            SettingsShortcutRow {
                id: "previous-file",
                label: "Previous File",
                hint: "Moves to the previous changed file.",
                input_state: self.previous_file.clone(),
            },
            SettingsShortcutRow {
                id: "view-current-review-file",
                label: "View Review File",
                hint: "Opens the selected review file in Files view.",
                input_state: self.view_current_review_file.clone(),
            },
            SettingsShortcutRow {
                id: "toggle-sidebar-tree",
                label: "Toggle File Tree",
                hint: "Collapses or expands the left file tree pane.",
                input_state: self.toggle_sidebar_tree.clone(),
            },
            SettingsShortcutRow {
                id: "switch-to-files-view",
                label: "Switch to Files View",
                hint: "Switches the workspace to file editing view.",
                input_state: self.switch_to_files_view.clone(),
            },
            SettingsShortcutRow {
                id: "switch-to-review-view",
                label: "Switch to Review View",
                hint: "Switches the workspace to side-by-side diff review.",
                input_state: self.switch_to_review_view.clone(),
            },
            SettingsShortcutRow {
                id: "switch-to-git-view",
                label: "Switch to Git View",
                hint: "Switches the workspace to the Git workflow view.",
                input_state: self.switch_to_git_view.clone(),
            },
            SettingsShortcutRow {
                id: "toggle-ai-terminal-drawer",
                label: "Toggle AI Terminal",
                hint: "Opens or closes the AI bottom terminal panel.",
                input_state: self.toggle_ai_terminal_drawer.clone(),
            },
            SettingsShortcutRow {
                id: "open-project",
                label: "Open Project",
                hint: "Opens the system project picker.",
                input_state: self.open_project.clone(),
            },
            SettingsShortcutRow {
                id: "save-current-file",
                label: "Save Current File",
                hint: "Saves the active file editor buffer.",
                input_state: self.save_current_file.clone(),
            },
            SettingsShortcutRow {
                id: "open-settings",
                label: "Open Settings",
                hint: "Opens this settings popup.",
                input_state: self.open_settings.clone(),
            },
            SettingsShortcutRow {
                id: "quit-app",
                label: "Quit App",
                hint: "Quits Hunk.",
                input_state: self.quit_app.clone(),
            },
            SettingsShortcutRow {
                id: "repo-tree-new-file",
                label: "Tree: New File",
                hint: "Creates a file from the focused file tree.",
                input_state: self.repo_tree_new_file.clone(),
            },
            SettingsShortcutRow {
                id: "repo-tree-new-folder",
                label: "Tree: New Folder",
                hint: "Creates a folder from the focused file tree.",
                input_state: self.repo_tree_new_folder.clone(),
            },
            SettingsShortcutRow {
                id: "repo-tree-rename-file",
                label: "Tree: Rename File",
                hint: "Renames the selected file in the focused file tree.",
                input_state: self.repo_tree_rename_file.clone(),
            },
        ]
    }
}

#[derive(Clone)]
struct SettingsDraft {
    category: SettingsCategory,
    theme: ThemePreference,
    reduce_motion: bool,
    show_fps_counter: bool,
    terminal: SettingsTerminalState,
    shortcuts: SettingsShortcutInputs,
    error_message: Option<String>,
}

fn settings_terminal_input(
    value: &str,
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut Context<DiffViewer>,
) -> Entity<InputState> {
    let value = value.to_string();
    cx.new(|cx| {
        let mut state = InputState::new(window, cx).placeholder(placeholder);
        state.set_value(value.clone(), window, cx);
        state
    })
}

fn settings_terminal_shell_choice(shell: &TerminalShell) -> SettingsTerminalShellChoice {
    match shell {
        TerminalShell::System => SettingsTerminalShellChoice::System,
        TerminalShell::WithArguments { .. } => SettingsTerminalShellChoice::Custom,
        TerminalShell::Program(program) => match std::path::Path::new(program)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(program.as_str())
            .to_ascii_lowercase()
            .as_str()
        {
            "bash" => SettingsTerminalShellChoice::Bash,
            "zsh" => SettingsTerminalShellChoice::Zsh,
            "fish" => SettingsTerminalShellChoice::Fish,
            "pwsh" | "pwsh.exe" => SettingsTerminalShellChoice::PowerShell,
            "powershell" | "powershell.exe" => SettingsTerminalShellChoice::WindowsPowerShell,
            "cmd" | "cmd.exe" => SettingsTerminalShellChoice::CommandPrompt,
            _ => SettingsTerminalShellChoice::Custom,
        },
    }
}

fn settings_terminal_custom_program(shell: &TerminalShell) -> String {
    match shell {
        TerminalShell::System => String::new(),
        TerminalShell::Program(program) => program.clone(),
        TerminalShell::WithArguments { program, .. } => program.clone(),
    }
}

fn terminal_shell_choice_program(choice: SettingsTerminalShellChoice) -> Option<&'static str> {
    match choice {
        SettingsTerminalShellChoice::System => None,
        SettingsTerminalShellChoice::Bash => Some("/bin/bash"),
        SettingsTerminalShellChoice::Zsh => Some("/bin/zsh"),
        SettingsTerminalShellChoice::Fish => Some("fish"),
        SettingsTerminalShellChoice::PowerShell => Some("pwsh.exe"),
        SettingsTerminalShellChoice::WindowsPowerShell => Some("powershell.exe"),
        SettingsTerminalShellChoice::CommandPrompt => Some("cmd.exe"),
        SettingsTerminalShellChoice::Custom => None,
    }
}

fn terminal_custom_placeholder() -> &'static str {
    if cfg!(target_os = "windows") {
        r"Program path or command, e.g. C:\Program Files\PowerShell\7\pwsh.exe"
    } else {
        "Program path or command, e.g. /opt/homebrew/bin/fish"
    }
}

fn settings_terminal_config(
    state: &SettingsTerminalState,
    cx: &Context<DiffViewer>,
) -> Result<TerminalConfig, String> {
    let shell = settings_terminal_shell_from_choice(
        state.shell_choice,
        state.custom_program.read(cx).value().trim(),
        &state.original_shell,
    )?;

    Ok(TerminalConfig {
        shell,
        inherit_login_environment: state.inherit_login_environment,
        hydrate_app_environment_on_launch: state.hydrate_app_environment_on_launch,
    })
}

fn settings_terminal_shell_from_choice(
    choice: SettingsTerminalShellChoice,
    custom_program: &str,
    original_shell: &TerminalShell,
) -> Result<TerminalShell, String> {
    match choice {
        SettingsTerminalShellChoice::System => Ok(TerminalShell::System),
        SettingsTerminalShellChoice::Custom => {
            let program = custom_program.trim();
            if program.is_empty() {
                return Err("Terminal shell: custom program cannot be empty".to_string());
            }

            match original_shell {
                TerminalShell::WithArguments {
                    program: original_program,
                    ..
                } if original_program == program => Ok(original_shell.clone()),
                _ => Ok(TerminalShell::Program(program.to_string())),
            }
        }
        choice => Ok(TerminalShell::Program(
            terminal_shell_choice_program(choice)
                .expect("preset shell choice should resolve to a program")
                .to_string(),
        )),
    }
}

fn terminal_shell_preserves_custom_arguments(shell: &TerminalShell) -> bool {
    matches!(shell, TerminalShell::WithArguments { .. })
}

fn shortcut_lines(values: &[String]) -> String {
    values.join(", ")
}

fn parse_shortcut_lines(value: &str) -> Vec<String> {
    let mut shortcuts = Vec::new();
    let mut token = String::new();
    let mut previous_non_whitespace = None;

    for character in value.chars() {
        let is_separator =
            character == '\n' || (character == ',' && previous_non_whitespace != Some('-'));
        if is_separator {
            let trimmed = token.trim();
            if !trimmed.is_empty() {
                shortcuts.push(trimmed.to_owned());
            }
            token.clear();
            previous_non_whitespace = Some(character);
            continue;
        }

        token.push(character);
        if !character.is_whitespace() {
            previous_non_whitespace = Some(character);
        }
    }

    let trimmed = token.trim();
    if !trimmed.is_empty() {
        shortcuts.push(trimmed.to_owned());
    }

    shortcuts
}
