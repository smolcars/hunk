use anyhow::Result;
use hunk_forge::github::github_api_base_url;
use hunk_forge::{CreateReviewInput, ForgeProvider, ForgeRepoRef, GitHubReviewClient};

#[test]
fn github_api_base_url_matches_host_type() {
    assert_eq!(github_api_base_url("github.com"), "https://api.github.com");
    assert_eq!(
        github_api_base_url("github.company.internal"),
        "https://github.company.internal/api/v3"
    );
    assert_eq!(
        github_api_base_url("github.company.internal:8443"),
        "https://github.company.internal:8443/api/v3"
    );
}

#[test]
fn github_client_rejects_empty_token() {
    let err = GitHubReviewClient::new("github.com", "").expect_err("empty tokens must be rejected");
    assert!(err.to_string().contains("token"));
}

#[test]
fn github_client_rejects_non_github_repo_requests() {
    let client = GitHubReviewClient::new("github.com", "token").expect("client");
    let err = client
        .create_review(&CreateReviewInput {
            repo: ForgeRepoRef {
                provider: ForgeProvider::GitLab,
                host: "gitlab.com".to_string(),
                authority: "gitlab.com".to_string(),
                namespace: "example-group".to_string(),
                name: "hunk".to_string(),
                path: "example-group/hunk".to_string(),
                web_base_url: "https://gitlab.com/example-group/hunk".to_string(),
            },
            source_branch: "feature/forge".to_string(),
            source_head_owner: None,
            target_branch: "main".to_string(),
            title: "Forge PR".to_string(),
            body: Some("Body".to_string()),
            draft: true,
        })
        .expect_err("non-GitHub repos must be rejected");
    assert!(err.to_string().contains("GitHub"));
}

#[test]
fn github_repo_helper_exposes_owner() -> Result<()> {
    let repo = github_repo();
    assert_eq!(repo.github_owner()?, "example-org");
    Ok(())
}

fn github_repo() -> ForgeRepoRef {
    ForgeRepoRef {
        provider: ForgeProvider::GitHub,
        host: "github.com".to_string(),
        authority: "github.com".to_string(),
        namespace: "example-org".to_string(),
        name: "hunk".to_string(),
        path: "example-org/hunk".to_string(),
        web_base_url: "https://github.com/example-org/hunk".to_string(),
    }
}
