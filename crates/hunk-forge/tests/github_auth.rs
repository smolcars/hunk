use hunk_forge::{GitHubAuthMode, GitHubOAuthAppConfig, github_auth_mode_for_host};

#[test]
fn github_auth_mode_prefers_device_flow_for_github_com() {
    assert_eq!(
        github_auth_mode_for_host("github.com"),
        GitHubAuthMode::DeviceFlow
    );
    assert_eq!(
        github_auth_mode_for_host("HTTPS://GITHUB.COM/"),
        GitHubAuthMode::DeviceFlow
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
fn github_oauth_app_config_requires_client_id() {
    let err = GitHubOAuthAppConfig::new("").expect_err("empty client id must fail");

    assert!(err.to_string().contains("client id"));
}
