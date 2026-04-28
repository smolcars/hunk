use std::time::Instant;

use hunk_forge::{
    ForgeProvider, GitHubAuthMode, GitHubAuthenticatedAccount, GitHubDeviceAuthorization,
    GitHubDeviceFlowPoll, GitHubDeviceFlowService, GitHubOAuthAppConfig,
    github_auth_mode_for_host,
};

// OAuth client IDs are public identifiers, so shipping the GitHub.com device-flow client id
// in the desktop binary is expected and avoids a local env-var setup step.
const HUNK_GITHUB_DEVICE_FLOW_CLIENT_ID: &str = "Ov23liecmGTDOJDVpP5c";
const GITHUB_OAUTH_CLIENT_ID_ENV_KEY: &str = "HUNK_GITHUB_OAUTH_CLIENT_ID";
const GITHUB_DEVICE_FLOW_SLOW_DOWN_SECS: u64 = 5;

#[derive(Debug, Clone)]
struct GitHubDeviceSignInResult {
    repo: ForgeRepoRef,
    account: GitHubAuthenticatedAccount,
    access_token: String,
}

impl DiffViewer {
    fn refresh_selected_git_workspace_review_summary_after_auth_change(
        &mut self,
        repo: &ForgeRepoRef,
        cx: &mut Context<Self>,
    ) {
        let Some(repo_root) = self.selected_git_workspace_root() else {
            return;
        };
        let branch_name = self.git_workspace.branch_name.trim().to_string();
        if branch_name.is_empty() || matches!(branch_name.as_str(), "detached" | "unknown") {
            return;
        }

        self.refresh_git_workspace_forge_repo(repo_root.as_path(), branch_name.as_str());
        if self.git_workspace_forge_repo.as_ref() != Some(repo) {
            return;
        }

        self.clear_review_summary_miss_for_branch(repo_root.as_path(), branch_name.as_str());
        self.maybe_queue_review_summary_lookup(repo_root, branch_name, cx);
    }

    pub(super) fn refresh_git_workspace_forge_repo(
        &mut self,
        repo_root: &std::path::Path,
        branch_name: &str,
    ) {
        let branch_name = branch_name.trim();
        if branch_name.is_empty() || matches!(branch_name, "detached" | "unknown") {
            self.git_workspace_forge_repo = None;
            return;
        }

        self.git_workspace_forge_repo = review_remote_for_branch_with_provider_map(
            repo_root,
            branch_name,
            self.config.review_provider_mappings.as_slice(),
        )
        .ok()
        .flatten()
        .and_then(|review_remote| ForgeRepoRef::try_from(&review_remote).ok());
    }

    pub(super) fn current_git_workspace_github_dot_com_repo(&self) -> Option<&ForgeRepoRef> {
        let repo = self.git_workspace_forge_repo.as_ref()?;
        (repo.provider == ForgeProvider::GitHub
            && github_auth_mode_for_host(repo.host.as_str()) == GitHubAuthMode::DeviceFlow)
            .then_some(repo)
    }

    pub(super) fn current_git_workspace_forge_repo(&self) -> Option<&ForgeRepoRef> {
        self.git_workspace_forge_repo.as_ref()
    }

    pub(super) fn pending_github_device_flow_prompt_for_repo(
        &self,
        repo: &ForgeRepoRef,
    ) -> Option<&GitHubDeviceFlowPromptState> {
        self.github_device_flow_prompt
            .as_ref()
            .filter(|prompt| &prompt.repo == repo)
    }

    pub(super) fn github_session_identity_for_repo(
        &self,
        repo: &ForgeRepoRef,
    ) -> Option<(String, String, Option<String>)> {
        let resolved = self.resolved_forge_credential_for_repo(repo)?;
        let credential = self
            .config
            .forge_credentials
            .iter()
            .find(|credential| credential.id == resolved.credential_id)?;
        (credential.kind == hunk_domain::config::ForgeCredentialKind::GitHubComSession).then(|| {
            (
                credential.id.clone(),
                credential.account_label.clone(),
                credential.account_login.clone(),
            )
        })
    }

