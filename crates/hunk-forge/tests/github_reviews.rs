use anyhow::Result;
use hunk_forge::github::{GitHubCreatePullRequest, github_api_base_url};
use hunk_forge::{CreateReviewInput, ForgeProvider, ForgeRepoRef, GitHubReviewClient};

#[test]
fn github_api_base_url_matches_host_type() {
    assert_eq!(github_api_base_url("github.com"), "https://api.github.com");
    assert_eq!(
        github_api_base_url("github.company.internal"),
        "https://github.company.internal/api/v3"
    );
}

#[test]
fn create_pull_request_maps_input_fields() {
    let input = CreateReviewInput {
        repo: github_repo(),
        source_branch: "feature/forge".to_string(),
        target_branch: "main".to_string(),
        title: "Forge PR".to_string(),
        body: Some("Body".to_string()),
        draft: true,
    };

    let request = GitHubCreatePullRequest::from_input(&input);
    assert_eq!(
        request,
        GitHubCreatePullRequest {
            title: "Forge PR".to_string(),
            head: "feature/forge".to_string(),
            base: "main".to_string(),
            body: Some("Body".to_string()),
            draft: true,
        }
    );
}

#[test]
fn github_client_rejects_empty_token() {
    let err = GitHubReviewClient::new("github.com", "").expect_err("empty tokens must be rejected");
    assert!(err.to_string().contains("token"));
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
        namespace: "example-org".to_string(),
        name: "hunk".to_string(),
        path: "example-org/hunk".to_string(),
        web_base_url: "https://github.com/example-org/hunk".to_string(),
    }
}
