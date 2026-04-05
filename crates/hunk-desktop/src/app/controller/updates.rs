#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateCheckTrigger {
    Automatic,
    UserInitiated,
}

impl UpdateCheckTrigger {
    const fn should_notify_on_error(self) -> bool {
        matches!(self, Self::UserInitiated)
    }

    const fn should_notify_when_up_to_date(self) -> bool {
        matches!(self, Self::UserInitiated)
    }
}

impl DiffViewer {
    const AUTO_UPDATE_CHECK_INTERVAL_MS: i64 = 12 * 60 * 60 * 1000;

    pub(super) fn check_for_updates_action(
        &mut self,
        _: &CheckForUpdates,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_update_check(UpdateCheckTrigger::UserInitiated, Some(window), cx);
    }

    pub(super) fn install_available_update(
        &mut self,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if self.update_activity_in_progress() {
            Self::push_warning_notification(
                "Another updater action is already in progress.".to_string(),
                window,
                cx,
            );
            return;
        }

        let update = match self.update_status.clone() {
            UpdateStatus::UpdateAvailable(update) => update,
            _ => {
                Self::push_warning_notification(
                    "No downloaded update is available to install yet.".to_string(),
                    window,
                    cx,
                );
                return;
            }
        };

        if let InstallSource::PackageManaged { explanation } = &self.update_install_source {
            self.update_status = UpdateStatus::DisabledByInstallSource {
                explanation: explanation.clone(),
            };
            self.git_status_message = Some(explanation.clone());
            Self::push_warning_notification(explanation.clone(), window, cx);
            cx.notify();
            return;
        }

        let public_key = match hunk_updater::required_public_key_base64() {
            Ok(public_key) => public_key,
            Err(err) => {
                let message = err.to_string();
                self.update_status = UpdateStatus::Error(message.clone());
                self.git_status_message = Some(message.clone());
                Self::push_error_notification(message, cx);
                cx.notify();
                return;
            }
        };

        let version = update.version.clone();
        let current_pid = std::process::id();
        self.update_status = UpdateStatus::Downloading {
            version: version.clone(),
        };
        self.git_status_message = Some(format!("Downloading Hunk {version}..."));
        cx.notify();

        self.update_apply_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let staged_update =
                        hunk_updater::stage_available_update(&update, public_key.as_str())?;
                    let current_executable = std::env::current_exe()
                        .context("resolve current Hunk executable for updater install")?;
                    let install_target =
                        hunk_updater::detect_install_target(current_executable.as_path())?;
                    Ok::<_, anyhow::Error>((staged_update, current_executable, install_target, current_pid))
                })
                .await;

            let Some(this) = this.upgrade() else {
                return;
            };

            let mut should_exit = false;
            this.update(cx, |this, cx| {
                match result {
                    Ok((staged_update, current_executable, install_target, current_pid)) => {
                        let version = staged_update.version.clone();
                        match spawn_staged_update_apply(
                            current_executable.as_path(),
                            current_pid,
                            &install_target,
                            &staged_update,
                        ) {
                            Ok(()) => {
                                this.update_status = UpdateStatus::Installing {
                                    version: version.clone(),
                                };
                                this.git_status_message =
                                    Some(format!("Installing Hunk {version}..."));
                                should_exit = true;
                            }
                            Err(err) => {
                                error!("failed to start updater install helper: {err:#}");
                                let summary = err.to_string();
                                this.update_status = UpdateStatus::Error(summary.clone());
                                this.git_status_message =
                                    Some(format!("Update install failed: {summary}"));
                                Self::push_error_notification(
                                    format!("Update install failed: {summary}"),
                                    cx,
                                );
                            }
                        }
                    }
                    Err(err) => {
                        error!("failed to stage update install: {err:#}");
                        let summary = err.to_string();
                        this.update_status = UpdateStatus::Error(summary.clone());
                        this.git_status_message = Some(format!("Update install failed: {summary}"));
                        Self::push_error_notification(
                            format!("Update install failed: {summary}"),
                            cx,
                        );
                    }
                }

                cx.notify();
            });

            if should_exit {
                hunk_codex::host::begin_host_shutdown();
                hunk_codex::host::cleanup_tracked_hosts_for_shutdown();
                std::process::exit(0);
            }
        });
    }

    pub(super) fn maybe_schedule_startup_update_check(&mut self, cx: &mut Context<Self>) {
        if !self.config.auto_update_enabled {
            return;
        }
        if !matches!(self.update_install_source, InstallSource::SelfManaged) {
            return;
        }
        if self.update_activity_in_progress() {
            return;
        }

        let now = now_unix_ms();
        let due = self
            .config
            .last_update_check_at
            .is_none_or(|last_checked| now.saturating_sub(last_checked) >= Self::AUTO_UPDATE_CHECK_INTERVAL_MS);
        if !due {
            return;
        }

        self.start_update_check(UpdateCheckTrigger::Automatic, None, cx);
    }

    fn start_update_check(
        &mut self,
        trigger: UpdateCheckTrigger,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if self.update_activity_in_progress() {
            if matches!(trigger, UpdateCheckTrigger::UserInitiated) {
                Self::push_warning_notification(
                    "Another updater action is already in progress.".to_string(),
                    window,
                    cx,
                );
            }
            return;
        }

        if let InstallSource::PackageManaged { explanation } = &self.update_install_source {
            self.update_status = UpdateStatus::DisabledByInstallSource {
                explanation: explanation.clone(),
            };
            self.git_status_message = Some(explanation.clone());
            if matches!(trigger, UpdateCheckTrigger::UserInitiated) {
                Self::push_warning_notification(explanation.clone(), window, cx);
            }
            cx.notify();
            return;
        }

        let manifest_url = hunk_updater::resolve_manifest_url();
        let current_version = env!("CARGO_PKG_VERSION").to_string();
        let started_at = Instant::now();

        self.update_status = UpdateStatus::Checking;
        self.git_status_message = Some("Checking for updates...".to_string());
        cx.notify();

        self.update_check_task = cx.spawn(async move |this, cx| {
            let (manifest_url, result) = cx
                .background_executor()
                .spawn(async move {
                    let result = hunk_updater::check_for_updates(
                        manifest_url.as_str(),
                        current_version.as_str(),
                    );
                    (manifest_url, result)
                })
                .await;

            let checked_at = now_unix_ms();
            let total_elapsed = started_at.elapsed();
            let Some(this) = this.upgrade() else {
                return;
            };

            this.update(cx, |this, cx| {
                match result {
                    Ok(hunk_updater::UpdateCheckResult::UpToDate { version }) => {
                        debug!(
                            manifest_url,
                            version,
                            elapsed_ms = total_elapsed.as_millis(),
                            "update check completed: up to date"
                        );
                        this.config.last_update_check_at = Some(checked_at);
                        this.persist_config();
                        this.update_status = UpdateStatus::UpToDate {
                            version: version.clone(),
                            checked_at_unix_ms: checked_at,
                        };
                        this.git_status_message = Some(format!("Hunk is up to date ({version})."));
                        if trigger.should_notify_when_up_to_date() {
                            Self::push_success_notification(
                                format!("Hunk is up to date ({version})."),
                                cx,
                            );
                        }
                    }
                    Ok(hunk_updater::UpdateCheckResult::UpdateAvailable(update)) => {
                        let version = update.version.clone();
                        debug!(
                            manifest_url,
                            version,
                            elapsed_ms = total_elapsed.as_millis(),
                            "update check completed: update available"
                        );
                        this.config.last_update_check_at = Some(checked_at);
                        this.persist_config();
                        this.update_status = UpdateStatus::UpdateAvailable(update);
                        let message = format!("Hunk {version} is available.");
                        this.git_status_message = Some(message.clone());
                        Self::push_warning_notification(message, None, cx);
                    }
                    Err(err) => {
                        error!(
                            manifest_url,
                            elapsed_ms = total_elapsed.as_millis(),
                            "update check failed: {err:#}"
                        );
                        let summary = err.to_string();
                        this.update_status = UpdateStatus::Error(summary.clone());
                        this.git_status_message = Some(format!("Update check failed: {summary}"));
                        if trigger.should_notify_on_error() {
                            Self::push_error_notification(
                                format!("Update check failed: {summary}"),
                                cx,
                            );
                        }
                    }
                }

                cx.notify();
            });
        });
    }

    pub(super) fn update_activity_in_progress(&self) -> bool {
        matches!(
            self.update_status,
            UpdateStatus::Checking
                | UpdateStatus::Downloading { .. }
                | UpdateStatus::Installing { .. }
        )
    }
}