    pub(super) fn open_forge_token_dialog_for_repo(
        &mut self,
        repo: ForgeRepoRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let token_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(format!("{} personal access token", forge_provider_label(repo.provider)))
        });
        let description = format!("Save a personal access token for {}.", repo.path);
        let host_hint = format!("Host: {}", repo.host);
        let view = cx.entity();

        gpui_component::WindowExt::open_alert_dialog(window, cx, move |alert, _, cx| {
            alert
                .width(px(520.0))
                .title(format!(
                    "{} Personal Access Token",
                    forge_provider_label(repo.provider)
                ))
                .description(description.clone())
                .child(
                    v_flex()
                        .w_full()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(host_hint.clone()),
                        )
                        .child(
                            v_flex()
                                .w_full()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Personal Access Token"),
                                )
                                .child(
                                    gpui_component::input::Input::new(&token_input)
                                        .appearance(true)
                                        .w_full()
                                        .with_size(gpui_component::Size::Medium),
                                ),
                        ),
                )
                .footer(
                    DialogFooter::new()
                        .w_full()
                        .justify_end()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Button::new("forge-token-cancel")
                                        .label("Cancel")
                                        .outline()
                                        .on_click(|_, window, cx| {
                                            window.close_dialog(cx);
                                        }),
                                )
                                .child(
                                    Button::new("forge-token-save")
                                        .label("Save Token")
                                        .primary()
                                        .on_click({
                                            let view = view.clone();
                                            let repo = repo.clone();
                                            let token_input = token_input.clone();
                                            move |_, window, cx| {
                                                let token =
                                                    token_input.read(cx).value().to_string();
                                                let result = view.update(cx, |this, cx| {
                                                    this.save_forge_token_for_repo(
                                                        &repo,
                                                        token.as_str(),
                                                        cx,
                                                    )
                                                });
                                                match result {
                                                    Ok(()) => window.close_dialog(cx),
                                                    Err(message) => {
                                                        view.update(cx, |this, cx| {
                                                            this.set_git_warning_message(
                                                                message,
                                                                Some(window),
                                                                cx,
                                                            );
                                                        });
                                                    }
                                                }
                                            }
                                        }),
                                ),
                        ),
                )
        });
    }

    pub(super) fn save_forge_token_for_repo(
        &mut self,
        repo: &ForgeRepoRef,
        token: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let token = token.trim();
        if token.is_empty() {
            return Err(format!("{} token is required.", forge_provider_label(repo.provider)));
        }

        let credential_id = self
            .remember_forge_token_for_repo(repo, token)
            .ok_or_else(|| format!("{} token is required.", forge_provider_label(repo.provider)))?;
        if let Err(err) = save_forge_secret(credential_id.as_str(), token) {
            error!(
                "failed to save {} token for credential {}: {err:#}",
                forge_provider_log_label(repo.provider),
                credential_id
            );
            Self::push_warning_notification(
                format!(
                    "{} token could not be saved to the system credential store. It will only be available in this Hunk session.",
                    forge_provider_label(repo.provider)
                ),
                None,
                cx,
            );
        }

        let message = format!("Saved {} token for {}", forge_provider_label(repo.provider), repo.path);
        self.git_status_message = Some(message.clone());
        Self::push_success_notification(message, cx);
        self.refresh_selected_git_workspace_review_summary_after_auth_change(repo, cx);
        cx.notify();
        Ok(())
    }

    pub(super) fn delete_forge_token_for_repo(
        &mut self,
        repo: &ForgeRepoRef,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let resolved = self
            .resolved_forge_credential_for_repo(repo)
            .ok_or_else(|| {
                format!(
                    "No saved {} token is available for this repo.",
                    forge_provider_label(repo.provider)
                )
            })?;
        let credential_id = resolved.credential_id;
        let removed_index = self
            .config
            .forge_credentials
            .iter()
            .position(|credential| credential.id == credential_id)
            .ok_or_else(|| "The saved forge token could not be found.".to_string())?;
        let removed = self.config.forge_credentials.remove(removed_index);

        self.config
            .forge_repo_credential_bindings
            .retain(|binding| binding.credential_id != credential_id);

        let host_missing_default = !self.config.forge_credentials.iter().any(|credential| {
            credential.provider == removed.provider
                && credential.host == removed.host
                && credential.is_default_for_host
        });
        if removed.is_default_for_host
            && host_missing_default
            && let Some(next_default) = self.config.forge_credentials.iter_mut().find(|credential| {
                credential.provider == removed.provider && credential.host == removed.host
            })
        {
            next_default.is_default_for_host = true;
        }

        self.forge_tokens_by_credential_id
            .remove(credential_id.as_str());
        self.persist_config();

        if let Err(err) = delete_forge_secret(credential_id.as_str()) {
            warn!(
                "failed to delete saved {} token {}: {err:#}",
                forge_provider_log_label(repo.provider),
                credential_id
            );
            Self::push_warning_notification(
                format!(
                    "Deleted the {} token in Hunk, but the saved token could not be removed from the system credential store.",
                    forge_provider_label(repo.provider)
                ),
                None,
                cx,
            );
        }

        let message = format!(
            "Deleted {} token for {}",
            forge_provider_label(repo.provider),
            repo.path
        );
        self.git_status_message = Some(message.clone());
        Self::push_success_notification(message, cx);
        self.refresh_selected_git_workspace_review_summary_after_auth_change(repo, cx);
        cx.notify();
        Ok(())
    }

    pub(super) fn start_github_device_sign_in(
        &mut self,
        repo: ForgeRepoRef,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if github_auth_mode_for_host(repo.host.as_str()) != GitHubAuthMode::DeviceFlow {
            return Err("Device sign-in is only available for github.com right now.".to_string());
        }

        let app_config = load_github_oauth_app_config().map_err(|err| err.to_string())?;
        let auth_service = GitHubDeviceFlowService::new(app_config.client_id.clone())
            .map_err(|err| err.to_string())?;
        let authorization = auth_service
            .start_device_flow()
            .map_err(|err| err.to_string())?;

        let browser_opened = open_url_in_browser(authorization.verification_uri.as_str()).is_ok();
        cx.write_to_clipboard(ClipboardItem::new_string(authorization.user_code.clone()));

        self.github_device_flow_prompt = Some(GitHubDeviceFlowPromptState {
            repo: repo.clone(),
            verification_uri: authorization.verification_uri.clone(),
            user_code: authorization.user_code.clone(),
        });

        let launch_message = if browser_opened {
            format!(
                "Opened GitHub sign-in. Enter code {} in the browser. The code was copied to the clipboard.",
                authorization.user_code
            )
        } else {
            format!(
                "Open {} and enter code {}. The code was copied to the clipboard.",
                authorization.verification_uri, authorization.user_code
            )
        };

        let epoch = self.begin_git_action("GitHub sign-in".to_string(), cx);
        let started_at = Instant::now();
        self.git_status_message = Some(launch_message.clone());
        if browser_opened {
            Self::push_success_notification(launch_message, cx);
        } else {
            Self::push_warning_notification(launch_message, None, cx);
        }
        cx.notify();

        self.git_action_task = cx.spawn(async move |this, cx| {
            let (execution_elapsed, result) = cx
                .background_executor()
                .spawn(async move {
                    let execution_started_at = Instant::now();
                    let result = complete_github_device_sign_in(repo, auth_service, authorization);
                    (execution_started_at.elapsed(), result)
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    let total_elapsed = started_at.elapsed();
                    this.github_device_flow_prompt = None;
                    this.finish_git_action();
                    match result {
                        Ok(result) => {
                            let credential_id =
                                this.remember_github_session_for_repo(&result.repo, &result.account);
                            this.forge_tokens_by_credential_id.insert(
                                credential_id.clone(),
                                result.access_token.clone(),
                            );
                            if let Err(err) =
                                save_forge_secret(credential_id.as_str(), result.access_token.as_str())
                            {
                                warn!(
                                    "failed to save GitHub device-flow session {}: {err:#}",
                                    credential_id
                                );
                                Self::push_warning_notification(
                                    "GitHub session could not be saved to the system credential store. It will only be available in this Hunk session.".to_string(),
                                    None,
                                    cx,
                                );
                            }
                            debug!(
                                "github device sign-in complete: epoch={} login={} execution_elapsed_ms={} total_elapsed_ms={}",
                                epoch,
                                result.account.login,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            let message = format!("Signed in to GitHub as {}", result.account.login);
                            this.git_status_message = Some(message.clone());
                            Self::push_success_notification(message, cx);
                            this.refresh_selected_git_workspace_review_summary_after_auth_change(
                                &result.repo,
                                cx,
                            );
                        }
                        Err(err) => {
                            error!("github device sign-in failed: epoch={} err={err:#}", epoch);
                            let summary = err.to_string();
                            this.git_status_message =
                                Some(format!("GitHub sign-in failed: {summary}"));
                            Self::push_error_notification(
                                format!("GitHub sign-in failed: {summary}"),
                                cx,
                            );
                        }
                    }
                    cx.notify();
                });
            }
        });
        Ok(())
    }

    pub(super) fn copy_github_device_flow_code_for_repo(
        &mut self,
        repo: &ForgeRepoRef,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let prompt = self
            .pending_github_device_flow_prompt_for_repo(repo)
            .ok_or_else(|| "No GitHub device sign-in code is currently active.".to_string())?;
        cx.write_to_clipboard(ClipboardItem::new_string(prompt.user_code.clone()));
        let message = format!("Copied GitHub device code {}", prompt.user_code);
        self.git_status_message = Some(message.clone());
        Self::push_success_notification(message, cx);
        cx.notify();
        Ok(())
    }

    pub(super) fn open_github_device_flow_verification_for_repo(
        &mut self,
        repo: &ForgeRepoRef,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let prompt = self
            .pending_github_device_flow_prompt_for_repo(repo)
            .ok_or_else(|| "No GitHub device sign-in session is currently active.".to_string())?;
        open_url_in_browser(prompt.verification_uri.as_str()).map_err(|err| {
            format!("Failed to open the GitHub verification URL: {err}")
        })?;
        let message = "Opened GitHub device verification in the browser.".to_string();
        self.git_status_message = Some(message.clone());
        Self::push_success_notification(message, cx);
        cx.notify();
        Ok(())
    }

    pub(super) fn sign_out_github_session_for_repo(
        &mut self,
        repo: &ForgeRepoRef,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (credential_id, account_label, account_login) = self
            .github_session_identity_for_repo(repo)
            .ok_or_else(|| "No saved GitHub session is available for this repo.".to_string())?;

        let removed_index = self
            .config
            .forge_credentials
            .iter()
            .position(|credential| credential.id == credential_id)
            .ok_or_else(|| "The saved GitHub session could not be found.".to_string())?;
        let removed = self.config.forge_credentials.remove(removed_index);

        self.config
            .forge_repo_credential_bindings
            .retain(|binding| binding.credential_id != credential_id);

        let host_missing_default = !self.config.forge_credentials.iter().any(|credential| {
            credential.provider == removed.provider
                && credential.host == removed.host
                && credential.is_default_for_host
        });
        if removed.is_default_for_host
            && host_missing_default
            && let Some(next_default) = self.config.forge_credentials.iter_mut().find(|credential| {
                credential.provider == removed.provider && credential.host == removed.host
            })
        {
            next_default.is_default_for_host = true;
        }

        self.forge_tokens_by_credential_id
            .remove(credential_id.as_str());
        if self
            .github_device_flow_prompt
            .as_ref()
            .is_some_and(|prompt| &prompt.repo == repo)
        {
            self.github_device_flow_prompt = None;
        }
        self.persist_config();

        if let Err(err) = delete_forge_secret(credential_id.as_str()) {
            warn!(
                "failed to delete saved GitHub session {} during sign-out: {err:#}",
                credential_id
            );
            Self::push_warning_notification(
                "Signed out in Hunk, but the saved GitHub session could not be removed from the system credential store.".to_string(),
                None,
                cx,
            );
        }

        let account_name = account_login
            .filter(|login| !login.trim().is_empty())
            .unwrap_or_else(|| account_label.clone());
        let message = format!("Signed out of GitHub account {}", account_name);
        self.git_status_message = Some(message.clone());
        Self::push_success_notification(message, cx);
        cx.notify();
        Ok(())
    }

    fn remember_github_session_for_repo(
        &mut self,
        repo: &ForgeRepoRef,
        account: &GitHubAuthenticatedAccount,
    ) -> String {
        let has_host_default = self.config.forge_credentials.iter().any(|credential| {
            credential.provider == hunk_domain::config::ReviewProviderKind::GitHub
                && credential.host == repo.host
                && credential.is_default_for_host
        });
        let existing_index = self.config.forge_credentials.iter().position(|credential| {
            credential.provider == hunk_domain::config::ReviewProviderKind::GitHub
                && credential.host == repo.host
                && credential.kind == hunk_domain::config::ForgeCredentialKind::GitHubComSession
                && credential.account_login.as_deref() == Some(account.login.as_str())
        });

        let credential_id = if let Some(index) = existing_index {
            let should_be_default = self.config.forge_credentials[index].is_default_for_host
                || !has_host_default;
            let credential = &mut self.config.forge_credentials[index];
            credential.provider = hunk_domain::config::ReviewProviderKind::GitHub;
            credential.host = repo.host.clone();
            credential.kind = hunk_domain::config::ForgeCredentialKind::GitHubComSession;
            credential.account_label = account.display_label.clone();
            credential.account_login = Some(account.login.clone());
            credential.is_default_for_host = should_be_default;
            credential.id.clone()
        } else {
            let credential_id = next_forge_credential_id(
                ForgeProvider::GitHub,
                repo.host.as_str(),
                account.login.as_str(),
            );
            self.config
                .forge_credentials
                .push(hunk_domain::config::ForgeCredentialConfig {
                    id: credential_id.clone(),
                    provider: hunk_domain::config::ReviewProviderKind::GitHub,
                    host: repo.host.clone(),
                    kind: hunk_domain::config::ForgeCredentialKind::GitHubComSession,
                    account_label: account.display_label.clone(),
                    account_login: Some(account.login.clone()),
                    is_default_for_host: !has_host_default,
                });
            credential_id
        };

        self.config.forge_repo_credential_bindings.retain(|binding| {
            !(binding.provider == hunk_domain::config::ReviewProviderKind::GitHub
                && binding.host == repo.host
                && binding.repo_path == repo.path)
        });
        self.config
            .forge_repo_credential_bindings
            .push(hunk_domain::config::ForgeRepoCredentialBindingConfig {
                provider: hunk_domain::config::ReviewProviderKind::GitHub,
                host: repo.host.clone(),
                repo_path: repo.path.clone(),
                credential_id: credential_id.clone(),
            });
        self.persist_config();
        credential_id
    }
}

