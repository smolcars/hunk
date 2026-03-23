#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsCategory {
    Ui,
    KeyboardShortcuts,
}

impl SettingsCategory {
    const ALL: [Self; 2] = [Self::Ui, Self::KeyboardShortcuts];

    fn title(self) -> &'static str {
        match self {
            Self::Ui => "UI",
            Self::KeyboardShortcuts => "Keyboard Shortcuts",
        }
    }
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
    shortcuts: SettingsShortcutInputs,
    error_message: Option<String>,
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
