use std::cell::RefCell;
use std::collections::BTreeMap;
#[cfg(target_os = "macos")]
use std::os::unix::ffi::OsStrExt;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use cef::{args::Args, *};
use serde_json::json;

use crate::config::BrowserRuntimeConfig;
use crate::frame::{BrowserFrame, BrowserFrameRateLimiter};
use crate::session::{
    BrowserAction, BrowserConsoleLevel, BrowserError, BrowserInputModifiers, BrowserMouseButton,
    BrowserMouseInput, BrowserSession, BrowserSessionId, BrowserViewportSize,
};
use crate::snapshot::BrowserSnapshot;

const DEFAULT_URL: &str = "about:blank";
const DEVTOOLS_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(2);
const NEW_BROWSER_WARMUP_PUMP_ITERATIONS: usize = 8;

pub(crate) struct CefBrowserBackend {
    _app: cef::App,
    #[cfg(target_os = "macos")]
    _loader: MacCefLoader,
    browsers: BTreeMap<BrowserSessionId, CefBrowserHandle>,
    shared: Rc<RefCell<CefSharedState>>,
}

impl CefBrowserBackend {
    pub(crate) fn initialize(config: &BrowserRuntimeConfig) -> Result<Self, BrowserError> {
        #[cfg(target_os = "macos")]
        install_macos_nsapplication_compatibility();

        #[cfg(target_os = "macos")]
        let cef_paths = resolve_macos_cef_paths(config)?;
        #[cfg(target_os = "macos")]
        stage_macos_cef_sidecars_for_bare_run(&cef_paths)?;
        #[cfg(target_os = "macos")]
        let loader = load_macos_cef_framework(&cef_paths)?;
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        let cef_runtime_dir = resolve_flat_cef_runtime_dir(config)?;

        let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

        let args = Args::new();
        let cmd = args
            .as_cmd_line()
            .ok_or_else(|| backend_error("failed to read CEF command line"))?;
        let process_type_switch = CefString::from("type");
        let is_browser_process = cmd.has_switch(Some(&process_type_switch)) != 1;
        let shared = Rc::new(RefCell::new(CefSharedState::default()));
        let mut app = HunkCefAppBuilder::build(HunkCefApp {
            #[cfg(target_os = "macos")]
            cef_paths: cef_paths.clone(),
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            cef_runtime_dir: cef_runtime_dir.clone(),
        });

        if !is_browser_process {
            let process_result = execute_process(
                Some(args.as_main_args()),
                Some(&mut app),
                std::ptr::null_mut(),
            );
            return Err(backend_error(format!(
                "unexpected CEF subprocess execution in browser runtime: {process_result}"
            )));
        }

        let mut settings = Settings {
            browser_subprocess_path: CefString::from(
                config.helper_executable_path.to_string_lossy().as_ref(),
            ),
            root_cache_path: CefString::from(
                config
                    .storage_paths
                    .root_cache_path
                    .to_string_lossy()
                    .as_ref(),
            ),
            cache_path: CefString::from(
                config.storage_paths.profile_path.to_string_lossy().as_ref(),
            ),
            windowless_rendering_enabled: true as _,
            external_message_pump: true as _,
            no_sandbox: true as _,
            disable_signal_handlers: true as _,
            ..Default::default()
        };
        #[cfg(target_os = "macos")]
        {
            settings.framework_dir_path =
                CefString::from(cef_paths.framework_dir.to_string_lossy().as_ref());
            settings.resources_dir_path =
                CefString::from(cef_paths.resources_dir.to_string_lossy().as_ref());
            settings.locales_dir_path =
                CefString::from(cef_paths.resources_dir.to_string_lossy().as_ref());
        }
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            settings.resources_dir_path =
                CefString::from(cef_runtime_dir.to_string_lossy().as_ref());
            settings.locales_dir_path =
                CefString::from(cef_runtime_dir.join("locales").to_string_lossy().as_ref());
        }
        if initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            std::ptr::null_mut(),
        ) != 1
        {
            return Err(backend_error("CEF initialize failed"));
        }

        Ok(Self {
            _app: app,
            #[cfg(target_os = "macos")]
            _loader: loader,
            browsers: BTreeMap::new(),
            shared,
        })
    }

    pub(crate) fn ensure_session(
        &mut self,
        session_id: BrowserSessionId,
    ) -> Result<(), BrowserError> {
        if self.browsers.contains_key(&session_id) {
            return Ok(());
        }

        let render_handler = HunkCefRenderHandlerBuilder::build(HunkCefRenderHandler {
            session_id: session_id.clone(),
            shared: self.shared.clone(),
        });
        let load_handler = HunkCefLoadHandlerBuilder::build(HunkCefLoadHandler {
            session_id: session_id.clone(),
            shared: self.shared.clone(),
        });
        let display_handler = HunkCefDisplayHandlerBuilder::build(HunkCefDisplayHandler {
            session_id: session_id.clone(),
            shared: self.shared.clone(),
        });
        let context_menu_handler =
            HunkCefContextMenuHandlerBuilder::build(HunkCefContextMenuHandler);
        let mut devtools_observer =
            HunkCefDevToolsMessageObserverBuilder::build(HunkCefDevToolsMessageObserver {
                shared: self.shared.clone(),
            });
        let mut client = HunkCefClientBuilder::build(HunkCefClient {
            render_handler,
            load_handler,
            display_handler,
            context_menu_handler,
        });
        self.shared
            .borrow_mut()
            .set_viewport(session_id.clone(), BrowserViewportSize::default());

        let window_info = WindowInfo {
            windowless_rendering_enabled: true as _,
            ..Default::default()
        };
        let browser_settings = BrowserSettings {
            windowless_frame_rate: 60,
            ..Default::default()
        };

        let browser = browser_host_create_browser_sync(
            Some(&window_info),
            Some(&mut client),
            Some(&CefString::from(DEFAULT_URL)),
            Some(&browser_settings),
            None,
            None,
        )
        .ok_or_else(|| backend_error("CEF browser creation failed"))?;
        let devtools_registration = browser
            .host()
            .and_then(|host| host.add_dev_tools_message_observer(Some(&mut devtools_observer)));

        let handle = CefBrowserHandle {
            browser,
            _client: client,
            _devtools_observer: devtools_observer,
            _devtools_registration: devtools_registration,
        };
        warm_new_browser(&handle);

        self.browsers.insert(session_id.clone(), handle);
        Ok(())
    }

    pub(crate) fn capture_snapshot(
        &mut self,
        session_id: &BrowserSessionId,
        epoch: u64,
    ) -> Result<BrowserSnapshot, BrowserError> {
        let handle = self
            .browsers
            .get(session_id)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;
        let host = handle
            .browser
            .host()
            .ok_or_else(|| backend_error("CEF browser has no host"))?;
        let message_id = self.shared.borrow_mut().next_devtools_message_id();
        let expression = browser_snapshot_expression(epoch);
        let message = json!({
            "id": message_id,
            "method": "Runtime.evaluate",
            "params": {
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
            },
        })
        .to_string();

        if host.send_dev_tools_message(Some(message.as_bytes())) != 1 {
            return Err(backend_error(
                "failed to submit CEF DevTools snapshot request",
            ));
        }

        let deadline = Instant::now() + DEVTOOLS_SNAPSHOT_TIMEOUT;
        loop {
            do_message_loop_work();
            if let Some(host) = handle.browser.host() {
                host.send_external_begin_frame();
            }
            if let Some(result) = self.shared.borrow_mut().take_devtools_result(message_id) {
                return parse_devtools_snapshot_result(result);
            }
            if Instant::now() >= deadline {
                return Err(backend_error("timed out waiting for CEF DevTools snapshot"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub(crate) fn apply_action(
        &mut self,
        session_id: &BrowserSessionId,
        action: &BrowserAction,
    ) -> Result<(), BrowserError> {
        let handle = self
            .browsers
            .get(session_id)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;

        match action {
            BrowserAction::Navigate { url } => {
                let frame = handle
                    .browser
                    .main_frame()
                    .ok_or_else(|| backend_error("CEF browser has no main frame"))?;
                frame.load_url(Some(&CefString::from(url.as_str())));
            }
            BrowserAction::Reload => handle.browser.reload(),
            BrowserAction::Stop => handle.browser.stop_load(),
            BrowserAction::Back => handle.browser.go_back(),
            BrowserAction::Forward => handle.browser.go_forward(),
            BrowserAction::Screenshot => {}
            BrowserAction::Click { .. }
            | BrowserAction::Type { .. }
            | BrowserAction::Press { .. }
            | BrowserAction::Scroll { .. } => {}
        }

        Ok(())
    }

    pub(crate) fn resize_session(
        &mut self,
        session_id: &BrowserSessionId,
        viewport: BrowserViewportSize,
    ) -> Result<(), BrowserError> {
        let handle = self
            .browsers
            .get(session_id)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;
        self.shared
            .borrow_mut()
            .set_viewport(session_id.clone(), viewport);
        if let Some(host) = handle.browser.host() {
            host.was_resized();
            host.send_external_begin_frame();
        }
        Ok(())
    }

    pub(crate) fn focus_session(
        &mut self,
        session_id: &BrowserSessionId,
        focused: bool,
    ) -> Result<(), BrowserError> {
        let handle = self
            .browsers
            .get(session_id)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;
        if let Some(host) = handle.browser.host() {
            host.set_focus(focused as _);
        }
        Ok(())
    }

    pub(crate) fn send_mouse_move(
        &mut self,
        session_id: &BrowserSessionId,
        input: BrowserMouseInput,
    ) -> Result<(), BrowserError> {
        let handle = self
            .browsers
            .get(session_id)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;
        if let Some(host) = handle.browser.host() {
            let event = cef_mouse_event(input);
            host.send_mouse_move_event(Some(&event), false as _);
        }
        Ok(())
    }

    pub(crate) fn send_mouse_click(
        &mut self,
        session_id: &BrowserSessionId,
        input: BrowserMouseInput,
        button: BrowserMouseButton,
    ) -> Result<(), BrowserError> {
        let handle = self
            .browsers
            .get(session_id)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;
        if let Some(host) = handle.browser.host() {
            let event = cef_mouse_event(input);
            let button = cef_mouse_button(button);
            host.set_focus(true as _);
            host.send_mouse_move_event(Some(&event), false as _);
            host.send_mouse_click_event(Some(&event), button, false as _, 1);
            host.send_mouse_click_event(Some(&event), button, true as _, 1);
        }
        Ok(())
    }

    pub(crate) fn send_mouse_wheel(
        &mut self,
        session_id: &BrowserSessionId,
        input: BrowserMouseInput,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<(), BrowserError> {
        let handle = self
            .browsers
            .get(session_id)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;
        if let Some(host) = handle.browser.host() {
            let event = cef_mouse_event(input);
            host.send_mouse_wheel_event(Some(&event), delta_x, delta_y);
        }
        Ok(())
    }

    pub(crate) fn send_text(
        &mut self,
        session_id: &BrowserSessionId,
        text: &str,
    ) -> Result<(), BrowserError> {
        let handle = self
            .browsers
            .get(session_id)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;
        if let Some(host) = handle.browser.host() {
            for unit in text.encode_utf16() {
                let event = cef_key_event(KeyEventType::CHAR, unit as i32, unit, 0);
                host.send_key_event(Some(&event));
            }
        }
        Ok(())
    }

    pub(crate) fn send_key_press(
        &mut self,
        session_id: &BrowserSessionId,
        keys: &str,
    ) -> Result<(), BrowserError> {
        let handle = self
            .browsers
            .get(session_id)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;
        let Some(host) = handle.browser.host() else {
            return Ok(());
        };
        let Some(key) = parse_key_press(keys) else {
            return Err(backend_error(format!(
                "unsupported browser key press '{keys}'"
            )));
        };

        let down = cef_key_event(
            KeyEventType::RAWKEYDOWN,
            key.key_code,
            key.character,
            key.flags,
        );
        host.send_key_event(Some(&down));
        if key.character != 0 {
            let char_event =
                cef_key_event(KeyEventType::CHAR, key.key_code, key.character, key.flags);
            host.send_key_event(Some(&char_event));
        }
        let up = cef_key_event(KeyEventType::KEYUP, key.key_code, key.character, key.flags);
        host.send_key_event(Some(&up));
        Ok(())
    }

    pub(crate) fn pump(
        &mut self,
        sessions: &mut BTreeMap<BrowserSessionId, BrowserSession>,
    ) -> Result<bool, BrowserError> {
        do_message_loop_work();

        for handle in self.browsers.values() {
            if let Some(host) = handle.browser.host() {
                host.send_external_begin_frame();
            }
        }

        let events = self.shared.borrow_mut().drain();
        let changed = !events.frames.is_empty()
            || !events.loads.is_empty()
            || !events.console_entries.is_empty()
            || !events.viewports.is_empty();
        for frame in events.frames {
            if let Some(session) = sessions.get_mut(&frame.session_id) {
                session.set_latest_frame(frame.frame);
            }
        }
        for load in events.loads {
            if let Some(session) = sessions.get_mut(&load.session_id) {
                match load.state {
                    CefLoadState::Changed {
                        loading,
                        can_go_back,
                        can_go_forward,
                    } => {
                        session.apply_backend_loading_state(loading, can_go_back, can_go_forward);
                    }
                    CefLoadState::Failed(error) => session.set_load_error(error),
                }
            }
        }
        for entry in events.console_entries {
            if let Some(session) = sessions.get_mut(&entry.session_id) {
                session.push_console_entry(
                    entry.level,
                    entry.message,
                    entry.source,
                    entry.line,
                    entry.timestamp_ms,
                );
            }
        }
        for (session_id, viewport) in events.viewports {
            if let Some(session) = sessions.get_mut(&session_id) {
                session.set_viewport(viewport);
            }
        }

        Ok(changed)
    }

    pub(crate) fn shutdown(&mut self) {
        for handle in self.browsers.values() {
            if let Some(host) = handle.browser.host() {
                host.close_browser(true as _);
            }
        }
        for _ in 0..10 {
            do_message_loop_work();
            std::thread::sleep(Duration::from_millis(5));
        }
        self.browsers.clear();
        shutdown();
    }
}

struct CefBrowserHandle {
    browser: Browser,
    _client: Client,
    _devtools_observer: DevToolsMessageObserver,
    _devtools_registration: Option<Registration>,
}

fn warm_new_browser(handle: &CefBrowserHandle) {
    if let Some(host) = handle.browser.host() {
        host.was_resized();
    }
    for _ in 0..NEW_BROWSER_WARMUP_PUMP_ITERATIONS {
        do_message_loop_work();
        if let Some(host) = handle.browser.host() {
            host.send_external_begin_frame();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[derive(Default)]
struct CefSharedState {
    next_frame_epoch: u64,
    next_devtools_message_id: i32,
    frame_limiters: BTreeMap<BrowserSessionId, BrowserFrameRateLimiter>,
    frames: Vec<CefFrameEvent>,
    loads: Vec<CefLoadEvent>,
    console_entries: Vec<CefConsoleEvent>,
    viewports: BTreeMap<BrowserSessionId, BrowserViewportSize>,
    viewport_events: Vec<(BrowserSessionId, BrowserViewportSize)>,
    devtools_results: BTreeMap<i32, CefDevToolsResult>,
}

impl CefSharedState {
    fn push_frame(&mut self, session_id: BrowserSessionId, width: i32, height: i32, bgra: Vec<u8>) {
        let now = Instant::now();
        if !self
            .frame_limiters
            .entry(session_id.clone())
            .or_insert_with(BrowserFrameRateLimiter::v1_60fps)
            .should_notify(now)
        {
            return;
        }

        self.next_frame_epoch = self.next_frame_epoch.saturating_add(1);
        let Ok(frame) =
            BrowserFrame::from_bgra(width as u32, height as u32, self.next_frame_epoch, bgra)
        else {
            return;
        };
        self.frames.push(CefFrameEvent { session_id, frame });
    }

    fn push_load(&mut self, session_id: BrowserSessionId, state: CefLoadState) {
        self.loads.push(CefLoadEvent { session_id, state });
    }

    fn push_console_entry(
        &mut self,
        session_id: BrowserSessionId,
        level: BrowserConsoleLevel,
        message: String,
        source: Option<String>,
        line: Option<u32>,
    ) {
        self.console_entries.push(CefConsoleEvent {
            session_id,
            level,
            message,
            source,
            line,
            timestamp_ms: browser_console_timestamp_ms(),
        });
    }

    fn set_viewport(&mut self, session_id: BrowserSessionId, viewport: BrowserViewportSize) {
        self.viewports.insert(session_id.clone(), viewport);
        self.viewport_events.push((session_id, viewport));
    }

    fn next_devtools_message_id(&mut self) -> i32 {
        self.next_devtools_message_id = self.next_devtools_message_id.saturating_add(1).max(1);
        self.next_devtools_message_id
    }

    fn push_devtools_result(&mut self, result: CefDevToolsResult) {
        self.devtools_results.insert(result.message_id, result);
    }

    fn take_devtools_result(&mut self, message_id: i32) -> Option<CefDevToolsResult> {
        self.devtools_results.remove(&message_id)
    }

    fn viewport(&self, session_id: &BrowserSessionId) -> BrowserViewportSize {
        self.viewports.get(session_id).copied().unwrap_or_default()
    }

    fn drain(&mut self) -> CefSharedDrain {
        CefSharedDrain {
            frames: std::mem::take(&mut self.frames),
            loads: std::mem::take(&mut self.loads),
            console_entries: std::mem::take(&mut self.console_entries),
            viewports: std::mem::take(&mut self.viewport_events),
        }
    }
}

struct CefSharedDrain {
    frames: Vec<CefFrameEvent>,
    loads: Vec<CefLoadEvent>,
    console_entries: Vec<CefConsoleEvent>,
    viewports: Vec<(BrowserSessionId, BrowserViewportSize)>,
}

struct CefFrameEvent {
    session_id: BrowserSessionId,
    frame: BrowserFrame,
}

struct CefLoadEvent {
    session_id: BrowserSessionId,
    state: CefLoadState,
}

struct CefConsoleEvent {
    session_id: BrowserSessionId,
    level: BrowserConsoleLevel,
    message: String,
    source: Option<String>,
    line: Option<u32>,
    timestamp_ms: u64,
}

struct CefDevToolsResult {
    message_id: i32,
    success: bool,
    result: Option<String>,
}

enum CefLoadState {
    Changed {
        loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    },
    Failed(String),
}

#[derive(Clone)]
struct HunkCefApp {
    #[cfg(target_os = "macos")]
    cef_paths: MacCefPaths,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    cef_runtime_dir: PathBuf,
}

wrap_app! {
    struct HunkCefAppBuilder {
        app: HunkCefApp,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&cef::CefStringUtf16>,
            command_line: Option<&mut cef::CommandLine>,
        ) {
            let Some(command_line) = command_line else {
                return;
            };

            command_line.append_switch(Some(&"no-startup-window".into()));
            command_line.append_switch(Some(&"noerrdialogs".into()));
            command_line.append_switch(Some(&"hide-crash-restore-bubble".into()));
            command_line.append_switch(Some(&"use-mock-keychain".into()));
            command_line.append_switch(Some(&"enable-logging=stderr".into()));
            #[cfg(target_os = "macos")]
            append_macos_cef_switches(command_line, &self.app.cef_paths);
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            append_flat_cef_switches(command_line, &self.app.cef_runtime_dir);
        }

        fn browser_process_handler(&self) -> Option<cef::BrowserProcessHandler> {
            Some(HunkCefBrowserProcessHandlerBuilder::build(
                HunkCefBrowserProcessHandler {
                    #[cfg(target_os = "macos")]
                    cef_paths: self.app.cef_paths.clone(),
                    #[cfg(any(target_os = "linux", target_os = "windows"))]
                    cef_runtime_dir: self.app.cef_runtime_dir.clone(),
                },
            ))
        }
    }
}

impl HunkCefAppBuilder {
    fn build(app: HunkCefApp) -> cef::App {
        Self::new(app)
    }
}

#[derive(Clone)]
struct HunkCefBrowserProcessHandler {
    #[cfg(target_os = "macos")]
    cef_paths: MacCefPaths,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    cef_runtime_dir: PathBuf,
}

wrap_browser_process_handler! {
    struct HunkCefBrowserProcessHandlerBuilder {
        handler: HunkCefBrowserProcessHandler,
    }

    impl BrowserProcessHandler {
        fn on_before_child_process_launch(&self, command_line: Option<&mut CommandLine>) {
            let Some(command_line) = command_line else {
                return;
            };

            command_line.append_switch(Some(&"disable-session-crashed-bubble".into()));
            command_line.append_switch(Some(&"enable-logging=stderr".into()));
            #[cfg(target_os = "macos")]
            append_macos_cef_switches(command_line, &self.handler.cef_paths);
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            append_flat_cef_switches(command_line, &self.handler.cef_runtime_dir);
        }
    }
}

impl HunkCefBrowserProcessHandlerBuilder {
    fn build(handler: HunkCefBrowserProcessHandler) -> BrowserProcessHandler {
        Self::new(handler)
    }
}

#[derive(Clone)]
struct HunkCefRenderHandler {
    session_id: BrowserSessionId,
    shared: Rc<RefCell<CefSharedState>>,
}

wrap_render_handler! {
    struct HunkCefRenderHandlerBuilder {
        handler: HunkCefRenderHandler,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                let viewport = self
                    .handler
                    .shared
                    .borrow()
                    .viewport(&self.handler.session_id);
                rect.width = viewport.width.min(i32::MAX as u32) as i32;
                rect.height = viewport.height.min(i32::MAX as u32) as i32;
            }
        }

        fn screen_info(
            &self,
            _browser: Option<&mut Browser>,
            screen_info: Option<&mut ScreenInfo>,
        ) -> ::std::os::raw::c_int {
            if let Some(screen_info) = screen_info {
                screen_info.device_scale_factor = self
                    .handler
                    .shared
                    .borrow()
                    .viewport(&self.handler.session_id)
                    .device_scale_factor;
                return true as _;
            }
            false as _
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ) {
            if type_ != PaintElementType::VIEW || buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }

            let buffer_len = (width as usize)
                .saturating_mul(height as usize)
                .saturating_mul(4);
            let pixels = unsafe { std::slice::from_raw_parts(buffer, buffer_len) }.to_vec();
            self.handler.shared.borrow_mut().push_frame(
                self.handler.session_id.clone(),
                width,
                height,
                pixels,
            );
        }
    }
}

impl HunkCefRenderHandlerBuilder {
    fn build(handler: HunkCefRenderHandler) -> RenderHandler {
        Self::new(handler)
    }
}

#[derive(Clone)]
struct HunkCefLoadHandler {
    session_id: BrowserSessionId,
    shared: Rc<RefCell<CefSharedState>>,
}

wrap_load_handler! {
    struct HunkCefLoadHandlerBuilder {
        handler: HunkCefLoadHandler,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            let state = CefLoadState::Changed {
                loading: is_loading == 1,
                can_go_back: can_go_back == 1,
                can_go_forward: can_go_forward == 1,
            };
            self.handler
                .shared
                .borrow_mut()
                .push_load(self.handler.session_id.clone(), state);
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            self.handler.shared.borrow_mut().push_load(
                self.handler.session_id.clone(),
                CefLoadState::Failed(format!(
                    "load failed for {}: {:?} {}",
                    failed_url.map(CefString::to_string).unwrap_or_default(),
                    error_code,
                    error_text.map(CefString::to_string).unwrap_or_default()
                )),
            );
        }
    }
}

impl HunkCefLoadHandlerBuilder {
    fn build(handler: HunkCefLoadHandler) -> LoadHandler {
        Self::new(handler)
    }
}

#[derive(Clone)]
struct HunkCefDevToolsMessageObserver {
    shared: Rc<RefCell<CefSharedState>>,
}

wrap_dev_tools_message_observer! {
    struct HunkCefDevToolsMessageObserverBuilder {
        observer: HunkCefDevToolsMessageObserver,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            _browser: Option<&mut Browser>,
            message_id: ::std::os::raw::c_int,
            success: ::std::os::raw::c_int,
            result: Option<&[u8]>,
        ) {
            let result = result.and_then(|bytes| std::str::from_utf8(bytes).ok()).map(str::to_string);
            self.observer
                .shared
                .borrow_mut()
                .push_devtools_result(CefDevToolsResult {
                    message_id,
                    success: success == 1,
                    result,
                });
        }
    }
}

impl HunkCefDevToolsMessageObserverBuilder {
    fn build(observer: HunkCefDevToolsMessageObserver) -> DevToolsMessageObserver {
        Self::new(observer)
    }
}

#[derive(Clone)]
struct HunkCefClient {
    render_handler: RenderHandler,
    load_handler: LoadHandler,
    display_handler: DisplayHandler,
    context_menu_handler: ContextMenuHandler,
}

wrap_client! {
    struct HunkCefClientBuilder {
        client: HunkCefClient,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.client.render_handler.clone())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.client.load_handler.clone())
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(self.client.display_handler.clone())
        }

        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(self.client.context_menu_handler.clone())
        }
    }
}

