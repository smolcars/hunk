use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use hunk_forge::{
    GitHubAuthMode, GitHubBrowserAuthService, GitHubOAuthAppConfig, github_auth_mode_for_host,
    github_loopback_redirect_url,
};
use url::Url;

#[test]
fn github_auth_mode_prefers_browser_sign_in_for_github_com() {
    assert_eq!(
        github_auth_mode_for_host("github.com"),
        GitHubAuthMode::BrowserSession
    );
    assert_eq!(
        github_auth_mode_for_host("HTTPS://GITHUB.COM/"),
        GitHubAuthMode::BrowserSession
    );
}

#[test]
fn github_auth_mode_uses_pat_for_enterprise_hosts() {
    assert_eq!(
        github_auth_mode_for_host("github.company.com"),
        GitHubAuthMode::PersonalAccessToken
    );
}

#[test]
fn github_oauth_app_config_requires_client_secret() {
    let err =
        GitHubOAuthAppConfig::new("client-id", "").expect_err("empty client secret must fail");

    assert!(err.to_string().contains("client secret"));
}

#[test]
fn github_loopback_redirect_url_uses_callback_path() {
    let redirect = github_loopback_redirect_url(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::LOCALHOST,
        48721,
    )))
    .expect("redirect URL should build");

    assert_eq!(redirect, "http://127.0.0.1:48721/auth/github/callback");
}

#[test]
fn github_browser_auth_request_contains_expected_query_parameters() {
    let service = GitHubBrowserAuthService::new("hunk-client-id").expect("service should build");
    let request = service
        .begin_loopback_auth(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::LOCALHOST,
            40321,
        )))
        .expect("auth request should build");
    let url = Url::parse(request.authorization_url.as_str()).expect("url should parse");
    let query = url.query_pairs().collect::<Vec<_>>();

    assert!(
        url.as_str()
            .starts_with("https://github.com/login/oauth/authorize")
    );
    assert_eq!(
        query
            .iter()
            .find(|(key, _)| key == "client_id")
            .map(|(_, value)| value.as_ref()),
        Some("hunk-client-id")
    );
    assert_eq!(
        query
            .iter()
            .find(|(key, _)| key == "redirect_uri")
            .map(|(_, value)| value.as_ref()),
        Some("http://127.0.0.1:40321/auth/github/callback")
    );
    assert_eq!(
        query
            .iter()
            .find(|(key, _)| key == "scope")
            .map(|(_, value)| value.as_ref()),
        Some("repo read:user")
    );
    assert_eq!(
        query
            .iter()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.as_ref()),
        Some(request.state.as_str())
    );
    assert!(
        query
            .iter()
            .any(|(key, value)| key == "code_challenge" && !value.is_empty())
    );
    assert_eq!(
        query
            .iter()
            .find(|(key, _)| key == "code_challenge_method")
            .map(|(_, value)| value.as_ref()),
        Some("S256")
    );
    assert!(!request.pkce_verifier.is_empty());
}

#[test]
fn github_browser_auth_callback_validation_requires_matching_state() {
    let service = GitHubBrowserAuthService::new("hunk-client-id").expect("service should build");
    let err = service
        .validate_callback_url(
            "http://127.0.0.1:40321/auth/github/callback?code=abc123&state=wrong",
            "expected",
        )
        .expect_err("mismatched state should fail");

    assert!(
        err.to_string().contains("state did not match"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn github_browser_auth_callback_validation_returns_code_on_success() {
    let service = GitHubBrowserAuthService::new("hunk-client-id").expect("service should build");
    let callback = service
        .validate_callback_url(
            "http://127.0.0.1:40321/auth/github/callback?code=abc123&state=expected",
            "expected",
        )
        .expect("callback should validate");

    assert_eq!(callback.code, "abc123");
    assert_eq!(callback.state, "expected");
}
