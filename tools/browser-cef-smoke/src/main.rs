use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use cef::{args::Args, *};

const DEFAULT_URL: &str = "https://example.com";
const WIDTH: i32 = 1024;
const HEIGHT: i32 = 768;
const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Default)]
struct SmokeState {
    nonblank_frame: bool,
    frame_count: usize,
    last_frame_size: Option<(i32, i32)>,
    load_done: bool,
    load_error: Option<String>,
}

#[derive(Clone)]
struct SmokeApp;

wrap_app! {
    struct SmokeAppBuilder {
        app: SmokeApp,
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
        }

        fn browser_process_handler(&self) -> Option<cef::BrowserProcessHandler> {
            Some(SmokeBrowserProcessHandlerBuilder::build(
                SmokeBrowserProcessHandler,
            ))
        }
    }
}

impl SmokeAppBuilder {
    fn build(app: SmokeApp) -> cef::App {
        Self::new(app)
    }
}

#[derive(Clone)]
struct SmokeBrowserProcessHandler;

wrap_browser_process_handler! {
    struct SmokeBrowserProcessHandlerBuilder {
        handler: SmokeBrowserProcessHandler,
    }

    impl BrowserProcessHandler {
        fn on_before_child_process_launch(&self, command_line: Option<&mut CommandLine>) {
            let Some(command_line) = command_line else {
                return;
            };

            command_line.append_switch(Some(&"disable-web-security".into()));
            command_line.append_switch(Some(&"allow-running-insecure-content".into()));
            command_line.append_switch(Some(&"disable-session-crashed-bubble".into()));
            command_line.append_switch(Some(&"ignore-certificate-errors".into()));
            command_line.append_switch(Some(&"ignore-ssl-errors".into()));
            command_line.append_switch(Some(&"enable-logging=stderr".into()));
        }
    }
}

impl SmokeBrowserProcessHandlerBuilder {
    fn build(handler: SmokeBrowserProcessHandler) -> BrowserProcessHandler {
        Self::new(handler)
    }
}

#[derive(Clone)]
struct SmokeRenderHandler {
    state: Rc<RefCell<SmokeState>>,
}

wrap_render_handler! {
    struct SmokeRenderHandlerBuilder {
        handler: SmokeRenderHandler,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                rect.width = WIDTH;
                rect.height = HEIGHT;
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
            let pixels = unsafe { std::slice::from_raw_parts(buffer, buffer_len) };
            let nonblank = pixels.iter().any(|channel| *channel != 0);

            let mut state = self.handler.state.borrow_mut();
            state.frame_count += 1;
            state.last_frame_size = Some((width, height));
            state.nonblank_frame |= nonblank;
        }
    }
}

impl SmokeRenderHandlerBuilder {
    fn build(handler: SmokeRenderHandler) -> RenderHandler {
        Self::new(handler)
    }
}

#[derive(Clone)]
struct SmokeLoadHandler {
    state: Rc<RefCell<SmokeState>>,
}

wrap_load_handler! {
    struct SmokeLoadHandlerBuilder {
        handler: SmokeLoadHandler,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _http_status_code: ::std::os::raw::c_int,
        ) {
            self.handler.state.borrow_mut().load_done = true;
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            self.handler.state.borrow_mut().load_error = Some(format!(
                "load failed for {}: {:?} {}",
                failed_url.map(CefString::to_string).unwrap_or_default(),
                error_code,
                error_text.map(CefString::to_string).unwrap_or_default()
            ));
        }
    }
}

impl SmokeLoadHandlerBuilder {
    fn build(handler: SmokeLoadHandler) -> LoadHandler {
        Self::new(handler)
    }
}

#[derive(Clone)]
struct SmokeClient {
    render_handler: RenderHandler,
    load_handler: LoadHandler,
}

