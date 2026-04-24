use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use hunk_browser::{
    BrowserAction, BrowserRuntime, BrowserSafetyDecision, classify_browser_action,
    redact_browser_tool_text,
};
use hunk_codex::browser_tools::{
    BrowserDynamicToolRequest, is_browser_dynamic_tool, parse_browser_dynamic_tool_request,
};
use hunk_codex::protocol::{
    DynamicToolCallOutputContentItem, DynamicToolCallParams, DynamicToolCallResponse,
};
use hunk_codex::tools::DynamicToolRegistry;
use image::ImageEncoder as _;
use serde_json::json;

#[derive(Debug, Clone)]
pub(crate) struct AiDynamicToolExecutor {
    workspace_tools: DynamicToolRegistry,
    browser_tools: AiBrowserDynamicToolExecutor,
}

impl Default for AiDynamicToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl AiDynamicToolExecutor {
    pub(crate) fn new() -> Self {
        Self {
            workspace_tools: DynamicToolRegistry::new(),
            browser_tools: AiBrowserDynamicToolExecutor::disabled(),
        }
    }

    pub(crate) fn with_state_only_browser() -> Self {
        Self {
            workspace_tools: DynamicToolRegistry::new(),
            browser_tools: AiBrowserDynamicToolExecutor::state_only(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_browser_runtime(runtime: BrowserRuntime) -> Self {
        Self {
            workspace_tools: DynamicToolRegistry::new(),
            browser_tools: AiBrowserDynamicToolExecutor {
                runtime: Some(runtime),
            },
        }
    }

    pub(crate) fn execute(
        &mut self,
        cwd: &Path,
        params: &DynamicToolCallParams,
    ) -> DynamicToolCallResponse {
        if is_browser_dynamic_tool(params.tool.as_str()) {
            return self.browser_tools.execute(params);
        }

        self.workspace_tools.execute(cwd, params)
    }
}

#[derive(Debug, Clone)]
struct AiBrowserDynamicToolExecutor {
    runtime: Option<BrowserRuntime>,
}

impl AiBrowserDynamicToolExecutor {
    fn disabled() -> Self {
        Self { runtime: None }
    }

    fn state_only() -> Self {
        Self {
            runtime: Some(BrowserRuntime::new_disabled()),
        }
    }

    fn execute(&mut self, params: &DynamicToolCallParams) -> DynamicToolCallResponse {
        let Some(runtime) = self.runtime.as_mut() else {
            return browser_unavailable_response(
                params,
                "The embedded browser executor is not connected yet.",
            );
        };
        execute_browser_dynamic_tool_with_runtime(runtime, params, false)
    }
}

pub(crate) fn execute_browser_dynamic_tool_with_runtime(
    runtime: &mut BrowserRuntime,
    params: &DynamicToolCallParams,
    use_backend: bool,
) -> DynamicToolCallResponse {
    let request = match parse_browser_dynamic_tool_request(params) {
        Ok(request) => request,
        Err(error) => {
            return json_error_response(json!({
            "error": "invalidBrowserToolArguments",
            "message": error,
            "tool": params.tool,
            "threadId": params.thread_id,
            "turnId": params.turn_id,
            }));
        }
    };

    match request {
        BrowserDynamicToolRequest::Snapshot => {
            if use_backend {
                if let Err(error) = runtime.capture_backend_snapshot(params.thread_id.as_str()) {
                    return json_error_response(json!({
                        "error": "browserSnapshotFailed",
                        "message": error.to_string(),
                        "tool": params.tool,
                        "threadId": params.thread_id,
                        "turnId": params.turn_id,
                    }));
                }
                let _ = runtime.pump_backend();
            }
            let session = runtime.ensure_session(params.thread_id.clone());
            json_success_response(snapshot_response(params, session))
        }
        BrowserDynamicToolRequest::Screenshot => {
            let session = runtime.ensure_session(params.thread_id.clone());
            let Some(frame) = session.latest_frame() else {
                return json_error_response(json!({
                    "error": "browserScreenshotUnavailable",
                    "message": "No browser frame has been captured yet.",
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                }));
            };
            let Some(image_url) = browser_frame_png_data_url(frame) else {
                return json_error_response(json!({
                    "error": "browserScreenshotEncodingFailed",
                    "message": "The latest browser frame could not be encoded as a PNG image.",
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                }));
            };
            json_success_response_with_items(
                json!({
                    "ok": true,
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                    "frame": frame.metadata(),
                    "message": "Screenshot frame is attached as an input image.",
                }),
                vec![DynamicToolCallOutputContentItem::InputImage { image_url }],
            )
        }
        BrowserDynamicToolRequest::Action(action) => {
            if let BrowserSafetyDecision::Prompt(kind) = classify_browser_action(&action) {
                return json_error_response(json!({
                    "error": "browserConfirmationRequired",
                    "message": "This browser action requires user confirmation before it can run.",
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                    "sensitiveAction": format!("{kind:?}"),
                }));
            }

            let action_result =
                if use_backend && runtime.status() == hunk_browser::BrowserRuntimeStatus::Ready {
                    runtime.apply_backend_action(params.thread_id.as_str(), &action)
                } else {
                    runtime.apply_state_only_action(params.thread_id.as_str(), &action)
                };

            if use_backend {
                let _ = runtime.pump_backend();
            }

            match action_result {
                Ok(()) => {
                    let session = runtime.ensure_session(params.thread_id.clone());
                    json_success_response(action_response(
                        params,
                        &action,
                        session.state(),
                        use_backend,
                    ))
                }
                Err(error) => json_error_response(json!({
                    "error": "browserActionRejected",
                    "message": error.to_string(),
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                })),
            }
        }
    }
}

pub(crate) fn browser_unavailable_response(
    params: &DynamicToolCallParams,
    message: &str,
) -> DynamicToolCallResponse {
    json_error_response(json!({
        "error": "browserUnavailable",
        "message": message,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
    }))
}

fn action_response(
    params: &DynamicToolCallParams,
    action: &BrowserAction,
    state: &hunk_browser::BrowserSessionState,
    use_backend: bool,
) -> serde_json::Value {
    json!({
        "ok": true,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
        "action": browser_action_label(action),
        "url": state.url,
        "title": state.title,
        "loading": state.loading,
        "snapshotEpoch": state.snapshot_epoch,
        "message": browser_action_message(action, use_backend),
    })
}

fn snapshot_response(
    params: &DynamicToolCallParams,
    session: &hunk_browser::BrowserSession,
) -> serde_json::Value {
    let state = session.state();
    let snapshot = session.latest_snapshot();
    let visible_text = snapshot
        .elements
        .iter()
        .flat_map(|element| [element.label.as_str(), element.text.as_str()])
        .filter(|text| !text.trim().is_empty())
        .map(redact_browser_tool_text)
        .collect::<Vec<_>>()
        .join("\n");
    let elements = snapshot
        .elements
        .iter()
        .map(|element| {
            json!({
                "index": element.index,
                "role": element.role,
                "label": redact_browser_tool_text(element.label.as_str()),
                "text": redact_browser_tool_text(element.text.as_str()),
                "rect": element.rect,
                "selector": element.selector,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "ok": true,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
        "snapshotEpoch": snapshot.epoch,
        "url": snapshot.url.as_ref().or(state.url.as_ref()),
        "title": snapshot.title.as_ref().or(state.title.as_ref()),
        "loading": state.loading,
        "viewport": snapshot.viewport,
        "scrollPosition": {
            "x": snapshot.viewport.scroll_x,
            "y": snapshot.viewport.scroll_y,
        },
        "visibleText": visible_text,
        "elements": elements,
    })
}

fn browser_action_label(action: &BrowserAction) -> &'static str {
    match action {
        BrowserAction::Navigate { .. } => "navigate",
        BrowserAction::Reload => "reload",
        BrowserAction::Stop => "stop",
        BrowserAction::Back => "back",
        BrowserAction::Forward => "forward",
        BrowserAction::Click { .. } => "click",
        BrowserAction::Type { .. } => "type",
        BrowserAction::Press { .. } => "press",
        BrowserAction::Scroll { .. } => "scroll",
        BrowserAction::Screenshot => "screenshot",
    }
}

fn browser_action_message(action: &BrowserAction, use_backend: bool) -> &'static str {
    if use_backend {
        return match action {
            BrowserAction::Navigate { .. } => "Navigation was sent to the embedded browser.",
            BrowserAction::Reload => "Reload was sent to the embedded browser.",
            BrowserAction::Stop => "Stop was sent to the embedded browser.",
            BrowserAction::Back => "Back navigation was sent to the embedded browser.",
            BrowserAction::Forward => "Forward navigation was sent to the embedded browser.",
            BrowserAction::Click { .. } => "Click was sent to the embedded browser.",
            BrowserAction::Type { .. } => "Text input was sent to the embedded browser.",
            BrowserAction::Press { .. } => "Key press was sent to the embedded browser.",
            BrowserAction::Scroll { .. } => "Scroll was sent to the embedded browser.",
            BrowserAction::Screenshot => "Screenshot was read from the embedded browser.",
        };
    }

    match action {
        BrowserAction::Navigate { .. } => "Navigation was accepted by the browser state layer.",
        BrowserAction::Reload => "Reload was accepted by the browser state layer.",
        BrowserAction::Stop => "Stop was accepted by the browser state layer.",
        BrowserAction::Back => "Back navigation was accepted by the browser state layer.",
        BrowserAction::Forward => "Forward navigation was accepted by the browser state layer.",
        BrowserAction::Click { .. } => "Click was accepted by the browser state layer.",
        BrowserAction::Type { .. } => "Type was accepted by the browser state layer.",
        BrowserAction::Press { .. } => "Key press was accepted by the browser state layer.",
        BrowserAction::Scroll { .. } => "Scroll was accepted by the browser state layer.",
        BrowserAction::Screenshot => "Screenshot was accepted by the browser state layer.",
    }
}

fn browser_frame_png_data_url(frame: &hunk_browser::BrowserFrame) -> Option<String> {
    let metadata = frame.metadata();
    let mut rgba = frame.bgra().to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }

    let image = image::RgbaImage::from_raw(metadata.width, metadata.height, rgba)?;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            image.as_raw(),
            metadata.width,
            metadata.height,
            image::ColorType::Rgba8.into(),
        )
        .ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD.encode(png)
    ))
}

fn json_success_response(value: serde_json::Value) -> DynamicToolCallResponse {
    json_response(value, true)
}

fn json_success_response_with_items(
    value: serde_json::Value,
    mut additional_items: Vec<DynamicToolCallOutputContentItem>,
) -> DynamicToolCallResponse {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|error| {
        format!("{{\"error\":\"browserResponseSerializationFailed\",\"message\":\"{error}\"}}")
    });
    let mut content_items = vec![DynamicToolCallOutputContentItem::InputText { text }];
    content_items.append(&mut additional_items);
    DynamicToolCallResponse {
        content_items,
        success: true,
    }
}

fn json_error_response(value: serde_json::Value) -> DynamicToolCallResponse {
    json_response(value, false)
}

fn json_response(value: serde_json::Value, success: bool) -> DynamicToolCallResponse {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|error| {
        format!("{{\"error\":\"browserResponseSerializationFailed\",\"message\":\"{error}\"}}")
    });
    DynamicToolCallResponse {
        content_items: vec![DynamicToolCallOutputContentItem::InputText { text }],
        success,
    }
}
