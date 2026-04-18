use hunk_forge::{CreateReviewInput, ForgeProvider, ForgeRepoRef, GitLabReviewClient};

#[test]
fn gitlab_client_rejects_empty_token() {
    let err =
        GitLabReviewClient::new(&gitlab_repo(), "").expect_err("empty tokens must be rejected");
    assert!(err.to_string().contains("token"));
}

#[test]
fn gitlab_client_rejects_non_gitlab_repo_requests() {
    let client = GitLabReviewClient::new(&gitlab_repo(), "token").expect("client");
    let err = client
        .create_review(&CreateReviewInput {
            base_repo: github_repo(),
            head_repo: gitlab_repo(),
            source_branch: "feature/forge".to_string(),
            target_branch: "main".to_string(),
            title: "Forge MR".to_string(),
            body: Some("Body".to_string()),
            draft: false,
        })
        .expect_err("non-GitLab repos must be rejected");
    assert!(err.to_string().contains("GitLab"));
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

fn gitlab_repo() -> ForgeRepoRef {
    ForgeRepoRef {
        provider: ForgeProvider::GitLab,
        host: "gitlab.com".to_string(),
        authority: "gitlab.com".to_string(),
        namespace: "example-group".to_string(),
        name: "hunk".to_string(),
        path: "example-group/hunk".to_string(),
        web_base_url: "https://gitlab.com/example-group/hunk".to_string(),
    }
}
