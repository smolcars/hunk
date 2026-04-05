use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use flate2::read::GzDecoder;
use semver::Version;
use serde::{Deserialize, Serialize};
use tar::Archive;

pub const DEFAULT_UPDATE_MANIFEST_URL: &str =
    "https://pub-de32dfa5fe9845849590fa075f3edafa.r2.dev/stable.json";
pub const UPDATE_EXPLANATION_ENV_VAR: &str = "HUNK_UPDATE_EXPLANATION";
pub const UPDATE_MANIFEST_URL_ENV_VAR: &str = "HUNK_UPDATE_MANIFEST_URL";
pub const UPDATE_PUBLIC_KEY_ENV_VAR: &str = "HUNK_UPDATE_PUBLIC_KEY";
pub const UPDATE_PRIVATE_KEY_ENV_VAR: &str = "HUNK_UPDATE_PRIVATE_KEY";
pub const UPDATE_PUBLIC_KEY_BUILD_ENV_VAR: &str = "HUNK_UPDATE_PUBLIC_KEY";

const UPDATE_HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const PROCESS_EXIT_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssetFormat {
    App,
    Wix,
    Tarball,
}

impl AssetFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Wix => "wix",
            Self::Tarball => "tarball",
        }
    }
}

impl std::str::FromStr for AssetFormat {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "app" => Ok(Self::App),
            "wix" => Ok(Self::Wix),
            "tarball" => Ok(Self::Tarball),
            other => bail!("unsupported asset format `{other}`"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub url: String,
    pub signature: String,
    pub format: AssetFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub version: String,
    #[serde(default)]
    pub pub_date: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub platforms: BTreeMap<String, ReleaseAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallSource {
    SelfManaged,
    PackageManaged { explanation: String },
}

impl InstallSource {
    pub fn explanation(&self) -> Option<&str> {
        match self {
            Self::SelfManaged => None,
            Self::PackageManaged { explanation } => Some(explanation.as_str()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub manifest_url: String,
    pub version: String,
    pub pub_date: Option<String>,
    pub notes: Option<String>,
    pub target: String,
    pub asset: ReleaseAsset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckResult {
    UpToDate { version: String },
    UpdateAvailable(AvailableUpdate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedUpdate {
    pub manifest_url: String,
    pub version: String,
    pub target: String,
    pub asset: ReleaseAsset,
    pub package_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateInstallTarget {
    MacOsApp {
        app_path: PathBuf,
        relaunch_executable: PathBuf,
    },
    LinuxBundle {
        install_root: PathBuf,
        relaunch_executable: PathBuf,
    },
    WindowsMsi {
        current_executable: PathBuf,
    },
}

impl UpdateInstallTarget {
    pub fn relaunch_executable(&self) -> &Path {
        match self {
            Self::MacOsApp {
                relaunch_executable, ..
            } => relaunch_executable.as_path(),
            Self::LinuxBundle {
                relaunch_executable, ..
            } => relaunch_executable.as_path(),
            Self::WindowsMsi { current_executable } => current_executable.as_path(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedUpdate {
    pub relaunch_executable: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    Downloading { version: String },
    Installing { version: String },
    DisabledByInstallSource {
        explanation: String,
    },
    UpToDate {
        version: String,
        checked_at_unix_ms: i64,
    },
    UpdateAvailable(AvailableUpdate),
    Error(String),
}

pub fn resolve_manifest_url() -> String {
    env::var(UPDATE_MANIFEST_URL_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_UPDATE_MANIFEST_URL.to_string())
}

pub fn resolve_public_key_base64() -> Option<String> {
    env::var(UPDATE_PUBLIC_KEY_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            option_env!("HUNK_UPDATE_PUBLIC_KEY")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}

pub fn required_public_key_base64() -> Result<String> {
    resolve_public_key_base64().ok_or_else(|| {
        anyhow!(
            "updater public key is not configured; set {} at runtime or compile it into the build",
            UPDATE_PUBLIC_KEY_ENV_VAR
        )
    })
}

pub fn resolve_private_key_base64() -> Option<String> {
    env::var(UPDATE_PRIVATE_KEY_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn detect_install_source() -> InstallSource {
    install_source_from_explanation(env::var(UPDATE_EXPLANATION_ENV_VAR).ok().as_deref())
}

pub fn install_source_from_explanation(explanation: Option<&str>) -> InstallSource {
    explanation
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| InstallSource::PackageManaged {
            explanation: value.to_string(),
        })
        .unwrap_or(InstallSource::SelfManaged)
}

pub fn current_update_target() -> Result<&'static str> {
    match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => Ok("macos-aarch64"),
        ("macos", "x86_64") => Ok("macos-x86_64"),
        ("windows", "x86_64") => Ok("windows-x86_64"),
        ("windows", "aarch64") => Ok("windows-aarch64"),
        ("linux", "x86_64") => Ok("linux-x86_64"),
        ("linux", "aarch64") => Ok("linux-aarch64"),
        (os, arch) => bail!("unsupported update target: {os}-{arch}"),
    }
}

pub fn check_for_updates(manifest_url: &str, current_version: &str) -> Result<UpdateCheckResult> {
    let manifest = fetch_release_manifest(manifest_url)?;

    evaluate_manifest(
        manifest_url,
        current_version,
        current_update_target()?,
        manifest,
    )
}

pub fn fetch_release_manifest(manifest_url: &str) -> Result<ReleaseManifest> {
    let client = build_http_client()?;
    let manifest_bytes = client
        .get(manifest_url)
        .send()
        .with_context(|| format!("failed to fetch update manifest from {manifest_url}"))?
        .error_for_status()
        .with_context(|| format!("update manifest request failed for {manifest_url}"))?
        .bytes()
        .with_context(|| format!("failed to read update manifest bytes from {manifest_url}"))?
        .to_vec();

    let public_key_base64 = required_public_key_base64()?;
    let signature_url = format!("{manifest_url}.sig");
    let signature = client
        .get(signature_url.as_str())
        .send()
        .with_context(|| {
            format!("failed to fetch update manifest signature from {signature_url}")
        })?
        .error_for_status()
        .with_context(|| {
            format!("update manifest signature request failed for {signature_url}")
        })?
        .text()
        .with_context(|| {
            format!("failed to read update manifest signature from {signature_url}")
        })?;
    verify_payload_signature(
        &manifest_bytes,
        signature.trim(),
        public_key_base64.as_str(),
    )
    .with_context(|| format!("failed to verify update manifest from {manifest_url}"))?;

    serde_json::from_slice::<ReleaseManifest>(&manifest_bytes)
        .with_context(|| format!("failed to parse update manifest from {manifest_url}"))
}

pub fn download_url(url: &str) -> Result<Vec<u8>> {
    build_http_client()?
        .get(url)
        .send()
        .with_context(|| format!("failed to download release asset from {url}"))?
        .error_for_status()
        .with_context(|| format!("release asset request failed for {url}"))?
        .bytes()
        .with_context(|| format!("failed to read release asset bytes from {url}"))
        .map(|bytes| bytes.to_vec())
}

pub fn write_downloaded_asset(destination: &Path, bytes: &[u8]) -> Result<()> {
    let parent = destination.parent().ok_or_else(|| {
        anyhow!(
            "download destination has no parent: {}",
            destination.display()
        )
    })?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create download directory {}", parent.display()))?;
    fs::write(destination, bytes).with_context(|| {
        format!(
            "failed to write downloaded asset to {}",
            destination.display()
        )
    })
}

pub fn download_and_verify_release_asset(
    asset: &ReleaseAsset,
    public_key_base64: &str,
) -> Result<Vec<u8>> {
    let bytes = download_url(asset.url.as_str())?;
    verify_payload_signature(&bytes, asset.signature.as_str(), public_key_base64)?;
    Ok(bytes)
}

pub fn stage_available_update(
    update: &AvailableUpdate,
    public_key_base64: &str,
) -> Result<StagedUpdate> {
    let bytes = download_and_verify_release_asset(&update.asset, public_key_base64)?;
    let staging_dir = create_staging_dir("download")?;
    let package_path = staging_dir.join(staged_asset_file_name(&update.asset));
    write_downloaded_asset(package_path.as_path(), &bytes)?;

    Ok(StagedUpdate {
        manifest_url: update.manifest_url.clone(),
        version: update.version.clone(),
        target: update.target.clone(),
        asset: update.asset.clone(),
        package_path,
    })
}

pub fn detect_install_target(current_executable: &Path) -> Result<UpdateInstallTarget> {
    #[cfg(target_os = "macos")]
    {
        if let Some(app_path) = macos_app_bundle_from_current_executable(current_executable) {
            let executable_name = current_executable.file_name().ok_or_else(|| {
                anyhow!(
                    "current executable has no file name: {}",
                    current_executable.display()
                )
            })?;
            return Ok(UpdateInstallTarget::MacOsApp {
                relaunch_executable: app_path
                    .join("Contents")
                    .join("MacOS")
                    .join(executable_name),
                app_path,
            });
        }

        bail!(
            "could not resolve macOS app bundle from {}",
            current_executable.display()
        );
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(install_root) = linux_bundle_install_root_from_current_executable(current_executable)
        {
            let executable_name = current_executable.file_name().ok_or_else(|| {
                anyhow!(
                    "current executable has no file name: {}",
                    current_executable.display()
                )
            })?;
            return Ok(UpdateInstallTarget::LinuxBundle {
                relaunch_executable: install_root.join(executable_name),
                install_root,
            });
        }

        bail!(
            "could not resolve Linux self-managed install root from {}",
            current_executable.display()
        );
    }

    #[cfg(target_os = "windows")]
    {
        Ok(UpdateInstallTarget::WindowsMsi {
            current_executable: current_executable.to_path_buf(),
        })
    }
}

pub fn apply_staged_update_from_current_executable(
    current_executable: &Path,
    staged_package_path: &Path,
    asset_format: AssetFormat,
) -> Result<AppliedUpdate> {
    let install_target = detect_install_target(current_executable)?;
    let relaunch_executable = install_target.relaunch_executable().to_path_buf();

    match install_target {
        #[cfg(target_os = "macos")]
        UpdateInstallTarget::MacOsApp { app_path, .. } => {
            if !matches!(asset_format, AssetFormat::App) {
                bail!(
                    "macOS updater expected `app` asset format, received `{}`",
                    asset_format.as_str()
                );
            }
            apply_archive_replace(staged_package_path, app_path.as_path(), locate_extracted_macos_app)?;
        }
        #[cfg(target_os = "linux")]
        UpdateInstallTarget::LinuxBundle { install_root, .. } => {
            if !matches!(asset_format, AssetFormat::Tarball) {
                bail!(
                    "Linux updater expected `tarball` asset format, received `{}`",
                    asset_format.as_str()
                );
            }
            apply_archive_replace(
                staged_package_path,
                install_root.as_path(),
                locate_extracted_linux_bundle,
            )?;
        }
        #[cfg(target_os = "windows")]
        UpdateInstallTarget::WindowsMsi { .. } => {
            let _ = staged_package_path;
            let _ = asset_format;
            bail!("Windows updates must be applied by running the staged MSI installer");
        }
        #[allow(unreachable_patterns)]
        other => bail!(
            "staged update application is not supported for install target {:?} on this platform",
            other
        ),
    }

    let _ = fs::remove_file(staged_package_path);

    Ok(AppliedUpdate {
        relaunch_executable,
    })
}

pub fn wait_for_process_to_exit(pid: u32, timeout: Duration) -> Result<()> {
    #[cfg(unix)]
    {
        let process_id = i32::try_from(pid).context("process id does not fit into pid_t")?;
        let started_at = Instant::now();
        loop {
            let result = unsafe { libc::kill(process_id, 0) };
            if result == 0 {
                if started_at.elapsed() >= timeout {
                    bail!("timed out waiting for process {pid} to exit");
                }
                thread::sleep(PROCESS_EXIT_WAIT_POLL_INTERVAL);
                continue;
            }

            let error = std::io::Error::last_os_error();
            match error.raw_os_error() {
                Some(code) if code == libc::ESRCH => return Ok(()),
                Some(code) if code == libc::EPERM => {
                    if started_at.elapsed() >= timeout {
                        bail!("timed out waiting for process {pid} to exit");
                    }
                    thread::sleep(PROCESS_EXIT_WAIT_POLL_INTERVAL);
                }
                _ => return Err(error).context("failed to poll process state"),
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        let _ = timeout;
        bail!("waiting for another process is not implemented on this platform")
    }
}

pub fn evaluate_manifest(
    manifest_url: &str,
    current_version: &str,
    target: &str,
    manifest: ReleaseManifest,
) -> Result<UpdateCheckResult> {
    let current = parse_stable_version(current_version)
        .with_context(|| format!("invalid current app version `{current_version}`"))?;
    let latest = parse_stable_version(manifest.version.as_str())
        .with_context(|| format!("invalid update manifest version `{}`", manifest.version))?;

    if latest <= current {
        return Ok(UpdateCheckResult::UpToDate {
            version: manifest.version,
        });
    }

    let asset = manifest
        .platforms
        .get(target)
        .cloned()
        .ok_or_else(|| anyhow!("update manifest does not contain platform asset `{target}`"))?;

    Ok(UpdateCheckResult::UpdateAvailable(AvailableUpdate {
        manifest_url: manifest_url.to_string(),
        version: manifest.version,
        pub_date: manifest.pub_date,
        notes: manifest.notes,
        target: target.to_string(),
        asset,
    }))
}

fn parse_stable_version(raw: &str) -> Result<Version> {
    let version = Version::parse(raw)?;
    if !version.pre.is_empty() {
        bail!("prerelease versions are not supported");
    }
    Ok(version)
}

pub fn public_key_from_private_key_base64(private_key_base64: &str) -> Result<String> {
    let signing_key = signing_key_from_base64(private_key_base64)?;
    Ok(BASE64_STANDARD.encode(signing_key.verifying_key().to_bytes()))
}

pub fn sign_payload(payload: &[u8], private_key_base64: &str) -> Result<String> {
    let signing_key = signing_key_from_base64(private_key_base64)?;
    Ok(BASE64_STANDARD.encode(signing_key.sign(payload).to_bytes()))
}

pub fn verify_payload_signature(
    payload: &[u8],
    signature_base64: &str,
    public_key_base64: &str,
) -> Result<()> {
    let verifying_key = verifying_key_from_base64(public_key_base64)?;
    let signature = signature_from_base64(signature_base64)?;
    verifying_key
        .verify_strict(payload, &signature)
        .context("release asset signature verification failed")
}

fn signing_key_from_base64(private_key_base64: &str) -> Result<SigningKey> {
    let bytes = decode_base64(private_key_base64, "private key")?;
    let key_bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow!("private key must decode to exactly 32 bytes"))?;
    Ok(SigningKey::from_bytes(&key_bytes))
}

fn verifying_key_from_base64(public_key_base64: &str) -> Result<VerifyingKey> {
    let bytes = decode_base64(public_key_base64, "public key")?;
    let key_bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow!("public key must decode to exactly 32 bytes"))?;
    VerifyingKey::from_bytes(&key_bytes).context("invalid Ed25519 public key")
}

fn signature_from_base64(signature_base64: &str) -> Result<Signature> {
    let bytes = decode_base64(signature_base64, "signature")?;
    Signature::try_from(bytes.as_slice()).context("invalid Ed25519 signature")
}

fn decode_base64(value: &str, label: &str) -> Result<Vec<u8>> {
    BASE64_STANDARD
        .decode(value.trim())
        .with_context(|| format!("failed to decode {label} from base64"))
}

fn build_http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(UPDATE_HTTP_TIMEOUT)
        .user_agent(format!("hunk-updater/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to create updater HTTP client")
}

fn create_staging_dir(label: &str) -> Result<PathBuf> {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before unix epoch")?
        .as_millis();
    let directory = env::temp_dir().join(format!(
        "hunk-updater-{label}-{}-{timestamp_ms}",
        std::process::id()
    ));
    fs::create_dir_all(directory.as_path())
        .with_context(|| format!("failed to create staging directory {}", directory.display()))?;
    Ok(directory)
}

fn staged_asset_file_name(asset: &ReleaseAsset) -> String {
    asset_url_file_name(asset.url.as_str()).unwrap_or_else(|| match asset.format {
        AssetFormat::App => "Hunk.app.tar.gz".to_string(),
        AssetFormat::Wix => "Hunk.msi".to_string(),
        AssetFormat::Tarball => "Hunk-linux.tar.gz".to_string(),
    })
}

fn asset_url_file_name(url: &str) -> Option<String> {
    let path = url.split('?').next()?;
    let file_name = path.rsplit('/').next()?.trim();
    if file_name.is_empty() {
        None
    } else {
        Some(file_name.to_string())
    }
}

fn apply_archive_replace(
    staged_package_path: &Path,
    install_root: &Path,
    locate_replacement_root: fn(&Path) -> Result<PathBuf>,
) -> Result<()> {
    install_root.parent().ok_or_else(|| {
        anyhow!(
            "install root has no parent directory: {}",
            install_root.display()
        )
    })?;
    let staging_root = unique_sibling_path(install_root, ".update-staged");
    fs::create_dir_all(staging_root.as_path())
        .with_context(|| format!("failed to create staging root {}", staging_root.display()))?;

    extract_tar_gz_archive(staged_package_path, staging_root.as_path())?;
    let replacement_root = locate_replacement_root(staging_root.as_path())?;
    replace_install_root(install_root, replacement_root.as_path())?;

    if staging_root.exists() && staging_root != replacement_root {
        let _ = fs::remove_dir_all(staging_root.as_path());
    }

    let _ = fs::remove_file(staged_package_path);

    Ok(())
}

fn extract_tar_gz_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    let archive_file = File::open(archive_path)
        .with_context(|| format!("failed to open archive {}", archive_path.display()))?;
    let decoder = GzDecoder::new(archive_file);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(destination)
        .with_context(|| format!("failed to unpack archive into {}", destination.display()))
}

fn replace_install_root(current_root: &Path, replacement_root: &Path) -> Result<()> {
    let backup_root = unique_sibling_path(current_root, ".update-backup");
    fs::rename(current_root, backup_root.as_path()).with_context(|| {
        format!(
            "failed to move current install root {} to backup {}",
            current_root.display(),
            backup_root.display()
        )
    })?;

    if let Err(err) = fs::rename(replacement_root, current_root) {
        let _ = fs::rename(backup_root.as_path(), current_root);
        return Err(err).with_context(|| {
            format!(
                "failed to move replacement install root {} into place at {}",
                replacement_root.display(),
                current_root.display()
            )
        });
    }

    let _ = fs::remove_dir_all(backup_root.as_path());
    Ok(())
}

fn unique_sibling_path(base_path: &Path, suffix: &str) -> PathBuf {
    let parent_directory = base_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = base_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("hunk");
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    parent_directory.join(format!(
        ".{file_name}{suffix}-{}-{timestamp_ms}",
        std::process::id()
    ))
}

#[cfg(target_os = "macos")]
fn macos_app_bundle_from_current_executable(current_executable: &Path) -> Option<PathBuf> {
    let macos_directory = current_executable.parent()?;
    if macos_directory.file_name()? != "MacOS" {
        return None;
    }
    let contents_directory = macos_directory.parent()?;
    if contents_directory.file_name()? != "Contents" {
        return None;
    }
    let app_path = contents_directory.parent()?;
    if app_path.extension()? != "app" {
        return None;
    }
    Some(app_path.to_path_buf())
}

#[cfg(target_os = "linux")]
fn linux_bundle_install_root_from_current_executable(current_executable: &Path) -> Option<PathBuf> {
    current_executable
        .ancestors()
        .filter(|ancestor| ancestor.is_dir())
        .find(|ancestor| is_linux_bundle_root(ancestor))
        .map(Path::to_path_buf)
}

#[cfg(target_os = "linux")]
fn is_linux_bundle_root(path: &Path) -> bool {
    path.join("hunk_desktop_bin").is_file()
        && path.join("hunk-desktop").is_file()
        && path.join("lib").is_dir()
        && path.join("codex-runtime").join("linux").join("codex").is_file()
}

#[cfg(target_os = "macos")]
fn locate_extracted_macos_app(staging_root: &Path) -> Result<PathBuf> {
    if staging_root.extension().is_some_and(|value| value == "app") && staging_root.is_dir() {
        return Ok(staging_root.to_path_buf());
    }

    locate_single_matching_child(staging_root, |candidate| {
        candidate.extension().is_some_and(|value| value == "app")
            && candidate.join("Contents").join("MacOS").is_dir()
    })
    .or_else(|_| {
        let child_directory = locate_single_directory_child(staging_root)?;
        locate_single_matching_child(child_directory.as_path(), |candidate| {
            candidate.extension().is_some_and(|value| value == "app")
                && candidate.join("Contents").join("MacOS").is_dir()
        })
    })
}

#[cfg(target_os = "linux")]
fn locate_extracted_linux_bundle(staging_root: &Path) -> Result<PathBuf> {
    if is_linux_bundle_root(staging_root) {
        return Ok(staging_root.to_path_buf());
    }

    let child_directory = locate_single_directory_child(staging_root)?;
    if is_linux_bundle_root(child_directory.as_path()) {
        return Ok(child_directory);
    }

    bail!(
        "failed to locate extracted Linux bundle inside {}",
        staging_root.display()
    )
}

fn locate_single_directory_child(root: &Path) -> Result<PathBuf> {
    locate_single_matching_child(root, |candidate| candidate.is_dir())
}

fn locate_single_matching_child(
    root: &Path,
    predicate: impl Fn(&Path) -> bool,
) -> Result<PathBuf> {
    let mut matches = Vec::new();
    for entry in fs::read_dir(root)
        .with_context(|| format!("failed to read directory {}", root.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read entry in {}", root.display()))?;
        let candidate = entry.path();
        if predicate(candidate.as_path()) {
            matches.push(candidate);
        }
    }

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => bail!("failed to locate extracted content inside {}", root.display()),
        _ => bail!("multiple extracted content candidates found inside {}", root.display()),
    }
}