fn spawn_staged_update_apply(
    current_executable: &std::path::Path,
    current_pid: u32,
    install_target: &hunk_updater::UpdateInstallTarget,
    staged_update: &hunk_updater::StagedUpdate,
) -> anyhow::Result<()> {
    match install_target {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        hunk_updater::UpdateInstallTarget::MacOsApp { .. }
        | hunk_updater::UpdateInstallTarget::LinuxBundle { .. } => {
            std::process::Command::new(current_executable)
                .arg("--apply-staged-update")
                .arg("--wait-pid")
                .arg(current_pid.to_string())
                .arg("--package")
                .arg(staged_update.package_path.as_os_str())
                .arg("--asset-format")
                .arg(staged_update.asset.format.as_str())
                .spawn()
                .with_context(|| {
                    format!(
                        "spawn updater helper from {}",
                        current_executable.display()
                    )
                })?;
            Ok(())
        }
        #[cfg(target_os = "windows")]
        hunk_updater::UpdateInstallTarget::WindowsMsi {
            current_executable,
        } => spawn_windows_update_script(current_pid, current_executable.as_path(), staged_update),
        #[allow(unreachable_patterns)]
        other => anyhow::bail!(
            "updater apply helper is not supported for install target {:?} on this platform",
            other
        ),
    }
}

#[cfg(target_os = "windows")]
fn spawn_windows_update_script(
    current_pid: u32,
    current_executable: &std::path::Path,
    staged_update: &hunk_updater::StagedUpdate,
) -> anyhow::Result<()> {
    let script_path = staged_update.package_path.with_extension("ps1");
    let script = format!(
        "$waitPid = {current_pid}\n\
$msiPath = {msi_path}\n\
$appPath = {app_path}\n\
while (Get-Process -Id $waitPid -ErrorAction SilentlyContinue) {{ Start-Sleep -Milliseconds 200 }}\n\
$process = Start-Process -FilePath 'msiexec.exe' -ArgumentList @('/i', $msiPath, '/passive', '/norestart') -Wait -PassThru\n\
if ($process.ExitCode -ne 0) {{ exit $process.ExitCode }}\n\
Start-Process -FilePath $appPath\n",
        msi_path = powershell_single_quoted(staged_update.package_path.as_path()),
        app_path = powershell_single_quoted(current_executable),
    );
    std::fs::write(script_path.as_path(), script).with_context(|| {
        format!(
            "write staged Windows update helper script {}",
            script_path.display()
        )
    })?;
    std::process::Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(script_path.as_os_str())
        .spawn()
        .context("spawn Windows staged updater helper")?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn powershell_single_quoted(path: &std::path::Path) -> String {
    let escaped = path.display().to_string().replace('\'', "''");
    format!("'{escaped}'")
}
