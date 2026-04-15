use std::net::SocketAddr;

use anyhow::{Context as _, Result, anyhow, bail};
use oauth2::{CsrfToken, PkceCodeChallenge, PkceCodeVerifier};
use url::Url;

const GITHUB_COM_HOST: &str = "github.com";
const GITHUB_AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitHubAuthMode {
    BrowserSession,
    PersonalAccessToken,
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
pub struct GitHubBrowserAuthRequest {
    pub authorization_url: String,
    pub redirect_url: String,
    pub listen_addr: SocketAddr,
    pub state: String,
    pub pkce_verifier: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubBrowserAuthCallback {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubBrowserAuthService {
    client_id: String,
    scopes: GitHubAuthScopes,
}

impl GitHubBrowserAuthService {
    pub fn new(client_id: impl Into<String>) -> Result<Self> {
        let client_id = client_id.into().trim().to_string();
        if client_id.is_empty() {
            bail!("github browser auth client id cannot be empty");
        }
        Ok(Self {
            client_id,
            scopes: GitHubAuthScopes::for_pull_request_workflows(),
        })
    }

    pub fn begin_loopback_auth(&self, listen_addr: SocketAddr) -> Result<GitHubBrowserAuthRequest> {
        let redirect_url = github_loopback_redirect_url(listen_addr)?;
        let state = CsrfToken::new_random();
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let mut authorization_url = Url::parse(GITHUB_AUTHORIZE_URL)
            .context("failed to construct GitHub authorization URL")?;
        authorization_url
            .query_pairs_mut()
            .append_pair("client_id", self.client_id.as_str())
            .append_pair("redirect_uri", redirect_url.as_str())
            .append_pair("scope", self.scopes.as_space_delimited().as_str())
            .append_pair("state", state.secret())
            .append_pair("code_challenge", pkce_challenge.as_str())
            .append_pair("code_challenge_method", "S256")
            .append_pair("allow_signup", "false");

        Ok(GitHubBrowserAuthRequest {
            authorization_url: authorization_url.into(),
            redirect_url,
            listen_addr,
            state: state.secret().to_string(),
            pkce_verifier: pkce_verifier.secret().to_string(),
        })
    }

    pub fn validate_callback_url(
        &self,
        callback_url: &str,
        expected_state: &str,
    ) -> Result<GitHubBrowserAuthCallback> {
        if expected_state.trim().is_empty() {
            bail!("expected GitHub auth state cannot be empty");
        }

        let parsed = Url::parse(callback_url).context("failed to parse GitHub callback URL")?;
        let query = parsed.query_pairs().collect::<Vec<_>>();
        if let Some((_, error)) = query.iter().find(|(key, _)| key == "error") {
            let description = query
                .iter()
                .find(|(key, _)| key == "error_description")
                .map(|(_, value)| value.to_string())
                .unwrap_or_else(|| error.to_string());
            bail!("github authorization failed: {description}");
        }

        let state = query
            .iter()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.to_string())
            .ok_or_else(|| anyhow!("github callback is missing state"))?;
        if state != expected_state {
            bail!("github callback state did not match the pending sign-in request");
        }

        let code = query
            .iter()
            .find(|(key, _)| key == "code")
            .map(|(_, value)| value.to_string())
            .ok_or_else(|| anyhow!("github callback is missing authorization code"))?;

        Ok(GitHubBrowserAuthCallback { code, state })
    }

    pub fn build_pkce_verifier(&self, secret: &str) -> Result<PkceCodeVerifier> {
        let secret = secret.trim();
        if secret.is_empty() {
            bail!("github PKCE verifier cannot be empty");
        }
        Ok(PkceCodeVerifier::new(secret.to_string()))
    }
}

pub fn github_auth_mode_for_host(host: &str) -> GitHubAuthMode {
    if normalize_host(host) == GITHUB_COM_HOST {
        GitHubAuthMode::BrowserSession
    } else {
        GitHubAuthMode::PersonalAccessToken
    }
}

pub fn github_loopback_redirect_url(listen_addr: SocketAddr) -> Result<String> {
    if listen_addr.ip().is_unspecified() {
        bail!("github loopback redirect cannot use an unspecified address");
    }
    Ok(format!("http://{listen_addr}/auth/github/callback"))
}

fn normalize_host(host: &str) -> String {
    let trimmed = host.trim();
    let without_scheme = trimmed
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    without_scheme.trim_end_matches('/').to_ascii_lowercase()
}