wrap_client! {
    struct SmokeClientBuilder {
        client: SmokeClient,
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

impl SmokeClientBuilder {
    fn build(client: SmokeClient) -> Client {
        Self::new(client)
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let is_helper_process =
        std::env::args().any(|arg| arg == "--type" || arg.starts_with("--type="));

    #[cfg(target_os = "macos")]
    let _loader = load_macos_cef_framework(is_helper_process)?;

    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = Args::new();
    let cmd = args
        .as_cmd_line()
        .ok_or_else(|| "failed to read CEF command line".to_string())?;
    let process_type_switch = CefString::from("type");
    let is_browser_process = cmd.has_switch(Some(&process_type_switch)) != 1;
    let mut app = SmokeAppBuilder::build(SmokeApp);

    let process_result = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if !is_browser_process {
        return if process_result >= 0 {
            Ok(())
        } else {
            Err("CEF subprocess dispatch failed".to_string())
        };
    }
    if process_result != -1 {
        return Err(format!(
            "unexpected CEF browser-process execute_process result: {process_result}"
        ));
    }

    let cache_root = cache_root_path()?;
    let subprocess_path = browser_subprocess_path()?;
    let settings = Settings {
        browser_subprocess_path: CefString::from(subprocess_path.to_string_lossy().as_ref()),
        root_cache_path: CefString::from(cache_root.to_string_lossy().as_ref()),
        cache_path: CefString::from(cache_root.join("profile").to_string_lossy().as_ref()),
        windowless_rendering_enabled: true as _,
        external_message_pump: true as _,
        no_sandbox: true as _,
        ..Default::default()
    };
    if initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    ) != 1
    {
        return Err("CEF initialize failed".to_string());
    }

    let result = run_browser();
    shutdown();
    result
}

fn run_browser() -> Result<(), String> {
    let state = Rc::new(RefCell::new(SmokeState::default()));
    let render_handler = SmokeRenderHandlerBuilder::build(SmokeRenderHandler {
        state: state.clone(),
    });
    let load_handler = SmokeLoadHandlerBuilder::build(SmokeLoadHandler {
        state: state.clone(),
    });
    let mut client = SmokeClientBuilder::build(SmokeClient {
        render_handler,
        load_handler,
    });

    let window_info = WindowInfo {
        windowless_rendering_enabled: true as _,
        ..Default::default()
    };
    let browser_settings = BrowserSettings {
        windowless_frame_rate: 30,
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
    .ok_or_else(|| "CEF browser creation failed".to_string())?;

    let start = Instant::now();
    while start.elapsed() < TIMEOUT {
        do_message_loop_work();
        if let Some(host) = browser.host() {
            host.send_external_begin_frame();
        }

        {
            let state = state.borrow();
            if state.nonblank_frame {
                println!(
                    "CEF smoke produced nonblank frame: {:?}, frames={}, load_done={}",
                    state.last_frame_size, state.frame_count, state.load_done
                );
                return Ok(());
            }
            if let Some(error) = state.load_error.as_ref() {
                return Err(error.clone());
            }
        }

        sleep(Duration::from_millis(16));
    }

    let state = state.borrow();
    Err(format!(
        "timed out waiting for nonblank CEF frame; frames={}, last_frame_size={:?}, load_done={}",
        state.frame_count, state.last_frame_size, state.load_done
    ))
}

fn cache_root_path() -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join("hunk-browser-cef-smoke-cache");
    std::fs::create_dir_all(&path).map_err(|error| {
        format!(
            "failed to create CEF smoke cache directory {}: {error}",
            path.display()
        )
    })?;
    std::fs::create_dir_all(path.join("profile")).map_err(|error| {
        format!(
            "failed to create CEF smoke profile directory {}: {error}",
            path.join("profile").display()
        )
    })?;
    path.canonicalize()
        .map_err(|error| format!("failed to canonicalize {}: {error}", path.display()))
}

fn browser_subprocess_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let app_contents = current_exe
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| format!("invalid app executable path: {}", current_exe.display()))?;
        let subprocess = app_contents
            .join("Frameworks")
            .join("hunk-browser-cef-smoke Helper.app")
            .join("Contents")
            .join("MacOS")
            .join("hunk-browser-cef-smoke Helper");
        if !subprocess.exists() {
            return Err(format!(
                "missing CEF helper executable: {}",
                subprocess.display()
            ));
        }
        Ok(subprocess)
    }

    #[cfg(not(target_os = "macos"))]
    {
        std::env::current_exe().map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "macos")]
fn load_macos_cef_framework(helper: bool) -> Result<cef::library_loader::LibraryLoader, String> {
    let loader = cef::library_loader::LibraryLoader::new(
        &std::env::current_exe().map_err(|error| error.to_string())?,
        helper,
    );
    if loader.load() {
        Ok(loader)
    } else {
        Err("failed to load Chromium Embedded Framework".to_string())
    }
}
