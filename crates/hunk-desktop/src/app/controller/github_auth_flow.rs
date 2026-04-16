use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use hunk_forge::{
    ForgeProvider, GitHubAuthMode, GitHubAuthenticatedAccount, GitHubBrowserAuthRequest,
    GitHubBrowserAuthService, GitHubOAuthAppConfig, github_auth_mode_for_host,
};

const GITHUB_OAUTH_CLIENT_ID_ENV_KEY: &str = "HUNK_GITHUB_OAUTH_CLIENT_ID";
const GITHUB_OAUTH_CLIENT_SECRET_ENV_KEY: &str = "HUNK_GITHUB_OAUTH_CLIENT_SECRET";
const GITHUB_OAUTH_CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);
const GITHUB_OAUTH_CALLBACK_POLL_INTERVAL: Duration = Duration::from_millis(200);
const GITHUB_OAUTH_RESPONSE_OK: &str = concat!(
    "HTTP/1.1 200 OK\r\n",
    "Content-Type: text/html; charset=utf-8\r\n",
    "Connection: close\r\n\r\n",
    "<html><body><h2>GitHub sign-in received.</h2>",
    "<p>You can return to Hunk.</p></body></html>"
);
const GITHUB_OAUTH_RESPONSE_BAD_REQUEST: &str = concat!(
    "HTTP/1.1 400 Bad Request\r\n",
    "Content-Type: text/html; charset=utf-8\r\n",
    "Connection: close\r\n\r\n",
    "<html><body><h2>GitHub sign-in failed.</h2>",
    "<p>You can close this tab and try again from Hunk.</p></body></html>"
);

#[derive(Debug, Clone)]
struct GitHubBrowserSignInResult {
    repo: ForgeRepoRef,
    account: GitHubAuthenticatedAccount,
    access_token: String,
}

impl DiffViewer {
    fn github_browser_sign_in_available_for_repo(&self, repo: &ForgeRepoRef) -> bool {
        github_auth_mode_for_host(repo.host.as_str()) == GitHubAuthMode::BrowserSession
            && load_github_oauth_app_config().is_ok()
    }

    fn github_browser_sign_in_hint_for_repo(&self, repo: &ForgeRepoRef) -> String {
        match github_auth_mode_for_host(repo.host.as_str()) {
            GitHubAuthMode::BrowserSession => match load_github_oauth_app_config() {
                Ok(_) => {
                    "Sign in with GitHub to save a reusable session for this repo. A personal access token still works as a fallback."
                        .to_string()
                }
                Err(_) => format!(
                    "Browser sign-in is not configured in this build. Set {GITHUB_OAUTH_CLIENT_ID_ENV_KEY} and {GITHUB_OAUTH_CLIENT_SECRET_ENV_KEY}, or use a personal access token."
                ),
            },
            GitHubAuthMode::PersonalAccessToken => {
                "This GitHub host currently uses a personal access token.".to_string()
            }
        }
    }

