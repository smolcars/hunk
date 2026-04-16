use anyhow::Result;
use hunk_forge::{ForgeProvider, ForgeRepoRef};
use hunk_git::branch::ReviewRemote;
use hunk_git::config::ReviewProviderKind;

#[test]
fn github_review_remote_maps_to_forge_repo() -> Result<()> {
    let remote = ReviewRemote {
        provider: ReviewProviderKind::GitHub,
        host: "github.com".to_string(),
        authority: "github.com".to_string(),
        repository_path: "example-org/hunk".to_string(),
        base_url: "https://github.com/example-org/hunk".to_string(),
    };

    let repo = ForgeRepoRef::try_from(&remote)?;
    assert_eq!(repo.provider, ForgeProvider::GitHub);
    assert_eq!(repo.namespace, "example-org");
    assert_eq!(repo.name, "hunk");
    assert_eq!(repo.authority, "github.com");
    assert_eq!(repo.path, "example-org/hunk");
    Ok(())
}

#[test]
fn gitlab_review_remote_supports_nested_groups() -> Result<()> {
    let remote = ReviewRemote {
        provider: ReviewProviderKind::GitLab,
        host: "gitlab.company.internal".to_string(),
        authority: "gitlab.company.internal:8443".to_string(),
        repository_path: "platform/desktop/hunk".to_string(),
        base_url: "https://gitlab.company.internal:8443/platform/desktop/hunk".to_string(),
    };

    let repo = ForgeRepoRef::try_from(&remote)?;
    assert_eq!(repo.provider, ForgeProvider::GitLab);
    assert_eq!(repo.namespace, "platform/desktop");
    assert_eq!(repo.name, "hunk");
    assert_eq!(repo.authority, "gitlab.company.internal:8443");
    Ok(())
}

#[test]
fn github_review_remote_rejects_non_owner_repo_shape() {
    let remote = ReviewRemote {
        provider: ReviewProviderKind::GitHub,
        host: "github.company.internal".to_string(),
        authority: "github.company.internal".to_string(),
        repository_path: "platform/desktop/hunk".to_string(),
        base_url: "https://github.company.internal/platform/desktop/hunk".to_string(),
    };

    let err = ForgeRepoRef::try_from(&remote).expect_err("github path must be owner/repo");
    assert!(err.to_string().contains("exactly two segments"));
}
