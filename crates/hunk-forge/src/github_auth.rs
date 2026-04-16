use anyhow::{Context as _, Result, bail};
use reqwest::header::ACCEPT;
use serde::Deserialize;

const GITHUB_COM_HOST: &str = "github.com";
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const GITHUB_ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const GITHUB_DEVICE_CODE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
const HUNK_GITHUB_AUTH_USER_AGENT: &str = "Hunk";
const DEFAULT_DEVICE_POLL_INTERVAL_SECS: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitHubAuthMode {
    DeviceFlow,
    PersonalAccessToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubOAuthAppConfig {
    pub client_id: String,
}

impl GitHubOAuthAppConfig {
    pub fn new(client_id: impl Into<String>) -> Result<Self> {
        let client_id = client_id.into().trim().to_string();
        if client_id.is_empty() {
            bail!("github oauth app client id cannot be empty");
        }

        Ok(Self { client_id })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubAuthScopes(Vec<String>);

impl GitHubAuthScopes {
    pub fn for_pull_request_workflows() -> Self {
        Self(vec!["repo".to_string(), "read:user".to_string()])
    }

    pub fn as_space_delimited(&self) -> String {
        self.0.join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubDeviceAuthorization {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in_secs: u64,
    pub interval_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubOAuthToken {
    pub access_token: String,
    pub token_type: String,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHubDeviceFlowPoll {
    AuthorizationPending,
    SlowDown,
    Complete(GitHubOAuthToken),
    AccessDenied(String),
    ExpiredToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubDeviceFlowService {
    client_id: String,
    scopes: GitHubAuthScopes,
}

impl GitHubDeviceFlowService {
    pub fn new(client_id: impl Into<String>) -> Result<Self> {
        let client_id = client_id.into().trim().to_string();
        if client_id.is_empty() {
            bail!("github device flow client id cannot be empty");
        }
        Ok(Self {
            client_id,
            scopes: GitHubAuthScopes::for_pull_request_workflows(),
        })
    }

    pub fn start_device_flow(&self) -> Result<GitHubDeviceAuthorization> {
        let client = build_github_auth_http_client()?;
        let response = client
            .post(GITHUB_DEVICE_CODE_URL)
            .header(ACCEPT, "application/json")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("scope", self.scopes.as_space_delimited().as_str()),
            ])
            .send()
            .context("failed to start GitHub device authorization flow")?;
        let status = response.status();
        let payload = response
            .json::<GitHubDeviceAuthorizationResponse>()
            .context("failed to decode GitHub device authorization response")?;
        if !status.is_success() {
            bail!("github device authorization failed with status {status}");
        }

        let device_code = payload.device_code.trim().to_string();
        if device_code.is_empty() {
            bail!("github device authorization response was missing a device code");
        }

        let user_code = payload.user_code.trim().to_string();
        if user_code.is_empty() {
            bail!("github device authorization response was missing a user code");
        }

        let verification_uri = payload.verification_uri.trim().to_string();
        if verification_uri.is_empty() {
            bail!("github device authorization response was missing a verification URI");
        }

        if payload.expires_in <= 0 {
            bail!("github device authorization response returned an invalid expiration");
        }

        Ok(GitHubDeviceAuthorization {
            device_code,
            user_code,
            verification_uri,
            expires_in_secs: payload.expires_in as u64,
            interval_secs: payload.interval.max(1) as u64,
        })
    }

    pub fn poll_device_flow_token(&self, device_code: &str) -> Result<GitHubDeviceFlowPoll> {
        let device_code = device_code.trim();
        if device_code.is_empty() {
            bail!("github device code cannot be empty");
        }

        let client = build_github_auth_http_client()?;
        let response = client
            .post(GITHUB_ACCESS_TOKEN_URL)
            .header(ACCEPT, "application/json")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("device_code", device_code),
                ("grant_type", GITHUB_DEVICE_CODE_GRANT_TYPE),
            ])
            .send()
            .context("failed to poll GitHub device authorization token")?;
        let status = response.status();
        let payload = response
            .json::<GitHubDeviceTokenResponse>()
            .context("failed to decode GitHub device token response")?;

        if let Some(access_token) = payload
            .access_token
            .as_ref()
            .map(|token| token.trim())
            .filter(|token| !token.is_empty())
        {
            let token_type = payload
                .token_type
                .as_ref()
                .map(|token_type| token_type.trim())
                .filter(|token_type| !token_type.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("github device token response was missing a token type")
                })?;

            return Ok(GitHubDeviceFlowPoll::Complete(GitHubOAuthToken {
                access_token: access_token.to_string(),
                token_type: token_type.to_string(),
                scope: payload.scope.filter(|scope| !scope.trim().is_empty()),
            }));
        }

        match payload.error.as_deref() {
            Some("authorization_pending") => Ok(GitHubDeviceFlowPoll::AuthorizationPending),
            Some("slow_down") => Ok(GitHubDeviceFlowPoll::SlowDown),
            Some("access_denied") => Ok(GitHubDeviceFlowPoll::AccessDenied(
                payload
                    .error_description
                    .filter(|description| !description.trim().is_empty())
                    .unwrap_or_else(|| "GitHub sign-in was denied.".to_string()),
            )),
            Some("expired_token") => Ok(GitHubDeviceFlowPoll::ExpiredToken),
            Some(error) => {
                let description = payload
                    .error_description
                    .filter(|description| !description.trim().is_empty())
                    .unwrap_or_else(|| error.to_string());
                bail!("github device token polling failed: {description}");
            }
            None if !status.is_success() => {
                bail!("github device token polling failed with status {status}");
            }
            None => bail!("github device token response was missing both token and error data"),
        }
    }
}

pub fn github_auth_mode_for_host(host: &str) -> GitHubAuthMode {
    if normalize_host(host) == GITHUB_COM_HOST {
        GitHubAuthMode::DeviceFlow
    } else {
        GitHubAuthMode::PersonalAccessToken
    }
}

fn build_github_auth_http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(HUNK_GITHUB_AUTH_USER_AGENT)
        .build()
        .context("failed to build GitHub auth HTTP client")
}

fn normalize_host(host: &str) -> String {
    let normalized = host.trim().to_ascii_lowercase();
    let without_scheme = normalized
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    without_scheme.trim_end_matches('/').to_ascii_lowercase()
}

#[derive(Debug, Deserialize)]
struct GitHubDeviceAuthorizationResponse {
    #[serde(default)]
    device_code: String,
    #[serde(default)]
    user_code: String,
    #[serde(default)]
    verification_uri: String,
    expires_in: i64,
    #[serde(default = "default_device_poll_interval_secs")]
    interval: i64,
}

#[derive(Debug, Deserialize)]
struct GitHubDeviceTokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

const fn default_device_poll_interval_secs() -> i64 {
    DEFAULT_DEVICE_POLL_INTERVAL_SECS as i64
}
