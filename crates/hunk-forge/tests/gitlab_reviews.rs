use hunk_forge::{ForgeProvider, ForgeRepoRef, GitLabReviewClient};

#[test]
fn gitlab_client_rejects_empty_token() {
    let err =
        GitLabReviewClient::new(&gitlab_repo(), "").expect_err("empty tokens must be rejected");
    assert!(err.to_string().contains("token"));
}

#[test]
fn gitlab_client_rejects_non_gitlab_repo_requests() {
    let err = GitLabReviewClient::new(&github_repo(), "token")
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