impl HunkCefClientBuilder {
    fn build(client: HunkCefClient) -> Client {
        Self::new(client)
    }
}

#[derive(Clone)]
struct HunkCefDisplayHandler {
    session_id: BrowserSessionId,
    shared: Rc<RefCell<CefSharedState>>,
}

wrap_display_handler! {
    struct HunkCefDisplayHandlerBuilder {
        handler: HunkCefDisplayHandler,
    }

    impl DisplayHandler {
        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            self.handler.shared.borrow_mut().push_console_entry(
                self.handler.session_id.clone(),
                browser_console_level(level),
                message.map(CefString::to_string).unwrap_or_default(),
                source.map(CefString::to_string).filter(|source| !source.is_empty()),
                u32::try_from(line).ok().filter(|line| *line > 0),
            );
            true as _
        }
    }
}

impl HunkCefDisplayHandlerBuilder {
    fn build(handler: HunkCefDisplayHandler) -> DisplayHandler {
        Self::new(handler)
    }
}

#[derive(Clone)]
struct HunkCefContextMenuHandler;

wrap_context_menu_handler! {
    struct HunkCefContextMenuHandlerBuilder {
        handler: HunkCefContextMenuHandler,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            if let Some(model) = model {
                model.clear();
            }
        }

        fn run_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            _model: Option<&mut MenuModel>,
            callback: Option<&mut RunContextMenuCallback>,
        ) -> ::std::os::raw::c_int {
            if let Some(callback) = callback {
                callback.cancel();
            }
            true as _
        }
    }
}