fn load_github_oauth_app_config() -> anyhow::Result<GitHubOAuthAppConfig> {
    let client_id = std::env::var(GITHUB_OAUTH_CLIENT_ID_ENV_KEY)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| HUNK_GITHUB_DEVICE_FLOW_CLIENT_ID.to_string());
    GitHubOAuthAppConfig::new(client_id)
}

fn complete_github_device_sign_in(
    repo: ForgeRepoRef,
    auth_service: GitHubDeviceFlowService,
    authorization: GitHubDeviceAuthorization,
) -> anyhow::Result<GitHubDeviceSignInResult> {
    let deadline =
        Instant::now() + std::time::Duration::from_secs(authorization.expires_in_secs.max(1));
    let mut poll_interval_secs = authorization.interval_secs.max(1);

    loop {
        if Instant::now() >= deadline {
            anyhow::bail!("Timed out waiting for GitHub sign-in approval.");
        }

        match auth_service.poll_device_flow_token(authorization.device_code.as_str())? {
            GitHubDeviceFlowPoll::AuthorizationPending => {
                sleep_until_next_poll(deadline, poll_interval_secs);
            }
            GitHubDeviceFlowPoll::SlowDown => {
                poll_interval_secs =
                    poll_interval_secs.saturating_add(GITHUB_DEVICE_FLOW_SLOW_DOWN_SECS);
                sleep_until_next_poll(deadline, poll_interval_secs);
            }
            GitHubDeviceFlowPoll::Complete(token) => {
                let client = GitHubReviewClient::for_repo(&repo, token.access_token.as_str())?;
                let account = client.current_user()?;
                return Ok(GitHubDeviceSignInResult {
                    repo,
                    account,
                    access_token: token.access_token,
                });
            }
            GitHubDeviceFlowPoll::AccessDenied(description) => {
                anyhow::bail!("{description}");
            }
            GitHubDeviceFlowPoll::ExpiredToken => {
                anyhow::bail!("GitHub sign-in expired before approval was completed.");
            }
        }
    }
}

fn sleep_until_next_poll(deadline: Instant, poll_interval_secs: u64) {
    let now = Instant::now();
    if now >= deadline {
        return;
    }

    let remaining = deadline.saturating_duration_since(now);
    let requested = std::time::Duration::from_secs(poll_interval_secs.max(1));
    std::thread::sleep(requested.min(remaining));
}
