impl DiffViewer {
    fn render_tree(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let tree_key_context = if self.repo_tree_inline_edit.is_some() {
            "RepoTreeInlineEdit"
        } else {
            "RepoTree"
        };
        let tree_summary = if self.repo_tree.loading && !self.repo_tree.rows.is_empty() {
            format!(
                "{} files • {} folders • Refreshing...",
                self.repo_tree.file_count, self.repo_tree.folder_count
            )
        } else {
            format!(
                "{} files • {} folders",
                self.repo_tree.file_count, self.repo_tree.folder_count
            )
        };

        v_flex()
            .size_full()
            .relative()
            .key_context(tree_key_context)
            .track_focus(&self.repo_tree_focus_handle)
            .on_action(cx.listener(Self::repo_tree_new_file_action))
            .on_action(cx.listener(Self::repo_tree_new_folder_action))
            .on_action(cx.listener(Self::repo_tree_rename_file_action))
            .on_action(cx.listener(
                |this: &mut Self, _: &RepoTreeCancelInlineEdit, _: &mut Window, cx| {
                    this.cancel_repo_tree_inline_edit(cx);
                },
            ))
            .on_mouse_down(MouseButton::Left, {
                let view = view.clone();
                move |_, window, cx| {
                    view.update(cx, |this, cx| {
                        this.repo_tree_focus_handle.focus(window, cx);
                        this.close_repo_tree_context_menu(cx);
                    });
                }
            })
            .on_mouse_down(MouseButton::Right, {
                let view = view.clone();
                move |_, window, cx| {
                    view.update(cx, |this, cx| {
                        this.repo_tree_focus_handle.focus(window, cx);
                        this.close_repo_tree_context_menu(cx);
                    });
                }
            })
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().sidebar.blend(cx.theme().muted.opacity(if is_dark {
                        0.18
                    } else {
                        0.30
                    })))
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().muted_foreground)
                            .child(tree_summary),
                    ),
            )
            .child(div().flex_1().min_h_0().child(self.render_repo_tree_content(cx)))
            .when_some(self.render_repo_tree_context_menu(cx), |this, menu| {
                this.child(menu)
            })
    }

    fn render_repo_tree_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.repo_tree.loading && self.repo_tree.rows.is_empty() {
            return v_flex()
                .w_full()
                .px_2()
                .py_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading repository tree..."),
                )
                .into_any_element();
        }

        if let Some(error) = self.repo_tree.error.as_ref() {
            return v_flex()
                .w_full()
                .px_2()
                .py_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .whitespace_normal()
                        .child(error.clone()),
                )
                .into_any_element();
        }

        if self.repo_tree.rows.is_empty() {
            return v_flex()
                .w_full()
                .px_2()
                .py_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No files found."),
                )
                .into_any_element();
        }

        self.sync_sidebar_repo_list_state(self.repo_tree.rows.len());
        let list_state = self.repo_tree.list_state.clone();
        let view = cx.entity();

        let list = list(list_state.clone(), {
            cx.processor(move |this, ix: usize, _window, cx| {
                this.repo_tree.rows
                    .get(ix)
                    .map(|row| this.render_repo_tree_row(row, cx))
                    .unwrap_or_else(|| div().into_any_element())
            })
        })
        .size_full()
        .map(|mut list| {
            list.style().restrict_scroll_to_axis = Some(true);
            list
        })
        .with_sizing_behavior(ListSizingBehavior::Auto);

        v_flex()
            .size_full()
            .when_some(self.render_repo_tree_inline_new_entry_row(cx), |this, inline_row| {
                this.child(inline_row)
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .px_1()
                    .py_1()
                    .on_mouse_down(MouseButton::Right, {
                        let view = view.clone();
                        move |event, window, cx| {
                            cx.stop_propagation();
                            view.update(cx, |this, cx| {
                                this.repo_tree_focus_handle.focus(window, cx);
                                this.open_repo_tree_context_menu(
                                    None,
                                    RepoTreeNodeKind::Directory,
                                    event.position,
                                    cx,
                                );
                            });
                        }
                    })
                    .child(list),
            )
            .into_any_element()
    }

    fn render_repo_tree_inline_new_entry_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (base_dir, is_folder, input_state) = self.inline_repo_tree_new_entry()?;
        let is_dark = cx.theme().mode.is_dark();
        let depth = base_dir
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(|value| value.split('/').count())
            .unwrap_or(0);
        let icon = if is_folder {
            IconName::FolderClosed
        } else {
            IconName::File
        };

        Some(
            h_flex()
                .w_full()
                .h(px(SIDEBAR_REPO_LIST_ESTIMATED_ROW_HEIGHT))
                .px_1()
                .items_center()
                .gap_1()
                .rounded_sm()
                .bg(cx.theme().accent.opacity(if is_dark { 0.14 } else { 0.08 }))
                .child(div().w(px(depth as f32 * 14.0)))
                .child(div().w(px(14.0)))
                .child(
                    div().w(px(18.0)).child(
                        Icon::new(icon)
                            .size(px(14.0))
                            .text_color(cx.theme().muted_foreground),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_mouse_down(MouseButton::Right, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .child(
                            Input::new(&input_state)
                                .h(px(20.0))
                                .rounded(px(4.0))
                                .border_1()
                                .border_color(cx.theme().border.opacity(if is_dark { 0.90 } else { 0.72 }))
                                .bg(cx.theme().background),
                        ),
                )
                .when_some(
                    base_dir
                        .as_ref()
                        .filter(|value| !value.is_empty())
                        .map(std::string::String::as_str),
                    |this, dir| {
                        this.child(
                            div()
                                .pr_1()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("in {dir}")),
                        )
                    },
                )
                .into_any_element(),
        )
    }

    fn render_repo_tree_context_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu_state = self.repo_tree_context_menu.as_ref()?.clone();
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let allow_manage = self.workspace_view_mode == WorkspaceViewMode::Files && self.repo_root.is_some();
        let allow_target_path = menu_state.target_path.is_some();
        let allow_rename =
            allow_manage && allow_target_path && menu_state.target_kind == RepoTreeNodeKind::File;
        let allow_delete = allow_rename;
        let allow_copy = allow_target_path;
        let allow_collapse = !self.repo_tree.expanded_dirs.is_empty();

        Some(
            deferred(
                anchored()
                    .position(menu_state.position)
                    .anchor(Corner::TopLeft)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        v_flex()
                            .id("repo-tree-context-menu")
                            .w(px(234.0))
                            .p_1()
                            .gap_0p5()
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(cx.theme().border.opacity(if is_dark { 0.92 } else { 0.74 }))
                            .bg(cx.theme().popover)
                            .shadow_none()
                            .on_mouse_down_out({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.close_repo_tree_context_menu(cx);
                                    });
                                }
                            })
                            .child(
                                self.render_repo_tree_context_menu_item(
                                    "New File",
                                    self.repo_tree_shortcut_label(RepoTreeMenuShortcut::NewFile),
                                    allow_manage,
                                    {
                                        let view = view.clone();
                                        let target_path = menu_state.target_path.clone();
                                        let target_kind = menu_state.target_kind;
                                        move |window, cx| {
                                            view.update(cx, |this, cx| {
                                                this.close_repo_tree_context_menu(cx);
                                                this.open_repo_tree_new_file_prompt_at(
                                                    target_path
                                                        .as_ref()
                                                        .map(|path| (path.clone(), target_kind)),
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }
                                    },
                                    cx,
                                ),
                            )
                            .child(
                                self.render_repo_tree_context_menu_item(
                                    "New Folder",
                                    self.repo_tree_shortcut_label(RepoTreeMenuShortcut::NewFolder),
                                    allow_manage,
                                    {
                                        let view = view.clone();
                                        let target_path = menu_state.target_path.clone();
                                        let target_kind = menu_state.target_kind;
                                        move |window, cx| {
                                            view.update(cx, |this, cx| {
                                                this.close_repo_tree_context_menu(cx);
                                                this.open_repo_tree_new_folder_prompt_at(
                                                    target_path
                                                        .as_ref()
                                                        .map(|path| (path.clone(), target_kind)),
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }
                                    },
                                    cx,
                                ),
                            )
                            .child(div().h(px(1.0)).mx_1().bg(cx.theme().border))
                            .child(
                                self.render_repo_tree_context_menu_item(
                                    "Rename File",
                                    self.repo_tree_shortcut_label(RepoTreeMenuShortcut::RenameFile),
                                    allow_rename,
                                    {
                                        let view = view.clone();
                                        let target_path = menu_state.target_path.clone();
                                        move |window, cx| {
                                            view.update(cx, |this, cx| {
                                                this.close_repo_tree_context_menu(cx);
                                                if let Some(path) = target_path.as_ref() {
                                                    this.open_repo_tree_rename_prompt_for_file(
                                                        path.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            });
                                        }
                                    },
                                    cx,
                                ),
                            )
                            .child(
                                self.render_repo_tree_context_menu_item(
                                    "Delete File",
                                    None,
                                    allow_delete,
                                    {
                                        let view = view.clone();
                                        let target_path = menu_state.target_path.clone();
                                        move |_, cx| {
                                            view.update(cx, |this, cx| {
                                                this.close_repo_tree_context_menu(cx);
                                                if let Some(path) = target_path.as_ref() {
                                                    this.delete_repo_tree_file_at(path.as_str(), cx);
                                                }
                                            });
                                        }
                                    },
                                    cx,
                                ),
                            )
                            .child(div().h(px(1.0)).mx_1().bg(cx.theme().border))
                            .child(
                                self.render_repo_tree_context_menu_item(
                                    "Copy Path",
                                    None,
                                    allow_copy,
                                    {
                                        let view = view.clone();
                                        let target_path = menu_state.target_path.clone();
                                        move |_, cx| {
                                            view.update(cx, |this, cx| {
                                                this.close_repo_tree_context_menu(cx);
                                                if let Some(path) = target_path.as_ref() {
                                                    this.copy_repo_tree_absolute_path(path.as_str(), cx);
                                                }
                                            });
                                        }
                                    },
                                    cx,
                                ),
                            )
                            .child(
                                self.render_repo_tree_context_menu_item(
                                    "Copy Relative Path",
                                    None,
                                    allow_copy,
                                    {
                                        let view = view.clone();
                                        let target_path = menu_state.target_path.clone();
                                        move |_, cx| {
                                            view.update(cx, |this, cx| {
                                                this.close_repo_tree_context_menu(cx);
                                                if let Some(path) = target_path.as_ref() {
                                                    this.copy_repo_tree_relative_path(path.as_str(), cx);
                                                }
                                            });
                                        }
                                    },
                                    cx,
                                ),
                            )
                            .child(div().h(px(1.0)).mx_1().bg(cx.theme().border))
                            .child(
                                self.render_repo_tree_context_menu_item(
                                    "Collapse All Folders",
                                    None,
                                    allow_collapse,
                                    {
                                        let view = view.clone();
                                        move |_, cx| {
                                            view.update(cx, |this, cx| {
                                                this.close_repo_tree_context_menu(cx);
                                                this.collapse_all_repo_tree_directories(cx);
                                            });
                                        }
                                    },
                                    cx,
                                ),
                            ),
                    ),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }

    fn render_repo_tree_context_menu_item(
        &self,
        label: &'static str,
        shortcut: Option<String>,
        enabled: bool,
        on_click: impl Fn(&mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text_color = if enabled {
            cx.theme().popover_foreground
        } else {
            cx.theme().muted_foreground
        };
        let hover_bg = cx.theme().secondary_hover;
        div()
            .w_full()
            .px_2()
            .py_1()
            .rounded(px(6.0))
            .text_sm()
            .text_color(text_color)
            .when(enabled, |this| {
                this.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    on_click(window, cx);
                })
                .hover(move |style| style.bg(hover_bg).cursor_pointer())
            })
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().min_w_0().truncate().child(label))
                    .when_some(shortcut, |this, value| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(value),
                        )
                    }),
            )
            .into_any_element()
    }

    fn sync_sidebar_repo_list_state(&mut self, row_count: usize) {
        if self.repo_tree.row_count == row_count
            && self.repo_tree.scroll_anchor_path.is_none()
        {
            return;
        }
        self.repo_tree.row_count = row_count;
        let anchor_path = self.repo_tree.scroll_anchor_path.take();
        Self::sync_sidebar_list_state(
            &self.repo_tree.list_state,
            &self.repo_tree.rows,
            anchor_path.as_deref(),
        );
    }

    fn sync_sidebar_list_state(
        list_state: &ListState,
        rows: &[super::data::RepoTreeRow],
        anchor_path: Option<&str>,
    ) {
        let row_count = rows.len();
        let previous_top = list_state.logical_scroll_top();
        list_state.reset(row_count);
        let fallback_item_ix = if row_count == 0 {
            0
        } else {
            previous_top.item_ix.min(row_count.saturating_sub(1))
        };
        let item_ix = if let Some(path) = anchor_path {
            rows.iter()
                .position(|row| row.path == path)
                .unwrap_or(fallback_item_ix)
        } else {
            fallback_item_ix
        };
        let offset_in_item = if row_count == 0 || item_ix != previous_top.item_ix {
            px(0.)
        } else {
            previous_top.offset_in_item
        };
        list_state.scroll_to(ListOffset {
            item_ix,
            offset_in_item,
        });
    }

    pub(crate) fn capture_sidebar_repo_scroll_anchor(&mut self) {
        let top_row_ix = self.repo_tree.list_state.logical_scroll_top().item_ix;
        self.repo_tree.scroll_anchor_path = self.repo_tree.rows.get(top_row_ix).map(|row| row.path.clone());
    }

    fn render_repo_tree_row(
        &self,
        row: &super::data::RepoTreeRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let is_selected =
            row.kind == RepoTreeNodeKind::File && self.selected_path.as_deref() == Some(row.path.as_str());
        let row_bg = if is_selected {
            cx.theme().accent.opacity(if is_dark { 0.30 } else { 0.14 })
        } else if row.ignored {
            cx.theme().muted.opacity(if is_dark { 0.16 } else { 0.22 })
        } else {
            cx.theme().background.opacity(0.0)
        };
        let text_color = if row.ignored {
            cx.theme().muted_foreground.opacity(if is_dark { 0.88 } else { 0.95 })
        } else {
            cx.theme().foreground
        };
        let icon_color = cx.theme().muted_foreground;
        let chevron_icon = if row.kind == RepoTreeNodeKind::Directory {
            Some(if row.expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
        } else {
            None
        };
        let icon = match row.kind {
            RepoTreeNodeKind::Directory => {
                if row.expanded {
                    IconName::FolderOpen
                } else {
                    IconName::FolderClosed
                }
            }
            RepoTreeNodeKind::File => file_icon_for_path(row.path.as_str()),
        };
        let row_id = stable_row_id_for_path(row.path.as_str());
        let file_status = row.file_status;
        let rename_input = self.inline_repo_tree_rename_input_for_path(row.path.as_str());
        let rename_active = rename_input.is_some();
        let row_hover_bg = if is_selected {
            cx.theme().secondary_active
        } else {
            cx.theme().secondary_hover
        };
        let path_for_click = row.path.clone();
        let kind_for_click = row.kind;
        let menu_target_path = row.path.clone();
        let menu_target_kind = row.kind;

        h_flex()
            .id(("repo-tree-row", row_id))
            .w_full()
            .h(px(SIDEBAR_REPO_LIST_ESTIMATED_ROW_HEIGHT))
            .items_center()
            .gap_1()
            .px_1()
            .rounded_sm()
            .bg(row_bg)
            .child(div().w(px(row.depth as f32 * 14.0)))
            .child(div().w(px(14.0)).when_some(chevron_icon, |this, icon_name| {
                this.child(
                    Icon::new(icon_name)
                        .size(px(12.0))
                        .text_color(cx.theme().muted_foreground),
                )
            }))
            .child(
                div().w(px(18.0)).child(
                    Icon::new(icon)
                        .size(px(14.0))
                        .text_color(icon_color),
                ),
            )
            .when(!rename_active, |this| {
                this.when_some(file_status, |this, status| {
                    let (status_label, status_color) = change_status_label_color(status, cx);
                    this.child(
                        div()
                            .px_1()
                            .py_0p5()
                            .rounded(px(4.0))
                            .text_xs()
                            .font_semibold()
                            .bg(status_color.opacity(if is_dark { 0.24 } else { 0.16 }))
                            .text_color(cx.theme().foreground)
                            .child(status_label),
                    )
                })
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .when_some(rename_input.clone(), |this, input_state| {
                        this.on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_mouse_down(MouseButton::Right, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .child(
                            Input::new(&input_state)
                                .h(px(20.0))
                                .rounded(px(4.0))
                                .border_1()
                                .border_color(cx.theme().accent.opacity(if is_dark { 0.74 } else { 0.58 }))
                                .bg(cx.theme().background),
                        )
                    })
                    .when(rename_input.is_none(), |this| {
                        this.text_xs()
                            .truncate()
                            .text_color(text_color)
                            .child(row.name.clone())
                    }),
            )
            .on_mouse_down(MouseButton::Right, {
                let view = view.clone();
                move |event, window, cx| {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| {
                        this.repo_tree_focus_handle.focus(window, cx);
                        this.open_repo_tree_context_menu(
                            Some(menu_target_path.clone()),
                            menu_target_kind,
                            event.position,
                            cx,
                        );
                    });
                }
            })
            .when(!rename_active, |this| {
                this.hover(move |style| style.bg(row_hover_bg).cursor_pointer())
                    .on_click({
                    let view = view.clone();
                    move |_, window, cx| {
                        view.update(cx, |this, cx| match kind_for_click {
                            RepoTreeNodeKind::Directory => {
                                this.toggle_repo_tree_directory(path_for_click.clone(), cx);
                                this.repo_tree_focus_handle.focus(window, cx);
                            }
                            RepoTreeNodeKind::File => {
                                this.select_repo_tree_file(path_for_click.clone(), cx);
                                this.repo_tree_focus_handle.focus(window, cx);
                            }
                        });
                    }
                })
            })
            .into_any_element()
    }
}

fn stable_row_id_for_path(path: &str) -> u64 {
    use std::hash::{Hash as _, Hasher as _};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Copy)]
enum RepoTreeMenuShortcut {
    NewFile,
    NewFolder,
    RenameFile,
}

impl DiffViewer {
    fn repo_tree_shortcut_label(&self, shortcut: RepoTreeMenuShortcut) -> Option<String> {
        let shortcuts = &self.config.keyboard_shortcuts;
        let values = match shortcut {
            RepoTreeMenuShortcut::NewFile => shortcuts.repo_tree_new_file.as_slice(),
            RepoTreeMenuShortcut::NewFolder => shortcuts.repo_tree_new_folder.as_slice(),
            RepoTreeMenuShortcut::RenameFile => shortcuts.repo_tree_rename_file.as_slice(),
        };
        let preferred = if cfg!(target_os = "macos") {
            values
                .iter()
                .find(|shortcut| shortcut.to_ascii_lowercase().contains("cmd"))
        } else {
            values
                .iter()
                .find(|shortcut| shortcut.to_ascii_lowercase().contains("ctrl"))
        }
        .or_else(|| values.first())?;
        Some(format_shortcut_label(preferred.as_str()))
    }
}

fn format_shortcut_label(shortcut: &str) -> String {
    shortcut
        .split('-')
        .map(|part| match part.to_ascii_lowercase().as_str() {
            "cmd" => "Cmd".to_string(),
            "ctrl" => "Ctrl".to_string(),
            "alt" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            "super" => "Super".to_string(),
            "secondary" => "Secondary".to_string(),
            _ => part.to_ascii_uppercase(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn path_extension(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

fn file_icon_for_path(path: &str) -> IconName {
    match path_extension(path).as_deref() {
        Some("toml") | Some("yaml") | Some("yml") | Some("json") | Some("lock") => {
            IconName::Settings
        }
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("svg") => {
            IconName::GalleryVerticalEnd
        }
        Some("md") => IconName::BookOpen,
        _ => IconName::File,
    }
}