impl HunkCefContextMenuHandlerBuilder {
    fn build(handler: HunkCefContextMenuHandler) -> ContextMenuHandler {
        Self::new(handler)
    }
}

const EVENTFLAG_SHIFT_DOWN: u32 = 2;
const EVENTFLAG_CONTROL_DOWN: u32 = 4;
const EVENTFLAG_ALT_DOWN: u32 = 8;
const EVENTFLAG_COMMAND_DOWN: u32 = 128;

struct ParsedKeyPress {
    key_code: i32,
    character: u16,
    flags: u32,
}

fn cef_mouse_event(input: BrowserMouseInput) -> MouseEvent {
    MouseEvent {
        x: input.point.x,
        y: input.point.y,
        modifiers: cef_input_modifiers(input.modifiers),
    }
}

fn cef_mouse_button(button: BrowserMouseButton) -> MouseButtonType {
    match button {
        BrowserMouseButton::Left => MouseButtonType::LEFT,
        BrowserMouseButton::Middle => MouseButtonType::MIDDLE,
        BrowserMouseButton::Right => MouseButtonType::RIGHT,
    }
}

fn browser_console_level(level: LogSeverity) -> BrowserConsoleLevel {
    match level.get_raw() {
        raw if raw == LogSeverity::VERBOSE.get_raw() => BrowserConsoleLevel::Verbose,
        raw if raw == LogSeverity::WARNING.get_raw() => BrowserConsoleLevel::Warning,
        raw if raw == LogSeverity::ERROR.get_raw() || raw == LogSeverity::FATAL.get_raw() => {
            BrowserConsoleLevel::Error
        }
        _ => BrowserConsoleLevel::Info,
    }
}

