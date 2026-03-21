#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiTerminalCursorOverlayKind {
    Beam,
    Outline,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AiTerminalCursorOverlay {
    column: usize,
    color: gpui::Hsla,
    kind: AiTerminalCursorOverlayKind,
}

#[derive(Debug, Clone, Copy)]
struct AiTerminalCursorRenderContext {
    cursor_shape: TerminalCursorShapeSnapshot,
    surface_focused: bool,
    cursor_visible: bool,
    default_foreground: gpui::Hsla,
    default_background: gpui::Hsla,
    is_dark: bool,
}

fn ai_terminal_cell_style(
    cell: &AiTerminalRenderCell,
    column: usize,
    link_active: bool,
    render: AiTerminalCursorRenderContext,
    cx: &App,
) -> (AiTerminalCellStyle, Option<AiTerminalCursorOverlay>) {
    let link_color = cx.theme().primary;
    let mut style = AiTerminalCellStyle {
        color: ai_terminal_snapshot_color(cell.fg, render.is_dark, cx),
        background: ai_terminal_snapshot_color(cell.bg, render.is_dark, cx),
        underline: None,
    };
    let mut overlay = None;

    if link_active {
        if style.color == render.default_foreground {
            style.color = link_color;
        }
        style.underline = Some(gpui::UnderlineStyle {
            thickness: px(1.0),
            color: Some(link_color),
            wavy: false,
        });
    }

    if cell.cursor && render.cursor_visible {
        let cursor_color = ai_terminal_snapshot_color(
            TerminalColorSnapshot::Named(TerminalNamedColorSnapshot::Cursor),
            render.is_dark,
            cx,
        );

        match render.cursor_shape {
            TerminalCursorShapeSnapshot::Hidden => {}
            TerminalCursorShapeSnapshot::Underline => {
                style.underline = Some(gpui::UnderlineStyle {
                    thickness: px(1.5),
                    color: Some(cursor_color),
                    wavy: false,
                });
            }
            TerminalCursorShapeSnapshot::Beam => {
                overlay = Some(AiTerminalCursorOverlay {
                    column,
                    color: cursor_color,
                    kind: if render.surface_focused {
                        AiTerminalCursorOverlayKind::Beam
                    } else {
                        AiTerminalCursorOverlayKind::Outline
                    },
                });
            }
            TerminalCursorShapeSnapshot::Block => {
                if render.surface_focused {
                    style.color = render.default_background;
                    style.background = cursor_color;
                } else {
                    overlay = Some(AiTerminalCursorOverlay {
                        column,
                        color: hunk_opacity(cursor_color, render.is_dark, 0.84, 0.72),
                        kind: AiTerminalCursorOverlayKind::Outline,
                    });
                }
            }
            TerminalCursorShapeSnapshot::HollowBlock => {
                overlay = Some(AiTerminalCursorOverlay {
                    column,
                    color: hunk_opacity(cursor_color, render.is_dark, 0.84, 0.72),
                    kind: AiTerminalCursorOverlayKind::Outline,
                });
            }
        }
    }

    if style.color == render.default_foreground
        && style.background == render.default_background
        && style.underline.is_none()
    {
        style = AiTerminalCellStyle {
            color: render.default_foreground,
            background: render.default_background,
            underline: None,
        };
    }

    (style, overlay)
}

fn ai_terminal_paint_cursor_overlays(
    overlays: &[AiTerminalCursorOverlay],
    row_origin: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    window: &mut Window,
) {
    for overlay in overlays {
        match overlay.kind {
            AiTerminalCursorOverlayKind::Beam => {
                let beam_width = (cell_width * 0.14).max(px(2.0));
                window.paint_quad(fill(
                    Bounds {
                        origin: point(
                            row_origin.x + cell_width * overlay.column as f32,
                            row_origin.y + px(1.5),
                        ),
                        size: size(beam_width, (line_height - px(3.0)).max(px(1.0))),
                    },
                    overlay.color,
                ));
            }
            AiTerminalCursorOverlayKind::Outline => {
                let origin_x = row_origin.x + cell_width * overlay.column as f32;
                let origin_y = row_origin.y + px(1.0);
                let width = cell_width.max(px(1.0));
                let height = (line_height - px(2.0)).max(px(1.0));
                let thickness = px(1.0);

                window.paint_quad(fill(
                    Bounds {
                        origin: point(origin_x, origin_y),
                        size: size(width, thickness),
                    },
                    overlay.color,
                ));
                window.paint_quad(fill(
                    Bounds {
                        origin: point(origin_x, origin_y + height - thickness),
                        size: size(width, thickness),
                    },
                    overlay.color,
                ));
                window.paint_quad(fill(
                    Bounds {
                        origin: point(origin_x, origin_y),
                        size: size(thickness, height),
                    },
                    overlay.color,
                ));
                window.paint_quad(fill(
                    Bounds {
                        origin: point(origin_x + width - thickness, origin_y),
                        size: size(thickness, height),
                    },
                    overlay.color,
                ));
            }
        }
    }
}
