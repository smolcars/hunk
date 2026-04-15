use hunk_forge::{
    ForgeCredentialKind, ForgeCredentialMetadata, ForgeCredentialResolution, ForgeProvider,
    ForgeRepoCredentialBinding, ForgeRepoRef, resolve_credential_for_repo,
};

fn github_repo(path: &str) -> ForgeRepoRef {
    let mut segments = path.split('/').collect::<Vec<_>>();
    let name = segments.pop().expect("repo name").to_string();
    ForgeRepoRef {
        provider: ForgeProvider::GitHub,
        host: "github.com".to_string(),
        namespace: segments.join("/"),
        name,
        path: path.to_string(),
        web_base_url: format!("https://github.com/{path}"),
    }
}

fn github_credential(
    id: &str,
    host: &str,
    account_label: &str,
    is_default_for_host: bool,
) -> ForgeCredentialMetadata {
    ForgeCredentialMetadata {
        id: id.to_string(),
        provider: ForgeProvider::GitHub,
        host: host.to_string(),
        kind: ForgeCredentialKind::PersonalAccessToken,
        account_label: account_label.to_string(),
        account_login: None,
        is_default_for_host,
    }
}

#[test]
fn repo_binding_takes_priority_over_host_default() {
    let repo = github_repo("smolcars/hunk");
    let resolved = resolve_credential_for_repo(
        &repo,
        &[
            github_credential("default", "github.com", "personal", true),
            github_credential("work", "github.com", "work", false),
        ],
        &[ForgeRepoCredentialBinding {
            provider: ForgeProvider::GitHub,
            host: "github.com".to_string(),
            repo_path: "smolcars/hunk".to_string(),
            credential_id: "work".to_string(),
        }],
    )
    .expect("repo binding should resolve");

    assert_eq!(resolved.credential_id, "work");
    assert_eq!(resolved.resolution, ForgeCredentialResolution::RepoBinding);
}

#[test]
fn host_default_resolves_when_repo_is_unbound() {
    let repo = github_repo("smolcars/hunk");
    let resolved = resolve_credential_for_repo(
        &repo,
        &[
            github_credential("personal", "github.com", "personal", true),
            github_credential("work", "github.com", "work", false),
        ],
        &[],
    )
    .expect("host default should resolve");

    assert_eq!(resolved.credential_id, "personal");
    assert_eq!(resolved.resolution, ForgeCredentialResolution::HostDefault);
}

#[test]
fn single_host_credential_resolves_without_default() {
    let repo = github_repo("smolcars/hunk");
    let resolved = resolve_credential_for_repo(
        &repo,
        &[github_credential("only", "github.com", "personal", false)],
        &[],
    )
    .expect("single host credential should resolve");

    assert_eq!(resolved.credential_id, "only");
    assert_eq!(
        resolved.resolution,
        ForgeCredentialResolution::SingleHostCredential
    );
}

#[test]
fn ambiguous_host_credentials_require_explicit_selection() {
    let repo = github_repo("smolcars/hunk");
    let resolved = resolve_credential_for_repo(
        &repo,
        &[
            github_credential("personal", "github.com", "personal", false),
            github_credential("work", "github.com", "work", false),
        ],
        &[],
    );

    assert_eq!(resolved, None);
}

#[test]
fn stale_repo_binding_is_ignored() {
    let repo = github_repo("smolcars/hunk");
    let resolved = resolve_credential_for_repo(
        &repo,
        &[github_credential(
            "personal",
            "github.com",
            "personal",
            true,
        )],
        &[ForgeRepoCredentialBinding {
            provider: ForgeProvider::GitHub,
            host: "github.com".to_string(),
            repo_path: "smolcars/hunk".to_string(),
            credential_id: "missing".to_string(),
        }],
    )
    .expect("host default should be used when binding is stale");

    assert_eq!(resolved.credential_id, "personal");
    assert_eq!(resolved.resolution, ForgeCredentialResolution::HostDefault);
}