fn browser_console_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

fn cef_input_modifiers(modifiers: BrowserInputModifiers) -> u32 {
    let mut flags = 0;
    if modifiers.shift {
        flags |= EVENTFLAG_SHIFT_DOWN;
    }
    if modifiers.control {
        flags |= EVENTFLAG_CONTROL_DOWN;
    }
    if modifiers.alt {
        flags |= EVENTFLAG_ALT_DOWN;
    }
    if modifiers.meta {
        flags |= EVENTFLAG_COMMAND_DOWN;
    }
    flags
}

fn cef_key_event(type_: KeyEventType, key_code: i32, character: u16, flags: u32) -> KeyEvent {
    KeyEvent {
        type_,
        modifiers: flags,
        windows_key_code: key_code,
        native_key_code: key_code,
        character,
        unmodified_character: character,
        ..Default::default()
    }
}

fn parse_key_press(keys: &str) -> Option<ParsedKeyPress> {
    let mut flags = 0;
    let mut key = None;
    for part in keys
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part.to_ascii_lowercase().as_str() {
            "shift" => flags |= EVENTFLAG_SHIFT_DOWN,
            "ctrl" | "control" => flags |= EVENTFLAG_CONTROL_DOWN,
            "alt" | "option" => flags |= EVENTFLAG_ALT_DOWN,
            "cmd" | "command" | "meta" | "super" => flags |= EVENTFLAG_COMMAND_DOWN,
            _ => key = Some(part),
        }
    }

    let key = key?;
    let (key_code, character) = match key.to_ascii_lowercase().as_str() {
        "enter" | "return" => (13, 0),
        "tab" => (9, 0),
        "escape" | "esc" => (27, 0),
        "backspace" => (8, 0),
        "delete" | "del" => (46, 0),
        "arrowleft" | "left" => (37, 0),
        "arrowup" | "up" => (38, 0),
        "arrowright" | "right" => (39, 0),
        "arrowdown" | "down" => (40, 0),
        "home" => (36, 0),
        "end" => (35, 0),
        "pageup" => (33, 0),
        "pagedown" => (34, 0),
        "space" => (32, b' ' as u16),
        _ => parse_printable_key(key, flags)?,
    };

    Some(ParsedKeyPress {
        key_code,
        character,
        flags,
    })
}

