use std::cell::RefCell;
use std::collections::BTreeMap;
#[cfg(target_os = "linux")]
use std::ffi::OsStr;
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
    BrowserAction, BrowserConsoleLevel, BrowserContextMenuTarget, BrowserError,
    BrowserInputModifiers, BrowserMouseButton, BrowserMouseInput, BrowserSession, BrowserSessionId,
    BrowserTabId, BrowserViewportSize,
};
use crate::snapshot::{BrowserPhysicalPoint, BrowserSnapshot};

const DEFAULT_URL: &str = "about:blank";
const DEVTOOLS_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(2);
const NEW_BROWSER_WARMUP_PUMP_ITERATIONS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CefBrowserKey {
    session_id: BrowserSessionId,
    tab_id: BrowserTabId,
}

impl CefBrowserKey {
    pub(crate) fn new(session_id: BrowserSessionId, tab_id: BrowserTabId) -> Self {
        Self { session_id, tab_id }
    }
}

pub(crate) struct CefBrowserBackend {
    _app: cef::App,
    #[cfg(target_os = "macos")]
    _loader: MacCefLoader,
    browsers: BTreeMap<CefBrowserKey, CefBrowserHandle>,
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

    pub(crate) fn ensure_tab(
        &mut self,
        session_id: BrowserSessionId,
        tab_id: BrowserTabId,
    ) -> Result<(), BrowserError> {
        let key = CefBrowserKey::new(session_id, tab_id);
        if self.browsers.contains_key(&key) {
            return Ok(());
        }

        let render_handler = HunkCefRenderHandlerBuilder::build(HunkCefRenderHandler {
            key: key.clone(),
            shared: self.shared.clone(),
        });
        let load_handler = HunkCefLoadHandlerBuilder::build(HunkCefLoadHandler {
            key: key.clone(),
            shared: self.shared.clone(),
        });
        let display_handler = HunkCefDisplayHandlerBuilder::build(HunkCefDisplayHandler {
            key: key.clone(),
            shared: self.shared.clone(),
        });
        let context_menu_handler =
            HunkCefContextMenuHandlerBuilder::build(HunkCefContextMenuHandler {
                key: key.clone(),
                shared: self.shared.clone(),
            });
        let life_span_handler = HunkCefLifeSpanHandlerBuilder::build(HunkCefLifeSpanHandler {
            key: key.clone(),
            shared: self.shared.clone(),
        });
        let mut devtools_observer =
            HunkCefDevToolsMessageObserverBuilder::build(HunkCefDevToolsMessageObserver {
                shared: self.shared.clone(),
            });
        let mut client = HunkCefClientBuilder::build(HunkCefClient {
            render_handler,
            load_handler,
            display_handler,
            context_menu_handler,
            life_span_handler,
        });
        self.shared
            .borrow_mut()
            .set_viewport(key.clone(), BrowserViewportSize::default());

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

        self.browsers.insert(key, handle);
        Ok(())
    }

    pub(crate) fn close_tab(&mut self, session_id: &BrowserSessionId, tab_id: &BrowserTabId) {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        if let Some(handle) = self.browsers.remove(&key)
            && let Some(host) = handle.browser.host()
        {
            host.close_browser(true as _);
        }
        self.shared.borrow_mut().remove_tab(&key);
    }

    pub(crate) fn capture_snapshot(
        &mut self,
        session_id: &BrowserSessionId,
        tab_id: &BrowserTabId,
        epoch: u64,
    ) -> Result<BrowserSnapshot, BrowserError> {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        let handle = self
            .browsers
            .get(&key)
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
        tab_id: &BrowserTabId,
        action: &BrowserAction,
    ) -> Result<(), BrowserError> {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        let handle = self
            .browsers
            .get(&key)
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
        tab_id: &BrowserTabId,
        viewport: BrowserViewportSize,
    ) -> Result<(), BrowserError> {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        let handle = self
            .browsers
            .get(&key)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;
        self.shared.borrow_mut().set_viewport(key, viewport);
        if let Some(host) = handle.browser.host() {
            host.was_resized();
            host.send_external_begin_frame();
        }
        Ok(())
    }

    pub(crate) fn focus_session(
        &mut self,
        session_id: &BrowserSessionId,
        tab_id: &BrowserTabId,
        focused: bool,
    ) -> Result<(), BrowserError> {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        let handle = self
            .browsers
            .get(&key)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;
        if let Some(host) = handle.browser.host() {
            host.set_focus(focused as _);
        }
        Ok(())
    }

