impl DiffViewer {
    fn render_diff_workspace_screen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .size_full()
            .child(if self.sidebar_collapsed {
                self.render_review_workspace_surface(window, cx)
                    .into_any_element()
            } else {
                h_resizable("hunk-diff-workspace")
                    .child(
                        resizable_panel()
                            .size(px(300.0))
                            .size_range(px(240.0)..px(520.0))
                            .child(self.render_tree(cx)),
                    )
                    .child(
                        resizable_panel().child(self.render_review_workspace_surface(window, cx)),
                    )
                    .into_any_element()
            })
            .into_any_element()
    }
}