fn parse_printable_key(key: &str, flags: u32) -> Option<(i32, u16)> {
    let mut chars = key.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    if !ch.is_ascii() {
        return None;
    }

    let ascii = ch as u8;
    let key_code = if ascii.is_ascii_alphabetic() {
        ascii.to_ascii_uppercase() as i32
    } else {
        ascii as i32
    };
    let character = if flags == 0 { ascii as u16 } else { 0 };
    Some((key_code, character))
}

fn browser_snapshot_expression(epoch: u64) -> String {
    const EXPRESSION: &str = r#"
(() => {
  const trim = value => String(value || '').replace(/\s+/g, ' ').trim().slice(0, 500);
  const isSensitiveValueElement = element => {
    const tag = element.tagName.toLowerCase();
    if (tag !== 'input' && tag !== 'textarea') return false;
    const type = (element.getAttribute('type') || '').toLowerCase();
    const autocomplete = (element.getAttribute('autocomplete') || '').toLowerCase();
    const name = (element.getAttribute('name') || '').toLowerCase();
    const id = (element.id || '').toLowerCase();
    return type === 'password' ||
      autocomplete.includes('password') ||
      autocomplete.includes('one-time-code') ||
      /password|passwd|passcode|otp|token|secret|api[-_]?key|credential/.test(name) ||
      /password|passwd|passcode|otp|token|secret|api[-_]?key|credential/.test(id);
  };
  const viewportWidth = Math.max(0, Math.round(window.innerWidth || document.documentElement.clientWidth || 0));
  const viewportHeight = Math.max(0, Math.round(window.innerHeight || document.documentElement.clientHeight || 0));
  const visible = element => {
    const style = window.getComputedStyle(element);
    if (!style || style.visibility === 'hidden' || style.display === 'none' || Number(style.opacity) === 0) {
      return false;
    }
    const rect = element.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0 && rect.bottom >= 0 && rect.right >= 0 &&
      rect.top <= viewportHeight && rect.left <= viewportWidth;
  };
  const roleFor = element => {
    const explicit = element.getAttribute('role');
    if (explicit) return explicit;
    const tag = element.tagName.toLowerCase();
    if (tag === 'a') return 'link';
    if (tag === 'button') return 'button';
    if (tag === 'textarea') return 'textbox';
    if (tag === 'select') return 'combobox';
    if (tag === 'summary') return 'button';
    if (tag === 'input') {
      const type = (element.getAttribute('type') || 'text').toLowerCase();
      if (type === 'checkbox') return 'checkbox';
      if (type === 'radio') return 'radio';
      if (type === 'submit' || type === 'button' || type === 'reset') return 'button';
      return 'textbox';
    }
    return 'generic';
  };
  const labelFor = element => {
    const labelledBy = element.getAttribute('aria-labelledby');
    if (labelledBy) {
      const labelledText = labelledBy
        .split(/\s+/)
        .map(id => document.getElementById(id))
        .filter(Boolean)
        .map(node => node.innerText || node.textContent || '')
        .join(' ');
      if (trim(labelledText)) return trim(labelledText);
    }
    if (element.labels && element.labels.length) {
      const labelText = Array.from(element.labels).map(label => label.innerText || label.textContent || '').join(' ');
      if (trim(labelText)) return trim(labelText);
    }
    return trim(
      element.getAttribute('aria-label') ||
      element.getAttribute('alt') ||
      element.getAttribute('title') ||
      element.getAttribute('placeholder') ||
      (isSensitiveValueElement(element) ? '' : element.getAttribute('value')) ||
      element.innerText ||
      element.textContent
    );
  };
  const selectorFor = element => {
    if (element.id) return `#${CSS.escape(element.id)}`;
    const parts = [];
    let current = element;
    while (current && current.nodeType === Node.ELEMENT_NODE && current !== document.documentElement && parts.length < 4) {
      let part = current.tagName.toLowerCase();
      const name = current.getAttribute('name');
      if (name) part += `[name="${CSS.escape(name)}"]`;
      const parent = current.parentElement;
      if (parent) {
        const siblings = Array.from(parent.children).filter(child => child.tagName === current.tagName);
        if (siblings.length > 1) part += `:nth-of-type(${siblings.indexOf(current) + 1})`;
      }
      parts.unshift(part);
      current = parent;
    }
    return parts.length ? parts.join(' > ') : null;
  };
  const candidates = Array.from(document.querySelectorAll([
    'a[href]',
    'button',
    'input',
    'textarea',
    'select',
    'summary',
    '[contenteditable="true"]',
    '[role="button"]',
    '[role="link"]',
    '[role="textbox"]',
    '[role="checkbox"]',
    '[role="radio"]',
    '[role="combobox"]',
    '[tabindex]:not([tabindex="-1"])'
  ].join(',')));
  const elements = [];
  for (const element of candidates) {
    if (!visible(element) || element.disabled || element.getAttribute('aria-hidden') === 'true') continue;
    const rect = element.getBoundingClientRect();
    const label = labelFor(element);
    const text = isSensitiveValueElement(element) ? '' : trim(element.value || element.innerText || element.textContent);
    if (!label && !text && roleFor(element) === 'generic') continue;
    elements.push({
      index: elements.length,
      role: roleFor(element),
      label,
      text,
      rect: {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height
      },
      selector: selectorFor(element)
    });
    if (elements.length >= 200) break;
  }
  return {
    epoch: __HUNK_EPOCH__,
    url: window.location.href || null,
    title: document.title || null,
    viewport: {
      width: viewportWidth,
      height: viewportHeight,
      deviceScaleFactor: window.devicePixelRatio || 1,
      scrollX: window.scrollX || 0,
      scrollY: window.scrollY || 0
    },
    elements
  };
})()
"#;

    EXPRESSION.replace("__HUNK_EPOCH__", &epoch.to_string())
}

