impl DiffViewer {
    fn render_browser_context_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu_state = self.browser_context_menu.as_ref()?.clone();
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();

        Some(
            deferred(
                anchored()
                    .position(menu_state.position)
                    .anchor(Anchor::TopLeft)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        v_flex()
                            .id("browser-context-menu")
                            .w(px(240.0))
                            .p_1()
                            .gap_0p5()
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(hunk_opacity(cx.theme().border, is_dark, 0.92, 0.74))
                            .bg(cx.theme().popover)
                            .shadow_none()
                            .on_mouse_down_out({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.close_browser_context_menu(cx);
                                    });
                                }
                            })
                            .children(self.render_browser_context_menu_entries(
                                view,
                                &menu_state.target,
                                cx,
                            )),
                    ),
            )
            .into_any_element(),
        )
    }

    fn render_browser_context_menu_entries(
        &self,
        view: Entity<Self>,
        target: &hunk_browser::BrowserContextMenuTarget,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let session_state = self
            .ai_selected_thread_id
            .as_deref()
            .and_then(|thread_id| self.ai_browser_runtime.session(thread_id))
            .map(|session| session.state().clone());
        let can_go_back = session_state
            .as_ref()
            .is_some_and(|state| state.can_go_back);
        let can_go_forward = session_state
            .as_ref()
            .is_some_and(|state| state.can_go_forward);
        let has_page_url = target.page_url.is_some()
            || session_state
                .as_ref()
                .and_then(|state| state.url.as_deref())
                .is_some_and(|url| !url.is_empty());
        let has_link_url = target.link_url.is_some();
        let has_source_url = target.source_url.is_some();
        let has_selection = target
            .selection_text
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty());

        let mut items = Vec::new();
        items.push(
            self.render_workspace_text_context_menu_item("Back", can_go_back, {
                let view = view.clone();
                move |cx| {
                    view.update(cx, |this, cx| {
                        this.browser_context_menu_navigate(hunk_browser::BrowserAction::Back, cx);
                    });
                }
            }, cx),
        );
        items.push(
            self.render_workspace_text_context_menu_item("Forward", can_go_forward, {
                let view = view.clone();
                move |cx| {
                    view.update(cx, |this, cx| {
                        this.browser_context_menu_navigate(hunk_browser::BrowserAction::Forward, cx);
                    });
                }
            }, cx),
        );
        items.push(
            self.render_workspace_text_context_menu_item("Reload", true, {
                let view = view.clone();
                move |cx| {
                    view.update(cx, |this, cx| {
                        this.browser_context_menu_navigate(hunk_browser::BrowserAction::Reload, cx);
                    });
                }
            }, cx),
        );
        items.push(div().h(px(1.0)).mx_1().bg(cx.theme().border).into_any_element());
        items.push(
            self.render_workspace_text_context_menu_item("Copy Page URL", has_page_url, {
                let view = view.clone();
                move |cx| {
                    view.update(cx, |this, cx| {
                        this.browser_context_menu_copy_page_url(cx);
                    });
                }
            }, cx),
        );
        if has_selection {
            items.push(
                self.render_workspace_text_context_menu_item("Copy Selected Text", true, {
                    let view = view.clone();
                    move |cx| {
                        view.update(cx, |this, cx| {
                            this.browser_context_menu_copy_selected_text(cx);
                        });
                    }
                }, cx),
            );
        }
        if has_link_url {
            items.push(
                self.render_workspace_text_context_menu_item("Open Link in New Tab", true, {
                    let view = view.clone();
                    move |cx| {
                        view.update(cx, |this, cx| {
                            this.browser_context_menu_open_link_in_new_tab(cx);
                        });
                    }
                }, cx),
            );
            items.push(
                self.render_workspace_text_context_menu_item("Copy Link Address", true, {
                    let view = view.clone();
                    move |cx| {
                        view.update(cx, |this, cx| {
                            this.browser_context_menu_copy_link_url(cx);
                        });
                    }
                }, cx),
            );
        }
        if has_source_url {
            items.push(
                self.render_workspace_text_context_menu_item("Open Media in New Tab", true, {
                    let view = view.clone();
                    move |cx| {
                        view.update(cx, |this, cx| {
                            this.browser_context_menu_open_source_in_new_tab(cx);
                        });
                    }
                }, cx),
            );
            items.push(
                self.render_workspace_text_context_menu_item("Copy Media Address", true, {
                    let view = view.clone();
                    move |cx| {
                        view.update(cx, |this, cx| {
                            this.browser_context_menu_copy_source_url(cx);
                        });
                    }
                }, cx),
            );
        }
        if target.editable {
            items.push(div().h(px(1.0)).mx_1().bg(cx.theme().border).into_any_element());
            items.push(
                self.render_workspace_text_context_menu_item("Cut", true, {
                    let view = view.clone();
                    move |cx| {
                        view.update(cx, |this, cx| {
                            this.browser_context_menu_edit_shortcut(BrowserEditShortcut::Cut, cx);
                        });
                    }
                }, cx),
            );
            items.push(
                self.render_workspace_text_context_menu_item("Copy", true, {
                    let view = view.clone();
                    move |cx| {
                        view.update(cx, |this, cx| {
                            this.browser_context_menu_edit_shortcut(BrowserEditShortcut::Copy, cx);
                        });
                    }
                }, cx),
            );
            items.push(
                self.render_workspace_text_context_menu_item("Paste", true, {
                    let view = view.clone();
                    move |cx| {
                        view.update(cx, |this, cx| {
                            this.browser_context_menu_edit_shortcut(BrowserEditShortcut::Paste, cx);
                        });
                    }
                }, cx),
            );
            items.push(
                self.render_workspace_text_context_menu_item("Select All", true, {
                    let view = view.clone();
                    move |cx| {
                        view.update(cx, |this, cx| {
                            this.browser_context_menu_edit_shortcut(
                                BrowserEditShortcut::SelectAll,
                                cx,
                            );
                        });
                    }
                }, cx),
            );
        }
        items.push(div().h(px(1.0)).mx_1().bg(cx.theme().border).into_any_element());
        items.push(
            self.render_workspace_text_context_menu_item("Inspect Element", true, {
                move |cx| {
                    view.update(cx, |this, cx| {
                        this.browser_context_menu_inspect_element(cx);
                    });
                }
            }, cx),
        );
        items
    }

    fn render_workspace_text_context_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu_state = self.workspace_text_context_menu.as_ref()?.clone();
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();

        Some(
            deferred(
                anchored()
                    .position(menu_state.position)
                    .anchor(Anchor::TopLeft)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        v_flex()
                            .id("workspace-text-context-menu")
                            .w(px(220.0))
                            .p_1()
                            .gap_0p5()
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(hunk_opacity(cx.theme().border, is_dark, 0.92, 0.74))
                            .bg(cx.theme().popover)
                            .shadow_none()
                            .on_mouse_down_out({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.close_workspace_text_context_menu(cx);
                                    });
                                }
                            })
                            .children(self.render_workspace_text_context_menu_entries(
                                view,
                                &menu_state.target,
                                cx,
                            )),
                    ),
            )
            .into_any_element(),
        )
    }

    fn render_workspace_text_context_menu_entries(
        &self,
        view: Entity<Self>,
        target: &WorkspaceTextContextMenuTarget,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut items = Vec::new();
        match target {
            WorkspaceTextContextMenuTarget::FilesEditor(target) => {
                items.push(
                    self.render_workspace_text_context_menu_item("Cut", target.can_cut, {
                        let view = view.clone();
                        move |cx| {
                            view.update(cx, |this, cx| {
                                this.workspace_text_context_menu_cut(cx);
                            });
                        }
                    }, cx),
                );
                items.push(
                    self.render_workspace_text_context_menu_item("Copy", target.can_copy, {
                        let view = view.clone();
                        move |cx| {
                            view.update(cx, |this, cx| {
                                this.workspace_text_context_menu_copy(cx);
                            });
                        }
                    }, cx),
                );
                items.push(
                    self.render_workspace_text_context_menu_item("Paste", target.can_paste, {
                        let view = view.clone();
                        move |cx| {
                            view.update(cx, |this, cx| {
                                this.workspace_text_context_menu_paste(cx);
                            });
                        }
                    }, cx),
                );
                items.push(div().h(px(1.0)).mx_1().bg(cx.theme().border).into_any_element());
                items.push(
                    self.render_workspace_text_context_menu_item(
                        "Select All",
                        target.can_select_all,
                        {
                            let view = view.clone();
                            move |cx| {
                                view.update(cx, |this, cx| {
                                    this.workspace_text_context_menu_select_all(cx);
                                });
                            }
                        },
                        cx,
                    ),
                );
            }
            WorkspaceTextContextMenuTarget::SelectableText(target) => {
                items.push(
                    self.render_workspace_text_context_menu_item("Copy", target.can_copy, {
                        let view = view.clone();
                        move |cx| {
                            view.update(cx, |this, cx| {
                                this.workspace_text_context_menu_copy(cx);
                            });
                        }
                    }, cx),
                );
                items.push(
                    self.render_workspace_text_context_menu_item(
                        "Select All",
                        target.can_select_all,
                        {
                            let view = view.clone();
                            move |cx| {
                                view.update(cx, |this, cx| {
                                    this.workspace_text_context_menu_select_all(cx);
                                });
                            }
                        },
                        cx,
                    ),
                );
                if target.link_target.is_some() {
                    items.push(div().h(px(1.0)).mx_1().bg(cx.theme().border).into_any_element());
                    items.push(
                        self.render_workspace_text_context_menu_item("Open Link", true, {
                            let view = view.clone();
                            move |cx| {
                                view.update(cx, |this, cx| {
                                    this.workspace_text_context_menu_open_link(cx);
                                });
                            }
                        }, cx),
                    );
                }
            }
            WorkspaceTextContextMenuTarget::Terminal(target) => {
                items.push(
                    self.render_workspace_text_context_menu_item("Copy", target.can_copy, {
                        let view = view.clone();
                        move |cx| {
                            view.update(cx, |this, cx| {
                                this.workspace_text_context_menu_copy(cx);
                            });
                        }
                    }, cx),
                );
                items.push(
                    self.render_workspace_text_context_menu_item("Paste", target.can_paste, {
                        let view = view.clone();
                        move |cx| {
                            view.update(cx, |this, cx| {
                                this.workspace_text_context_menu_paste(cx);
                            });
                        }
                    }, cx),
                );
                items.push(
                    self.render_workspace_text_context_menu_item(
                        "Select All",
                        target.can_select_all,
                        {
                            let view = view.clone();
                            move |cx| {
                                view.update(cx, |this, cx| {
                                    this.workspace_text_context_menu_select_all(cx);
                                });
                            }
                        },
                        cx,
                    ),
                );
                items.push(div().h(px(1.0)).mx_1().bg(cx.theme().border).into_any_element());
                items.push(
                    self.render_workspace_text_context_menu_item("Clear", target.can_clear, {
                        let view = view.clone();
                        move |cx| {
                            view.update(cx, |this, cx| {
                                this.workspace_text_context_menu_clear_terminal(cx);
                            });
                        }
                    }, cx),
                );
            }
            WorkspaceTextContextMenuTarget::DiffRows(target) => {
                items.push(
                    self.render_workspace_text_context_menu_item("Copy", target.can_copy, {
                        let view = view.clone();
                        move |cx| {
                            view.update(cx, |this, cx| {
                                this.workspace_text_context_menu_copy(cx);
                            });
                        }
                    }, cx),
                );
                items.push(
                    self.render_workspace_text_context_menu_item(
                        "Select All",
                        target.can_select_all,
                        move |cx| {
                            view.update(cx, |this, cx| {
                                this.workspace_text_context_menu_select_all(cx);
                            });
                        },
                        cx,
                    ),
                );
            }
        }
        items
    }

    fn render_workspace_text_context_menu_item(
        &self,
        label: &'static str,
        enabled: bool,
        on_click: impl Fn(&mut App) + 'static,
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
                this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    on_click(cx);
                })
                .hover(move |style| style.bg(hover_bg).cursor_pointer())
            })
            .child(div().min_w_0().truncate().child(label))
            .into_any_element()
    }
}
