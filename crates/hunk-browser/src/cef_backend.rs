use std::cell::RefCell;
use std::collections::BTreeMap;
#[cfg(target_os = "macos")]
use std::os::unix::ffi::OsStrExt;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use cef::{args::Args, *};

use crate::config::BrowserRuntimeConfig;
use crate::frame::BrowserFrame;
use crate::session::{BrowserAction, BrowserError, BrowserSession, BrowserSessionId};

const DEFAULT_WIDTH: i32 = 1024;
const DEFAULT_HEIGHT: i32 = 768;
const DEFAULT_URL: &str = "about:blank";

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
        let cef_paths = resolve_macos_cef_paths(config)?;
        #[cfg(target_os = "macos")]
        stage_macos_cef_sidecars_for_bare_run(&cef_paths)?;
        #[cfg(target_os = "macos")]
        let loader = load_macos_cef_framework(&cef_paths)?;

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
        let mut client = HunkCefClientBuilder::build(HunkCefClient {
            render_handler,
            load_handler,
        });

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

        self.browsers.insert(
            session_id,
            CefBrowserHandle {
                browser,
                _client: client,
            },
        );
        Ok(())
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
            | BrowserAction::Scroll { .. } => {
                return Err(backend_error(
                    "CEF input forwarding is not wired for this browser action yet",
                ));
            }
        }

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
        let changed = !events.frames.is_empty() || !events.loads.is_empty();
        for frame in events.frames {
            if let Some(session) = sessions.get_mut(&frame.session_id) {
                session.set_latest_frame(frame.frame);
            }
        }
        for load in events.loads {
            if let Some(session) = sessions.get_mut(&load.session_id) {
                match load.state {
                    CefLoadState::Started => session.set_loading(true),
                    CefLoadState::Ended => session.set_loading(false),
                    CefLoadState::Failed(error) => session.set_load_error(error),
                }
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
}

#[derive(Default)]
struct CefSharedState {
    next_frame_epoch: u64,
    frames: Vec<CefFrameEvent>,
    loads: Vec<CefLoadEvent>,
}

impl CefSharedState {
    fn push_frame(&mut self, session_id: BrowserSessionId, width: i32, height: i32, bgra: Vec<u8>) {
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

    fn drain(&mut self) -> CefSharedDrain {
        CefSharedDrain {
            frames: std::mem::take(&mut self.frames),
            loads: std::mem::take(&mut self.loads),
        }
    }
}

struct CefSharedDrain {
    frames: Vec<CefFrameEvent>,
    loads: Vec<CefLoadEvent>,
}

struct CefFrameEvent {
    session_id: BrowserSessionId,
    frame: BrowserFrame,
}

struct CefLoadEvent {
    session_id: BrowserSessionId,
    state: CefLoadState,
}

enum CefLoadState {
    Started,
    Ended,
    Failed(String),
}

#[derive(Clone)]
struct HunkCefApp {
    #[cfg(target_os = "macos")]
    cef_paths: MacCefPaths,
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
        }

        fn browser_process_handler(&self) -> Option<cef::BrowserProcessHandler> {
            Some(HunkCefBrowserProcessHandlerBuilder::build(
                HunkCefBrowserProcessHandler {
                    #[cfg(target_os = "macos")]
                    cef_paths: self.app.cef_paths.clone(),
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
                rect.width = DEFAULT_WIDTH;
                rect.height = DEFAULT_HEIGHT;
            }
        }

        fn screen_info(
            &self,
            _browser: Option<&mut Browser>,
            screen_info: Option<&mut ScreenInfo>,
        ) -> ::std::os::raw::c_int {
            if let Some(screen_info) = screen_info {
                screen_info.device_scale_factor = 1.0;
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
            _can_go_back: ::std::os::raw::c_int,
            _can_go_forward: ::std::os::raw::c_int,
        ) {
            let state = if is_loading == 1 {
                CefLoadState::Started
            } else {
                CefLoadState::Ended
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
struct HunkCefClient {
    render_handler: RenderHandler,
    load_handler: LoadHandler,
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
    }
}

impl HunkCefClientBuilder {
    fn build(client: HunkCefClient) -> Client {
        Self::new(client)
    }
}

fn backend_error(message: impl Into<String>) -> BrowserError {
    BrowserError::BackendUnavailable(message.into())
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