    fn start_github_browser_sign_in(
        &mut self,
        repo: ForgeRepoRef,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if github_auth_mode_for_host(repo.host.as_str()) != GitHubAuthMode::BrowserSession {
            return Err("Browser sign-in is only available for github.com right now.".to_string());
        }

        let app_config = load_github_oauth_app_config().map_err(|err| err.to_string())?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|err| {
            format!("Failed to bind a local callback listener for GitHub sign-in: {err}")
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|err| format!("Failed to configure GitHub callback listener: {err}"))?;
        let listen_addr = listener
            .local_addr()
            .map_err(|err| format!("Failed to inspect GitHub callback listener: {err}"))?;
        let auth_service = GitHubBrowserAuthService::new(app_config.client_id.clone())
            .map_err(|err| err.to_string())?;
        let auth_request = auth_service
            .begin_loopback_auth(listen_addr)
            .map_err(|err| err.to_string())?;

        open_url_in_browser(auth_request.authorization_url.as_str()).map_err(|err| {
            format!(
                "Failed to open the browser for GitHub sign-in: {}",
                err
            )
        })?;

        let epoch = self.begin_git_action("GitHub sign-in".to_string(), cx);
        let started_at = Instant::now();
        self.git_status_message = Some("Waiting for GitHub sign-in in the browser…".to_string());
        Self::push_success_notification(
            "Opened the browser for GitHub sign-in.".to_string(),
            cx,
        );
        cx.notify();

        self.git_action_task = cx.spawn(async move |this, cx| {
            let (execution_elapsed, result) = cx
                .background_executor()
                .spawn(async move {
                    let execution_started_at = Instant::now();
                    let result = complete_github_browser_sign_in(
                        repo,
                        listener,
                        auth_service,
                        auth_request,
                        app_config,
                    );
                    (execution_started_at.elapsed(), result)
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    let total_elapsed = started_at.elapsed();
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
                                    "failed to save GitHub browser session {}: {err:#}",
                                    credential_id
                                );
                                Self::push_warning_notification(
                                    "GitHub session could not be saved to the system credential store. It will only be available in this Hunk session.".to_string(),
                                    None,
                                    cx,
                                );
                            }
                            debug!(
                                "github sign-in complete: epoch={} login={} execution_elapsed_ms={} total_elapsed_ms={}",
                                epoch,
                                result.account.login,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            let message =
                                format!("Signed in to GitHub as {}", result.account.login);
                            this.git_status_message = Some(message.clone());
                            Self::push_success_notification(message, cx);
                        }
                        Err(err) => {
                            error!("github sign-in failed: epoch={} err={err:#}", epoch);
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
    let client_id = std::env::var(GITHUB_OAUTH_CLIENT_ID_ENV_KEY).with_context(|| {
        format!("{GITHUB_OAUTH_CLIENT_ID_ENV_KEY} is required for GitHub browser sign-in")
    })?;
    let client_secret = std::env::var(GITHUB_OAUTH_CLIENT_SECRET_ENV_KEY).with_context(|| {
        format!("{GITHUB_OAUTH_CLIENT_SECRET_ENV_KEY} is required for GitHub browser sign-in")
    })?;
    GitHubOAuthAppConfig::new(client_id, client_secret)
}

fn complete_github_browser_sign_in(
    repo: ForgeRepoRef,
    listener: TcpListener,
    auth_service: GitHubBrowserAuthService,
    auth_request: GitHubBrowserAuthRequest,
    app_config: GitHubOAuthAppConfig,
) -> anyhow::Result<GitHubBrowserSignInResult> {
    let callback_url =
        wait_for_github_browser_callback(listener, GITHUB_OAUTH_CALLBACK_TIMEOUT)?;
    let callback = auth_service.validate_callback_url(
        callback_url.as_str(),
        auth_request.state.as_str(),
    )?;
    let token = auth_service.exchange_code_for_token(
        &app_config,
        &callback,
        auth_request.redirect_url.as_str(),
        auth_request.pkce_verifier.as_str(),
    )?;
    let client = GitHubReviewClient::new(repo.host.as_str(), token.access_token.as_str())?;
    let account = client.current_user()?;
    Ok(GitHubBrowserSignInResult {
        repo,
        account,
        access_token: token.access_token,
    })
}

fn wait_for_github_browser_callback(
    listener: TcpListener,
    timeout: Duration,
) -> anyhow::Result<String> {
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((stream, _)) => return read_github_browser_callback(stream),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    anyhow::bail!("Timed out waiting for the GitHub sign-in callback.");
                }
                std::thread::sleep(GITHUB_OAUTH_CALLBACK_POLL_INTERVAL);
            }
            Err(err) => {
                return Err(err).context("failed while waiting for the GitHub sign-in callback");
            }
        }
    }
}

fn read_github_browser_callback(mut stream: TcpStream) -> anyhow::Result<String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .context("failed to set GitHub callback read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .context("failed to set GitHub callback write timeout")?;

    let request = read_http_request(&mut stream)?;
    let callback_url = match parse_github_callback_url(
        request.as_str(),
        stream
            .local_addr()
            .context("failed to read local GitHub callback address")?,
    ) {
        Ok(callback_url) => {
            let _ = stream.write_all(GITHUB_OAUTH_RESPONSE_OK.as_bytes());
            callback_url
        }
        Err(err) => {
            let _ = stream.write_all(GITHUB_OAUTH_RESPONSE_BAD_REQUEST.as_bytes());
            return Err(err);
        }
    };

    Ok(callback_url)
}

fn read_http_request(stream: &mut TcpStream) -> anyhow::Result<String> {
    let mut request = Vec::with_capacity(2048);
    let mut buffer = [0_u8; 1024];
    loop {
        let read = stream
            .read(&mut buffer)
            .context("failed to read the GitHub callback request")?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if request.len() >= 16 * 1024 {
            anyhow::bail!("github callback request was unexpectedly large");
        }
    }

    if request.is_empty() {
        anyhow::bail!("github callback request was empty");
    }

    String::from_utf8(request).context("github callback request was not valid UTF-8")
}

fn parse_github_callback_url(request: &str, local_addr: std::net::SocketAddr) -> anyhow::Result<String> {
    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("github callback request was missing a request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("github callback request was missing a method"))?;
    if method != "GET" {
        anyhow::bail!("github callback request used an unsupported method");
    }
    let target = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("github callback request was missing a target"))?;
    if !target.starts_with("/auth/github/callback") {
        anyhow::bail!("github callback request used an unexpected path");
    }

    Ok(format!("http://{local_addr}{target}"))
}
