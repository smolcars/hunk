use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::frame::MobileFrame;
use crate::session::{MobileDeviceId, MobileError, MobileSession, MobileSessionId};
use crate::snapshot::{MobileElement, MobileElementRect, MobileSnapshot, MobileViewport};

const UI_AUTOMATOR_DUMP_PATH: &str = "/sdcard/hunk-window.xml";
const DEFAULT_BOOT_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidRuntimeConfig {
    pub tools: AndroidToolPaths,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidToolPaths {
    pub adb: PathBuf,
    pub emulator: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidDeviceInventory {
    pub tools: AndroidToolsStatus,
    pub devices: Vec<AndroidDeviceSummary>,
    pub avds: Vec<AndroidAvdSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidToolsStatus {
    pub adb: AndroidToolStatus,
    pub emulator: AndroidToolStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidToolStatus {
    pub path: Option<PathBuf>,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidDeviceSummary {
    pub serial: MobileDeviceId,
    pub state: String,
    pub is_emulator: bool,
    pub details: BTreeMap<String, String>,
}

impl AndroidDeviceSummary {
    pub fn is_online_emulator(&self) -> bool {
        self.is_emulator && self.state == "device"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidAvdSummary {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AndroidAction {
    Tap {
        target: AndroidTapTarget,
    },
    Type {
        snapshot_epoch: Option<u64>,
        index: Option<u32>,
        text: String,
        clear: bool,
    },
    Press {
        key: AndroidKey,
    },
    Swipe {
        start_x: i32,
        start_y: i32,
        end_x: i32,
        end_y: i32,
        duration_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AndroidTapTarget {
    Element { snapshot_epoch: u64, index: u32 },
    Point { x: i32, y: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AndroidKey {
    Back,
    Home,
    Enter,
    Tab,
    Escape,
    Delete,
    RecentApps,
    Power,
    VolumeUp,
    VolumeDown,
    Raw(String),
}

impl AndroidKey {
    pub fn keyevent_arg(&self) -> Result<String, MobileError> {
        let value = match self {
            AndroidKey::Back => "4",
            AndroidKey::Home => "3",
            AndroidKey::Enter => "66",
            AndroidKey::Tab => "61",
            AndroidKey::Escape => "111",
            AndroidKey::Delete => "67",
            AndroidKey::RecentApps => "187",
            AndroidKey::Power => "26",
            AndroidKey::VolumeUp => "24",
            AndroidKey::VolumeDown => "25",
            AndroidKey::Raw(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty()
                    || trimmed
                        .chars()
                        .any(|value| !(value.is_ascii_alphanumeric() || value == '_'))
                {
                    return Err(MobileError::InvalidKey(value.clone()));
                }
                trimmed
            }
        };
        Ok(value.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct AndroidRuntime {
    config: Option<AndroidRuntimeConfig>,
    sessions: BTreeMap<MobileSessionId, MobileSession>,
}

impl Default for AndroidRuntime {
    fn default() -> Self {
        Self {
            config: find_android_tools()
                .ok()
                .map(|tools| AndroidRuntimeConfig { tools }),
            sessions: BTreeMap::new(),
        }
    }
}

impl AndroidRuntime {
    pub fn new_auto() -> Self {
        Self::default()
    }

    pub fn new_configured(config: AndroidRuntimeConfig) -> Self {
        Self {
            config: Some(config),
            sessions: BTreeMap::new(),
        }
    }

    pub fn config(&self) -> Option<&AndroidRuntimeConfig> {
        self.config.as_ref()
    }

    pub fn ensure_session(&mut self, thread_id: impl Into<String>) -> &mut MobileSession {
        let session_id = MobileSessionId::new(thread_id);
        self.sessions
            .entry(session_id.clone())
            .or_insert_with(|| MobileSession::new(session_id))
    }

    pub fn select_device(&mut self, thread_id: &str, device_id: MobileDeviceId) {
        self.ensure_session(thread_id.to_string())
            .select_device(device_id);
    }

    pub fn devices(&self) -> Result<AndroidDeviceInventory, MobileError> {
        let tools = self.require_tools()?;
        let devices_output = run_command(&tools.adb, ["devices", "-l"])?;
        let devices = parse_adb_devices(devices_output.stdout.as_str());
        let avds = if let Some(emulator) = tools.emulator.as_ref() {
            let output = run_command(emulator, ["-list-avds"])?;
            parse_avd_list(output.stdout.as_str())
        } else {
            Vec::new()
        };

        Ok(AndroidDeviceInventory {
            tools: AndroidToolsStatus {
                adb: AndroidToolStatus {
                    path: Some(tools.adb.clone()),
                    available: true,
                },
                emulator: AndroidToolStatus {
                    path: tools.emulator.clone(),
                    available: tools.emulator.is_some(),
                },
            },
            devices,
            avds,
        })
    }

    pub fn start_avd(
        &mut self,
        avd_name: &str,
        wait_for_boot: bool,
        timeout: Option<Duration>,
    ) -> Result<AndroidDeviceInventory, MobileError> {
        let tools = self.require_tools()?;
        let emulator = tools
            .emulator
            .as_ref()
            .ok_or_else(|| MobileError::MissingAndroidTool {
                tool: "emulator".to_string(),
            })?;
        Command::new(emulator)
            .args(["-avd", avd_name])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| MobileError::AndroidToolFailed {
                tool: emulator.display().to_string(),
                message: error.to_string(),
            })?;

        if wait_for_boot {
            self.wait_for_any_emulator_boot(timeout.unwrap_or(DEFAULT_BOOT_TIMEOUT))?;
        }

        self.devices()
    }

    pub fn install_apk(
        &mut self,
        thread_id: &str,
        device_id: Option<&MobileDeviceId>,
        apk_path: &Path,
    ) -> Result<String, MobileError> {
        let tools = self.require_tools()?;
        let serial = self.resolve_device(thread_id, device_id)?;
        let apk_path = apk_path.to_string_lossy().to_string();
        let output = run_command(
            &tools.adb,
            vec![
                "-s".to_string(),
                serial.as_str().to_string(),
                "install".to_string(),
                "-r".to_string(),
                apk_path,
            ],
        )?;
        Ok(join_command_output(output))
    }

    pub fn launch_package(
        &mut self,
        thread_id: &str,
        device_id: Option<&MobileDeviceId>,
        package: &str,
        activity: Option<&str>,
    ) -> Result<String, MobileError> {
        let tools = self.require_tools()?;
        let serial = self.resolve_device(thread_id, device_id)?;
        let output = if let Some(activity) = activity {
            let component = format!("{package}/{activity}");
            run_command(
                &tools.adb,
                vec![
                    "-s".to_string(),
                    serial.as_str().to_string(),
                    "shell".to_string(),
                    "am".to_string(),
                    "start".to_string(),
                    "-n".to_string(),
                    component,
                ],
            )?
        } else {
            run_command(
                &tools.adb,
                vec![
                    "-s".to_string(),
                    serial.as_str().to_string(),
                    "shell".to_string(),
                    "monkey".to_string(),
                    "-p".to_string(),
                    package.to_string(),
                    "1".to_string(),
                ],
            )?
        };
        Ok(join_command_output(output))
    }

    pub fn capture_snapshot(
        &mut self,
        thread_id: &str,
        device_id: Option<&MobileDeviceId>,
    ) -> Result<&MobileSnapshot, MobileError> {
        let tools = self.require_tools()?.clone();
        let serial = self.resolve_device(thread_id, device_id)?;
        run_command(
            &tools.adb,
            [
                "-s",
                serial.as_str(),
                "shell",
                "uiautomator",
                "dump",
                UI_AUTOMATOR_DUMP_PATH,
            ],
        )?;
        let output = run_command(
            &tools.adb,
            [
                "-s",
                serial.as_str(),
                "exec-out",
                "cat",
                UI_AUTOMATOR_DUMP_PATH,
            ],
        )?;

        let epoch = self
            .ensure_session(thread_id.to_string())
            .latest_snapshot()
            .epoch
            .saturating_add(1);
        let mut snapshot = parse_ui_automator_snapshot(output.stdout.as_str(), epoch)?;
        snapshot.device_id = Some(serial.as_str().to_string());
        let session = self.ensure_session(thread_id.to_string());
        session.select_device(serial);
        session.replace_snapshot(snapshot);
        Ok(session.latest_snapshot())
    }

    pub fn capture_screenshot(
        &mut self,
        thread_id: &str,
        device_id: Option<&MobileDeviceId>,
    ) -> Result<&MobileFrame, MobileError> {
        let tools = self.require_tools()?.clone();
        let serial = self.resolve_device(thread_id, device_id)?;
        let output = run_command_bytes(
            &tools.adb,
            ["-s", serial.as_str(), "exec-out", "screencap", "-p"],
        )?;
        let frame_epoch = self
            .ensure_session(thread_id.to_string())
            .latest_frame()
            .map(|frame| frame.metadata().frame_epoch.saturating_add(1))
            .unwrap_or(1);
        let frame = MobileFrame::from_png(output.stdout, frame_epoch, None)
            .map_err(|error| MobileError::Screenshot(error.to_string()))?;
        let session = self.ensure_session(thread_id.to_string());
        session.select_device(serial);
        session.set_latest_frame(frame);
        session
            .latest_frame()
            .ok_or_else(|| MobileError::Screenshot("screenshot frame was not stored".to_string()))
    }

    pub fn apply_action(
        &mut self,
        thread_id: &str,
        device_id: Option<&MobileDeviceId>,
        action: &AndroidAction,
    ) -> Result<(), MobileError> {
        let tools = self.require_tools()?.clone();
        let serial = self.resolve_device(thread_id, device_id)?;
        let session = self.ensure_session(thread_id.to_string());
        let argv = match action {
            AndroidAction::Tap { target } => {
                let point = match target {
                    AndroidTapTarget::Element {
                        snapshot_epoch,
                        index,
                    } => session
                        .validate_snapshot_element(*snapshot_epoch, *index)?
                        .rect
                        .center(),
                    AndroidTapTarget::Point { x, y } => {
                        crate::snapshot::MobilePoint { x: *x, y: *y }
                    }
                };
                vec![
                    "-s".to_string(),
                    serial.as_str().to_string(),
                    "shell".to_string(),
                    "input".to_string(),
                    "tap".to_string(),
                    point.x.to_string(),
                    point.y.to_string(),
                ]
            }
            AndroidAction::Type {
                snapshot_epoch,
                index,
                text,
                ..
            } => {
                if let (Some(snapshot_epoch), Some(index)) = (snapshot_epoch, index) {
                    let point = session
                        .validate_snapshot_element(*snapshot_epoch, *index)?
                        .rect
                        .center();
                    run_command(
                        &tools.adb,
                        vec![
                            "-s".to_string(),
                            serial.as_str().to_string(),
                            "shell".to_string(),
                            "input".to_string(),
                            "tap".to_string(),
                            point.x.to_string(),
                            point.y.to_string(),
                        ],
                    )?;
                }
                vec![
                    "-s".to_string(),
                    serial.as_str().to_string(),
                    "shell".to_string(),
                    "input".to_string(),
                    "text".to_string(),
                    parse_android_input_text(text)?.encoded,
                ]
            }
            AndroidAction::Press { key } => vec![
                "-s".to_string(),
                serial.as_str().to_string(),
                "shell".to_string(),
                "input".to_string(),
                "keyevent".to_string(),
                key.keyevent_arg()?,
            ],
            AndroidAction::Swipe {
                start_x,
                start_y,
                end_x,
                end_y,
                duration_ms,
            } => vec![
                "-s".to_string(),
                serial.as_str().to_string(),
                "shell".to_string(),
                "input".to_string(),
                "swipe".to_string(),
                start_x.to_string(),
                start_y.to_string(),
                end_x.to_string(),
                end_y.to_string(),
                duration_ms.to_string(),
            ],
        };

        run_command(&tools.adb, argv)?;
        Ok(())
    }

    pub fn logcat(
        &mut self,
        thread_id: &str,
        device_id: Option<&MobileDeviceId>,
        max_lines: usize,
    ) -> Result<Vec<String>, MobileError> {
        let tools = self.require_tools()?;
        let serial = self.resolve_device(thread_id, device_id)?;
        let output = run_command(&tools.adb, ["-s", serial.as_str(), "logcat", "-d"])?;
        let lines = output
            .stdout
            .lines()
            .rev()
            .take(max_lines.max(1))
            .map(str::to_string)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        Ok(lines)
    }

    fn require_tools(&self) -> Result<&AndroidToolPaths, MobileError> {
        self.config
            .as_ref()
            .map(|config| &config.tools)
            .ok_or_else(|| MobileError::MissingAndroidTool {
                tool: "adb".to_string(),
            })
    }

    fn resolve_device(
        &mut self,
        thread_id: &str,
        requested: Option<&MobileDeviceId>,
    ) -> Result<MobileDeviceId, MobileError> {
        if let Some(device_id) = requested {
            return Ok(device_id.clone());
        }
        if let Some(device_id) = self
            .ensure_session(thread_id.to_string())
            .selected_device_id()
            .cloned()
        {
            return Ok(device_id);
        }
        let device = self
            .devices()?
            .devices
            .into_iter()
            .find(AndroidDeviceSummary::is_online_emulator)
            .ok_or(MobileError::NoRunningEmulator)?;
        let device_id = device.serial;
        self.ensure_session(thread_id.to_string())
            .select_device(device_id.clone());
        Ok(device_id)
    }

    fn wait_for_any_emulator_boot(&mut self, timeout: Duration) -> Result<(), MobileError> {
        let started_at = Instant::now();
        while started_at.elapsed() < timeout {
            let Some(device) = self
                .devices()?
                .devices
                .into_iter()
                .find(AndroidDeviceSummary::is_online_emulator)
            else {
                thread::sleep(Duration::from_millis(500));
                continue;
            };
            let tools = self.require_tools()?;
            let output = run_command(
                &tools.adb,
                [
                    "-s",
                    device.serial.as_str(),
                    "shell",
                    "getprop",
                    "sys.boot_completed",
                ],
            )?;
            if output.stdout.trim() == "1" {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(500));
        }
        Err(MobileError::AndroidToolFailed {
            tool: "emulator".to_string(),
            message: format!(
                "emulator did not finish booting within {}s",
                timeout.as_secs()
            ),
        })
    }
}

pub fn find_android_tools() -> Result<AndroidToolPaths, MobileError> {
    let sdk_roots = android_sdk_roots();
    let adb = find_sdk_tool(
        &sdk_roots,
        &["platform-tools"],
        executable_name("adb").as_str(),
    )
    .or_else(|| find_on_path(executable_name("adb").as_str()))
    .ok_or_else(|| MobileError::MissingAndroidTool {
        tool: "adb".to_string(),
    })?;
    let emulator = find_sdk_tool(
        &sdk_roots,
        &["emulator"],
        executable_name("emulator").as_str(),
    )
    .or_else(|| find_on_path(executable_name("emulator").as_str()));
    Ok(AndroidToolPaths { adb, emulator })
}

pub fn parse_adb_devices(output: &str) -> Vec<AndroidDeviceSummary> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("List of devices") {
                return None;
            }
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            let state = parts.next().unwrap_or("unknown");
            let details = parts
                .filter_map(|part| {
                    let (key, value) = part.split_once(':')?;
                    Some((key.to_string(), value.to_string()))
                })
                .collect::<BTreeMap<_, _>>();
            Some(AndroidDeviceSummary {
                serial: MobileDeviceId::new(serial),
                state: state.to_string(),
                is_emulator: serial.starts_with("emulator-"),
                details,
            })
        })
        .collect()
}

pub fn parse_avd_list(output: &str) -> Vec<AndroidAvdSummary> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|name| AndroidAvdSummary {
            name: name.to_string(),
        })
        .collect()
}

pub fn parse_ui_automator_snapshot(xml: &str, epoch: u64) -> Result<MobileSnapshot, MobileError> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| MobileError::UiHierarchyParse(error.to_string()))?;
    let mut elements = Vec::new();
    let mut max_x = 0i32;
    let mut max_y = 0i32;

    for node in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "node")
    {
        let Some(bounds) = node
            .attribute("bounds")
            .and_then(|value| parse_bounds(value).ok())
        else {
            continue;
        };
        if bounds.width == 0 || bounds.height == 0 {
            continue;
        }
        max_x = max_x.max(bounds.max_x());
        max_y = max_y.max(bounds.max_y());

        let text = attr(&node, "text");
        let content_desc = attr(&node, "content-desc");
        let resource_id = attr(&node, "resource-id");
        let class_name = attr(&node, "class");
        let package_name = attr(&node, "package");
        let clickable = bool_attr(&node, "clickable");
        let focusable = bool_attr(&node, "focusable");
        let scrollable = bool_attr(&node, "scrollable");
        let label = first_non_empty([
            content_desc.as_deref(),
            text.as_deref(),
            resource_id.as_deref(),
        ])
        .unwrap_or_default();

        if label.is_empty() && !clickable && !focusable && !scrollable {
            continue;
        }

        elements.push(MobileElement {
            index: elements.len() as u32,
            role: infer_role(class_name.as_deref(), clickable, scrollable),
            label,
            text: text.unwrap_or_default(),
            rect: bounds,
            enabled: bool_attr_default(&node, "enabled", true),
            clickable,
            focusable,
            focused: bool_attr(&node, "focused"),
            scrollable,
            selected: bool_attr(&node, "selected"),
            checked: bool_attr(&node, "checked"),
            resource_id,
            class_name,
            package_name,
        });
    }

    Ok(MobileSnapshot {
        epoch,
        device_id: None,
        viewport: MobileViewport {
            width: max_x.max(0) as u32,
            height: max_y.max(0) as u32,
        },
        elements,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidInputText {
    pub encoded: String,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum AndroidInputTextError {
    #[error("text input cannot contain newlines")]
    Newline,
    #[error("text input contains unsupported control character")]
    ControlCharacter,
}

pub fn parse_android_input_text(text: &str) -> Result<AndroidInputText, MobileError> {
    let mut encoded = String::new();
    for character in text.chars() {
        match character {
            '\n' | '\r' => {
                return Err(MobileError::UnsupportedTextInput(
                    AndroidInputTextError::Newline.to_string(),
                ));
            }
            character if character.is_control() => {
                return Err(MobileError::UnsupportedTextInput(
                    AndroidInputTextError::ControlCharacter.to_string(),
                ));
            }
            ' ' => encoded.push_str("%s"),
            '\'' | '"' | '\\' | ';' | '&' | '|' | '<' | '>' | '(' | ')' | '$' | '`' => {
                encoded.push('\\');
                encoded.push(character);
            }
            _ => encoded.push(character),
        }
    }
    Ok(AndroidInputText { encoded })
}

fn parse_bounds(value: &str) -> Result<MobileElementRect, MobileError> {
    let trimmed = value.trim();
    let (first, rest) = trimmed
        .strip_prefix('[')
        .and_then(|value| value.split_once("]["))
        .ok_or_else(|| MobileError::InvalidBounds(value.to_string()))?;
    let second = rest
        .strip_suffix(']')
        .ok_or_else(|| MobileError::InvalidBounds(value.to_string()))?;
    let (x1, y1) = parse_point(first, value)?;
    let (x2, y2) = parse_point(second, value)?;
    let width = x2.saturating_sub(x1).max(0) as u32;
    let height = y2.saturating_sub(y1).max(0) as u32;
    Ok(MobileElementRect {
        x: x1,
        y: y1,
        width,
        height,
    })
}

fn parse_point(point: &str, original: &str) -> Result<(i32, i32), MobileError> {
    let (x, y) = point
        .split_once(',')
        .ok_or_else(|| MobileError::InvalidBounds(original.to_string()))?;
    let x = x
        .parse::<i32>()
        .map_err(|_| MobileError::InvalidBounds(original.to_string()))?;
    let y = y
        .parse::<i32>()
        .map_err(|_| MobileError::InvalidBounds(original.to_string()))?;
    Ok((x, y))
}

fn attr(node: &roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    node.attribute(name)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn bool_attr(node: &roxmltree::Node<'_, '_>, name: &str) -> bool {
    bool_attr_default(node, name, false)
}

fn bool_attr_default(node: &roxmltree::Node<'_, '_>, name: &str, default: bool) -> bool {
    node.attribute(name)
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(default)
}

fn first_non_empty(values: [Option<&str>; 3]) -> Option<String> {
    values
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_string)
}

fn infer_role(class_name: Option<&str>, clickable: bool, scrollable: bool) -> String {
    let class_name = class_name.unwrap_or_default().to_ascii_lowercase();
    if class_name.contains("edittext") {
        "textbox"
    } else if class_name.contains("checkbox") {
        "checkbox"
    } else if class_name.contains("switch") {
        "switch"
    } else if class_name.contains("button") || clickable {
        "button"
    } else if class_name.contains("image") {
        "image"
    } else if scrollable || class_name.contains("recyclerview") || class_name.contains("scroll") {
        "scrollable"
    } else if class_name.contains("textview") {
        "text"
    } else {
        "view"
    }
    .to_string()
}

fn android_sdk_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for key in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(path) = env::var_os(key).map(PathBuf::from) {
            roots.push(path);
        }
    }
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join("Library/Android/sdk"));
        roots.push(home.join("Android/Sdk"));
    }
    if let Some(local_app_data) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        roots.push(local_app_data.join("Android/Sdk"));
    }
    dedupe_paths(roots)
}

fn find_sdk_tool(roots: &[PathBuf], components: &[&str], executable: &str) -> Option<PathBuf> {
    roots
        .iter()
        .map(|root| {
            let mut path = root.clone();
            for component in components {
                path.push(component);
            }
            path.push(executable);
            path
        })
        .find(|path| path.is_file())
}

fn find_on_path(executable: &str) -> Option<PathBuf> {
    env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .map(|path| path.join(executable))
        .find(|path| path.is_file())
}

fn executable_name(name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut deduped = Vec::new();
    for path in paths {
        if !deduped.contains(&path) {
            deduped.push(path);
        }
    }
    deduped
}

#[derive(Debug, Clone)]
struct CommandOutput {
    stdout: String,
    stderr: String,
}

#[derive(Debug, Clone)]
struct CommandBytesOutput {
    stdout: Vec<u8>,
}

fn run_command<I, S>(program: &Path, args: I) -> Result<CommandOutput, MobileError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new(program).args(args).output().map_err(|error| {
        MobileError::AndroidToolFailed {
            tool: program.display().to_string(),
            message: error.to_string(),
        }
    })?;
    if !output.status.success() {
        return Err(MobileError::AndroidToolFailed {
            tool: program.display().to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn run_command_bytes<I, S>(program: &Path, args: I) -> Result<CommandBytesOutput, MobileError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new(program).args(args).output().map_err(|error| {
        MobileError::AndroidToolFailed {
            tool: program.display().to_string(),
            message: error.to_string(),
        }
    })?;
    if !output.status.success() {
        return Err(MobileError::AndroidToolFailed {
            tool: program.display().to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(CommandBytesOutput {
        stdout: output.stdout,
    })
}

fn join_command_output(output: CommandOutput) -> String {
    [output.stdout.trim(), output.stderr.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