fn parse_devtools_snapshot_result(
    result: CefDevToolsResult,
) -> Result<BrowserSnapshot, BrowserError> {
    if !result.success {
        return Err(backend_error(format!(
            "CEF DevTools snapshot failed: {}",
            result
                .result
                .unwrap_or_else(|| "missing error payload".to_string())
        )));
    }

    let raw = result
        .result
        .ok_or_else(|| backend_error("CEF DevTools snapshot returned no result"))?;
    let value: serde_json::Value = serde_json::from_str(raw.as_str()).map_err(|error| {
        backend_error(format!(
            "failed to parse CEF DevTools snapshot result: {error}"
        ))
    })?;
    if let Some(exception) = value.get("exceptionDetails") {
        return Err(backend_error(format!(
            "CEF DevTools snapshot JavaScript failed: {exception}"
        )));
    }
    let Some(snapshot) = value.get("result").and_then(|result| result.get("value")) else {
        return Err(backend_error(format!(
            "CEF DevTools snapshot missing result.value: {value}"
        )));
    };
    serde_json::from_value(snapshot.clone()).map_err(|error| {
        backend_error(format!(
            "failed to decode CEF DevTools browser snapshot: {error}"
        ))
    })
}

fn backend_error(message: impl Into<String>) -> BrowserError {
    BrowserError::BackendUnavailable(message.into())
}