    pub(crate) fn send_mouse_move(
        &mut self,
        session_id: &BrowserSessionId,
        tab_id: &BrowserTabId,
        input: BrowserMouseInput,
    ) -> Result<(), BrowserError> {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        let handle = self
            .browsers
            .get(&key)
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
        tab_id: &BrowserTabId,
        input: BrowserMouseInput,
        button: BrowserMouseButton,
    ) -> Result<(), BrowserError> {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        let handle = self
            .browsers
            .get(&key)
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
        tab_id: &BrowserTabId,
        input: BrowserMouseInput,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<(), BrowserError> {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        let handle = self
            .browsers
            .get(&key)
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
        tab_id: &BrowserTabId,
        text: &str,
    ) -> Result<(), BrowserError> {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        let handle = self
            .browsers
            .get(&key)
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
        tab_id: &BrowserTabId,
        keys: &str,
    ) -> Result<(), BrowserError> {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        let handle = self
            .browsers
            .get(&key)
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
        self.shared.borrow_mut().set_active_tabs(
            sessions
                .iter()
                .map(|(session_id, session)| (session_id.clone(), session.active_tab_id().clone())),
        );
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
            || !events.viewports.is_empty()
            || !events.context_menus.is_empty()
            || !events.popups.is_empty();
        for frame in events.frames {
            if let Some(session) = sessions.get_mut(&frame.key.session_id) {
                session.set_latest_frame_for_tab(&frame.key.tab_id, frame.frame);
            }
        }
        for load in events.loads {
            if let Some(session) = sessions.get_mut(&load.key.session_id) {
                match load.state {
                    CefLoadState::Changed {
                        loading,
                        can_go_back,
                        can_go_forward,
                        url,
                    } => {
                        session.apply_backend_loading_state_for_tab(
                            &load.key.tab_id,
                            loading,
                            can_go_back,
                            can_go_forward,
                            url,
                        );
                    }
                    CefLoadState::UrlChanged(url) => session.set_url_for_tab(&load.key.tab_id, url),
                    CefLoadState::TitleChanged(title) => {
                        session.set_title_for_tab(&load.key.tab_id, title);
                    }
                    CefLoadState::Failed(error) => {
                        session.set_load_error_for_tab(&load.key.tab_id, error);
                    }
                }
            }
        }
        for entry in events.console_entries {
            if let Some(session) = sessions.get_mut(&entry.key.session_id) {
                session.push_console_entry_for_tab(
                    entry.key.tab_id,
                    entry.level,
                    entry.message,
                    entry.source,
                    entry.line,
                    entry.timestamp_ms,
                );
            }
        }
        for (key, viewport) in events.viewports {
            if let Some(session) = sessions.get_mut(&key.session_id) {
                session.set_viewport_for_tab(&key.tab_id, viewport);
            }
        }
        for popup in events.popups {
            let Some(session) = sessions.get_mut(&popup.key.session_id) else {
                continue;
            };
            let _popup_metadata = (&popup.target_frame_name, popup.user_gesture);
            let activate = popup.target_disposition != WindowOpenDisposition::NEW_BACKGROUND_TAB;
            let tab_id = session.create_tab(Some(popup.target_url.clone()), activate);
            self.ensure_tab(popup.key.session_id.clone(), tab_id.clone())?;
            self.apply_action(
                &popup.key.session_id,
                &tab_id,
                &BrowserAction::Navigate {
                    url: popup.target_url,
                },
            )?;
        }

        Ok(changed)
    }

    pub(crate) fn take_context_menu_target(
        &mut self,
        session_id: &BrowserSessionId,
        tab_id: &BrowserTabId,
    ) -> Option<BrowserContextMenuTarget> {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        self.shared.borrow_mut().take_context_menu_target(&key)
    }

    pub(crate) fn show_devtools(
        &mut self,
        session_id: &BrowserSessionId,
        tab_id: &BrowserTabId,
        inspect_element_at: Option<BrowserPhysicalPoint>,
    ) -> Result<(), BrowserError> {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        let handle = self
            .browsers
            .get_mut(&key)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;
        let host = handle
            .browser
            .host()
            .ok_or_else(|| backend_error("CEF browser host unavailable"))?;
        let window_info = WindowInfo::default();
        let settings = BrowserSettings::default();
        let inspect_point = inspect_element_at.map(|point| Point {
            x: point.x,
            y: point.y,
        });
        host.show_dev_tools(
            Some(&window_info),
            None,
            Some(&settings),
            inspect_point.as_ref(),
        );
        Ok(())
    }

    pub(crate) fn close_devtools(
        &mut self,
        session_id: &BrowserSessionId,
        tab_id: &BrowserTabId,
    ) -> Result<(), BrowserError> {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        let handle = self
            .browsers
            .get_mut(&key)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;
        let host = handle
            .browser
            .host()
            .ok_or_else(|| backend_error("CEF browser host unavailable"))?;
        host.close_dev_tools();
        Ok(())
    }

    pub(crate) fn has_devtools(
        &mut self,
        session_id: &BrowserSessionId,
        tab_id: &BrowserTabId,
    ) -> Result<bool, BrowserError> {
        let key = CefBrowserKey::new(session_id.clone(), tab_id.clone());
        let handle = self
            .browsers
            .get_mut(&key)
            .ok_or_else(|| BrowserError::MissingSession(session_id.as_str().to_string()))?;
        let host = handle
            .browser
            .host()
            .ok_or_else(|| backend_error("CEF browser host unavailable"))?;
        Ok(host.has_dev_tools() == 1)
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
    frame_limiters: BTreeMap<CefBrowserKey, BrowserFrameRateLimiter>,
    active_tabs: BTreeMap<BrowserSessionId, BrowserTabId>,
    frames: Vec<CefFrameEvent>,
    loads: Vec<CefLoadEvent>,
    console_entries: Vec<CefConsoleEvent>,
    viewports: BTreeMap<CefBrowserKey, BrowserViewportSize>,
    viewport_events: Vec<(CefBrowserKey, BrowserViewportSize)>,
    context_menu_targets: BTreeMap<CefBrowserKey, BrowserContextMenuTarget>,
    context_menus: Vec<CefBrowserKey>,
    popups: Vec<CefPopupEvent>,
    devtools_results: BTreeMap<i32, CefDevToolsResult>,
}

impl CefSharedState {
    fn push_frame(&mut self, key: CefBrowserKey, width: i32, height: i32, bgra: Vec<u8>) {
        if self
            .active_tabs
            .get(&key.session_id)
            .is_some_and(|active_tab_id| active_tab_id != &key.tab_id)
        {
            return;
        }
        let now = Instant::now();
        if !self
            .frame_limiters
            .entry(key.clone())
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
        self.frames.push(CefFrameEvent { key, frame });
    }

    fn push_load(&mut self, key: CefBrowserKey, state: CefLoadState) {
        self.loads.push(CefLoadEvent { key, state });
    }

    fn push_console_entry(
        &mut self,
        key: CefBrowserKey,
        level: BrowserConsoleLevel,
        message: String,
        source: Option<String>,
        line: Option<u32>,
    ) {
        self.console_entries.push(CefConsoleEvent {
            key,
            level,
            message,
            source,
            line,
            timestamp_ms: browser_console_timestamp_ms(),
        });
    }

    fn set_viewport(&mut self, key: CefBrowserKey, viewport: BrowserViewportSize) {
        self.viewports.insert(key.clone(), viewport);
        self.viewport_events.push((key, viewport));
    }

    fn push_context_menu_target(&mut self, key: CefBrowserKey, target: BrowserContextMenuTarget) {
        self.context_menu_targets.insert(key.clone(), target);
        self.context_menus.push(key);
    }

    fn push_popup(&mut self, event: CefPopupEvent) {
        self.popups.push(event);
    }

    fn set_active_tabs(
        &mut self,
        active_tabs: impl IntoIterator<Item = (BrowserSessionId, BrowserTabId)>,
    ) {
        self.active_tabs = active_tabs.into_iter().collect();
    }

    fn take_context_menu_target(
        &mut self,
        key: &CefBrowserKey,
    ) -> Option<BrowserContextMenuTarget> {
        self.context_menu_targets.remove(key)
    }

    fn remove_tab(&mut self, key: &CefBrowserKey) {
        self.frame_limiters.remove(key);
        self.viewports.remove(key);
        self.frames.retain(|event| &event.key != key);
        self.loads.retain(|event| &event.key != key);
        self.console_entries.retain(|event| &event.key != key);
        self.viewport_events
            .retain(|(event_key, _)| event_key != key);
        self.context_menu_targets.remove(key);
        self.context_menus.retain(|event_key| event_key != key);
        self.popups.retain(|event| &event.key != key);
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

    fn viewport(&self, key: &CefBrowserKey) -> BrowserViewportSize {
        self.viewports.get(key).copied().unwrap_or_default()
    }

    fn drain(&mut self) -> CefSharedDrain {
        CefSharedDrain {
            frames: std::mem::take(&mut self.frames),
            loads: std::mem::take(&mut self.loads),
            console_entries: std::mem::take(&mut self.console_entries),
            viewports: std::mem::take(&mut self.viewport_events),
            context_menus: std::mem::take(&mut self.context_menus),
            popups: std::mem::take(&mut self.popups),
        }
    }
}

struct CefSharedDrain {
    frames: Vec<CefFrameEvent>,
    loads: Vec<CefLoadEvent>,
    console_entries: Vec<CefConsoleEvent>,
    viewports: Vec<(CefBrowserKey, BrowserViewportSize)>,
    context_menus: Vec<CefBrowserKey>,
    popups: Vec<CefPopupEvent>,
}

struct CefFrameEvent {
    key: CefBrowserKey,
    frame: BrowserFrame,
}

struct CefLoadEvent {
    key: CefBrowserKey,
    state: CefLoadState,
}

struct CefConsoleEvent {
    key: CefBrowserKey,
    level: BrowserConsoleLevel,
    message: String,
    source: Option<String>,
    line: Option<u32>,
    timestamp_ms: u64,
}

struct CefPopupEvent {
    key: CefBrowserKey,
    target_url: String,
    target_frame_name: Option<String>,
    target_disposition: WindowOpenDisposition,
    user_gesture: bool,
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
        url: Option<String>,
    },
    UrlChanged(String),
    TitleChanged(String),
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
            command_line.append_switch_with_value(
                Some(&"autoplay-policy".into()),
                Some(&CefString::from("no-user-gesture-required")),
            );
            #[cfg(target_os = "macos")]
            append_macos_cef_switches(command_line, &self.app.cef_paths);
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            append_flat_cef_switches(command_line, &self.app.cef_runtime_dir);
            #[cfg(target_os = "linux")]
            append_linux_cef_compositor_switches(command_line);
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
            #[cfg(target_os = "linux")]
            append_linux_cef_compositor_switches(command_line);
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
    key: CefBrowserKey,
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
                    .viewport(&self.handler.key);
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
                    .viewport(&self.handler.key)
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
                self.handler.key.clone(),
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
    key: CefBrowserKey,
    shared: Rc<RefCell<CefSharedState>>,
}

wrap_load_handler! {
    struct HunkCefLoadHandlerBuilder {
        handler: HunkCefLoadHandler,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            let url = browser
                .and_then(|browser| browser.main_frame())
                .map(|frame| CefString::from(&frame.url()).to_string())
                .filter(|url| !url.is_empty());
            let state = CefLoadState::Changed {
                loading: is_loading == 1,
                can_go_back: can_go_back == 1,
                can_go_forward: can_go_forward == 1,
                url,
            };
            self.handler
                .shared
                .borrow_mut()
                .push_load(self.handler.key.clone(), state);
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
                self.handler.key.clone(),
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
    life_span_handler: LifeSpanHandler,
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

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.client.life_span_handler.clone())
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
    key: CefBrowserKey,
    shared: Rc<RefCell<CefSharedState>>,
}

wrap_display_handler! {
    struct HunkCefDisplayHandlerBuilder {
        handler: HunkCefDisplayHandler,
    }

    impl DisplayHandler {
        fn on_address_change(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() != 1 {
                return;
            }
            let Some(url) = url.map(CefString::to_string).filter(|url| !url.is_empty()) else {
                return;
            };
            self.handler
                .shared
                .borrow_mut()
                .push_load(self.handler.key.clone(), CefLoadState::UrlChanged(url));
        }

        fn on_title_change(
            &self,
            _browser: Option<&mut Browser>,
            title: Option<&CefString>,
        ) {
            let Some(title) = title.map(CefString::to_string).filter(|title| !title.is_empty()) else {
                return;
            };
            self.handler
                .shared
                .borrow_mut()
                .push_load(self.handler.key.clone(), CefLoadState::TitleChanged(title));
        }

        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            self.handler.shared.borrow_mut().push_console_entry(
                self.handler.key.clone(),
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
struct HunkCefLifeSpanHandler {
    key: CefBrowserKey,
    shared: Rc<RefCell<CefSharedState>>,
}

wrap_life_span_handler! {
    struct HunkCefLifeSpanHandlerBuilder {
        handler: HunkCefLifeSpanHandler,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            if let Some(target_url) = cef_ref_optional_string(target_url) {
                self.handler.shared.borrow_mut().push_popup(CefPopupEvent {
                    key: self.handler.key.clone(),
                    target_url,
                    target_frame_name: cef_ref_optional_string(target_frame_name),
                    target_disposition,
                    user_gesture: user_gesture == 1,
                });
            }
            true as _
        }
    }
}

impl HunkCefLifeSpanHandlerBuilder {
    fn build(handler: HunkCefLifeSpanHandler) -> LifeSpanHandler {
        Self::new(handler)
    }
}

#[derive(Clone)]
struct HunkCefContextMenuHandler {
    key: CefBrowserKey,
    shared: Rc<RefCell<CefSharedState>>,
}

wrap_context_menu_handler! {
    struct HunkCefContextMenuHandlerBuilder {
        handler: HunkCefContextMenuHandler,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            if let Some(params) = params {
                let target = BrowserContextMenuTarget {
                    tab_id: self.handler.key.tab_id.clone(),
                    x: params.xcoord(),
                    y: params.ycoord(),
                    page_url: cef_optional_string(params.page_url()),
                    frame_url: cef_optional_string(params.frame_url()),
                    link_url: cef_optional_string(params.link_url())
                        .or_else(|| cef_optional_string(params.unfiltered_link_url())),
                    source_url: cef_optional_string(params.source_url()),
                    selection_text: cef_optional_string(params.selection_text()),
                    title_text: cef_optional_string(params.title_text()),
                    media_type: browser_context_media_type(params.media_type()),
                    editable: params.is_editable() == 1,
                };
                self.handler
                    .shared
                    .borrow_mut()
                    .push_context_menu_target(self.handler.key.clone(), target);
            }
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

fn cef_optional_string(value: CefStringUserfreeUtf16) -> Option<String> {
    let value = CefString::from(&value).to_string();
    (!value.trim().is_empty()).then_some(value)
}

fn cef_ref_optional_string(value: Option<&CefString>) -> Option<String> {
    value
        .map(CefString::to_string)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn browser_context_media_type(media_type: ContextMenuMediaType) -> Option<String> {
    if media_type == ContextMenuMediaType::IMAGE {
        Some("image".to_string())
    } else if media_type == ContextMenuMediaType::VIDEO {
        Some("video".to_string())
    } else if media_type == ContextMenuMediaType::AUDIO {
        Some("audio".to_string())
    } else if media_type == ContextMenuMediaType::CANVAS {
        Some("canvas".to_string())
    } else if media_type == ContextMenuMediaType::FILE {
        Some("file".to_string())
    } else if media_type == ContextMenuMediaType::PLUGIN {
        Some("plugin".to_string())
    } else {
        None
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
    #[cfg(target_os = "linux")]
    use super::linux_cef_ozone_platform;
    #[cfg(target_os = "linux")]
    use std::ffi::OsStr;

    #[test]
    fn snapshot_expression_redacts_sensitive_values_and_uses_viewport_rects() {
        let expression = browser_snapshot_expression(1);

        assert!(expression.contains("isSensitiveValueElement(element) ? ''"));
        assert!(!expression.contains("x: rect.x + window.scrollX"));
        assert!(!expression.contains("y: rect.y + window.scrollY"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_cef_ozone_platform_uses_x11_when_wayland_is_empty() {
        assert_eq!(
            linux_cef_ozone_platform(Some(OsStr::new("")), Some(OsStr::new(":0"))),
            Some("x11")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_cef_ozone_platform_prefers_wayland_when_available() {
        assert_eq!(
            linux_cef_ozone_platform(Some(OsStr::new("wayland-0")), Some(OsStr::new(":0"))),
            Some("wayland")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_cef_ozone_platform_is_unset_without_display() {
        assert_eq!(linux_cef_ozone_platform(None, None), None);
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

#[cfg(target_os = "linux")]
fn append_linux_cef_compositor_switches(command_line: &mut CommandLine) {
    let ozone_platform = linux_cef_ozone_platform(
        std::env::var_os("WAYLAND_DISPLAY").as_deref(),
        std::env::var_os("DISPLAY").as_deref(),
    );

    if let Some(ozone_platform) = ozone_platform {
        command_line.append_switch_with_value(
            Some(&"ozone-platform".into()),
            Some(&CefString::from(ozone_platform)),
        );
    }
}

#[cfg(target_os = "linux")]
fn linux_cef_ozone_platform(
    wayland_display: Option<&OsStr>,
    x11_display: Option<&OsStr>,
) -> Option<&'static str> {
    if os_str_is_non_empty(wayland_display) {
        Some("wayland")
    } else if os_str_is_non_empty(x11_display) {
        Some("x11")
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn os_str_is_non_empty(value: Option<&OsStr>) -> bool {
    value
        .map(|value| !value.to_string_lossy().is_empty())
        .unwrap_or(false)
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