#[cfg(test)]
mod tests {
    use super::browser_snapshot_expression;

    #[test]
    fn snapshot_expression_redacts_sensitive_values_and_uses_viewport_rects() {
        let expression = browser_snapshot_expression(1);

        assert!(expression.contains("isSensitiveValueElement(element) ? ''"));
        assert!(!expression.contains("x: rect.x + window.scrollX"));
        assert!(!expression.contains("y: rect.y + window.scrollY"));
    }
}

#[cfg(target_os = "macos")]
struct MacCefLoader {
    path: PathBuf,
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct MacCefPaths {
    framework_dir: PathBuf,
    resources_dir: PathBuf,
}

#[cfg(target_os = "macos")]
impl Drop for MacCefLoader {
    fn drop(&mut self) {
        if cef::unload_library() != 1 {
            eprintln!("cannot unload framework {}", self.path.display());
        }
    }
}

#[cfg(target_os = "macos")]
fn load_macos_cef_framework(paths: &MacCefPaths) -> Result<MacCefLoader, BrowserError> {
    let framework_path = paths.framework_dir.join("Chromium Embedded Framework");
    let name = std::ffi::CString::new(framework_path.as_os_str().as_bytes())
        .map_err(|error| backend_error(format!("invalid CEF framework path: {error}")))?;

    let library_path = unsafe { &*name.as_ptr().cast() };
    if cef::load_library(Some(library_path)) == 1 {
        Ok(MacCefLoader {
            path: framework_path,
        })
    } else {
        Err(backend_error("failed to load Chromium Embedded Framework"))
    }
}

#[cfg(target_os = "macos")]
fn resolve_macos_cef_paths(config: &BrowserRuntimeConfig) -> Result<MacCefPaths, BrowserError> {
    const FRAMEWORK_DIR: &str = "Chromium Embedded Framework.framework";

    let app_framework = std::env::current_exe()
        .ok()
        .and_then(|current_exe| {
            current_exe
                .parent()
                .map(|macos| macos.join("../Frameworks"))
        })
        .map(|frameworks| frameworks.join(FRAMEWORK_DIR));
    let runtime_framework = config.cef_runtime_dir.join(FRAMEWORK_DIR);
    let framework_dir = app_framework
        .filter(|path| path.exists())
        .unwrap_or(runtime_framework)
        .canonicalize()
        .map_err(|error| {
            backend_error(format!(
                "failed to resolve Chromium Embedded Framework from {}: {error}",
                config.cef_runtime_dir.display()
            ))
        })?;
    let resources_dir = framework_dir.join("Resources");

    let icu_data = resources_dir.join("icudtl.dat");
    if !icu_data.is_file() {
        return Err(backend_error(format!(
            "Chromium Embedded Framework resources are missing {}",
            icu_data.display()
        )));
    }

    Ok(MacCefPaths {
        framework_dir,
        resources_dir,
    })
}

#[cfg(target_os = "macos")]
fn append_macos_cef_switches(command_line: &mut CommandLine, paths: &MacCefPaths) {
    command_line.append_switch_with_value(
        Some(&"framework-dir-path".into()),
        Some(&CefString::from(
            paths.framework_dir.to_string_lossy().as_ref(),
        )),
    );
    command_line.append_switch_with_value(
        Some(&"resources-dir-path".into()),
        Some(&CefString::from(
            paths.resources_dir.to_string_lossy().as_ref(),
        )),
    );
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn resolve_flat_cef_runtime_dir(config: &BrowserRuntimeConfig) -> Result<PathBuf, BrowserError> {
    let runtime_dir = config.cef_runtime_dir.canonicalize().map_err(|error| {
        backend_error(format!(
            "failed to resolve Chromium Embedded Framework runtime from {}: {error}",
            config.cef_runtime_dir.display()
        ))
    })?;

    let icu_data = runtime_dir.join("icudtl.dat");
    if !icu_data.is_file() {
        return Err(backend_error(format!(
            "Chromium Embedded Framework resources are missing {}",
            icu_data.display()
        )));
    }

    #[cfg(target_os = "linux")]
    let cef_library = runtime_dir.join("libcef.so");
    #[cfg(target_os = "windows")]
    let cef_library = runtime_dir.join("libcef.dll");
    if !cef_library.is_file() {
        return Err(backend_error(format!(
            "Chromium Embedded Framework library is missing {}",
            cef_library.display()
        )));
    }

    let locales_dir = runtime_dir.join("locales");
    if !locales_dir.is_dir() {
        return Err(backend_error(format!(
            "Chromium Embedded Framework locales are missing {}",
            locales_dir.display()
        )));
    }

    Ok(runtime_dir)
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn append_flat_cef_switches(command_line: &mut CommandLine, runtime_dir: &PathBuf) {
    command_line.append_switch_with_value(
        Some(&"resources-dir-path".into()),
        Some(&CefString::from(runtime_dir.to_string_lossy().as_ref())),
    );
    command_line.append_switch_with_value(
        Some(&"locales-dir-path".into()),
        Some(&CefString::from(
            runtime_dir.join("locales").to_string_lossy().as_ref(),
        )),
    );
}

#[cfg(target_os = "macos")]
fn install_macos_nsapplication_compatibility() {
    use std::ffi::c_void;
    use std::os::raw::{c_char, c_schar};
    use std::sync::OnceLock;

    static INSTALLED: OnceLock<()> = OnceLock::new();

    INSTALLED.get_or_init(|| {
        unsafe extern "C" {
            fn objc_getClass(name: *const c_char) -> *mut c_void;
            fn sel_registerName(name: *const c_char) -> *mut c_void;
            fn class_getInstanceMethod(cls: *mut c_void, name: *mut c_void) -> *mut c_void;
            fn class_addMethod(
                cls: *mut c_void,
                name: *mut c_void,
                imp: *const c_void,
                types: *const c_char,
            ) -> bool;
        }

        extern "C" fn is_handling_send_event(
            _this: *mut c_void,
            _selector: *mut c_void,
        ) -> c_schar {
            0
        }

        extern "C" fn set_handling_send_event(
            _this: *mut c_void,
            _selector: *mut c_void,
            _handling_send_event: c_schar,
        ) {
        }

        // Some Chromium macOS paths ask NSApp whether it is inside sendEvent:.
        // GPUI's NSApplication subclass does not currently implement Chromium's
        // private CrAppProtocol selectors, so add conservative no-op responses.
        unsafe {
            let class_name = c"GPUIApplication";
            let class = objc_getClass(class_name.as_ptr());
            if class.is_null() {
                return;
            }
            add_missing_macos_method(
                class,
                c"isHandlingSendEvent",
                is_handling_send_event as *const c_void,
                c"c@:",
                sel_registerName,
                class_getInstanceMethod,
                class_addMethod,
            );
            add_missing_macos_method(
                class,
                c"setHandlingSendEvent:",
                set_handling_send_event as *const c_void,
                c"v@:c",
                sel_registerName,
                class_getInstanceMethod,
                class_addMethod,
            );
        }
    });
}

#[cfg(target_os = "macos")]
unsafe fn add_missing_macos_method(
    class: *mut std::ffi::c_void,
    selector_name: &'static std::ffi::CStr,
    implementation: *const std::ffi::c_void,
    type_encoding: &'static std::ffi::CStr,
    sel_register_name: unsafe extern "C" fn(*const std::os::raw::c_char) -> *mut std::ffi::c_void,
    class_get_instance_method: unsafe extern "C" fn(
        *mut std::ffi::c_void,
        *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void,
    class_add_method: unsafe extern "C" fn(
        *mut std::ffi::c_void,
        *mut std::ffi::c_void,
        *const std::ffi::c_void,
        *const std::os::raw::c_char,
    ) -> bool,
) {
    let selector = unsafe { sel_register_name(selector_name.as_ptr()) };
    if selector.is_null() || !unsafe { class_get_instance_method(class, selector) }.is_null() {
        return;
    }
    unsafe {
        class_add_method(class, selector, implementation, type_encoding.as_ptr());
    }
}

#[cfg(target_os = "macos")]
fn stage_macos_cef_sidecars_for_bare_run(paths: &MacCefPaths) -> Result<(), BrowserError> {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|current_exe| current_exe.parent().map(PathBuf::from))
    else {
        return Ok(());
    };
    if exe_dir.file_name().is_some_and(|name| name == "MacOS")
        && exe_dir.parent().is_some_and(|contents_dir| {
            contents_dir
                .file_name()
                .is_some_and(|name| name == "Contents")
        })
    {
        return Ok(());
    }

    let libraries_dir = paths.framework_dir.join("Libraries");
    for sidecar in [
        "libEGL.dylib",
        "libGLESv2.dylib",
        "libvk_swiftshader.dylib",
        "vk_swiftshader_icd.json",
    ] {
        let source = libraries_dir.join(sidecar);
        if !source.is_file() {
            return Err(backend_error(format!(
                "Chromium Embedded Framework sidecar is missing {}",
                source.display()
            )));
        }

        let dest = exe_dir.join(sidecar);
        if dest.is_file() {
            continue;
        }
        std::fs::copy(&source, &dest).map_err(|error| {
            backend_error(format!(
                "failed to stage Chromium Embedded Framework sidecar {} to {}: {error}",
                source.display(),
                dest.display()
            ))
        })?;
    }

    Ok(())
}
