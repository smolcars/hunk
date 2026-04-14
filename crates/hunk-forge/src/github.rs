use std::future::Future;

use anyhow::{Context as _, Result, anyhow, bail};
use octocrab::models::{IssueState, pulls::PullRequest};
use octocrab::{Octocrab, OctocrabBuilder, params};

use crate::models::{
    CreateReviewInput, CreateReviewResult, ForgeProvider, ForgeReviewState, OpenReviewQuery,
    OpenReviewSummary,
};

#[derive(Debug)]
pub struct GitHubReviewClient {
    api_base_url: String,
    token: String,
}

impl GitHubReviewClient {
    pub fn new(host: &str, token: &str) -> Result<Self> {
        let host = host.trim().trim_end_matches('/').to_ascii_lowercase();
        if host.is_empty() {
            return Err(anyhow!("github host cannot be empty"));
        }

        let token = token.trim();
        if token.is_empty() {
            return Err(anyhow!("github token cannot be empty"));
        }

        Ok(Self {
            api_base_url: github_api_base_url(host.as_str()),
            token: token.to_string(),
        })
    }

    pub fn find_open_review(&self, query: &OpenReviewQuery) -> Result<Option<OpenReviewSummary>> {
        let (owner, repo_name) = github_owner_and_name(&query.repo)?;
        let head = format!("{owner}:{}", query.source_branch);
        let repo_web_base_url = query.repo.web_base_url.clone();
        let target_branch = query.target_branch.clone();
        let pull_requests = self.run(async move {
            let octocrab = self.build_octocrab()?;
            let pulls = octocrab.pulls(owner, repo_name);
            let mut request = pulls.list().state(params::State::Open).head(head);
            if let Some(base) = target_branch
                .as_ref()
                .filter(|base| !base.trim().is_empty())
            {
                request = request.base(base.as_str());
            }
            request
                .send()
                .await
                .context("failed to query GitHub pull requests")
        })?;
        Ok(pull_requests
            .items
            .into_iter()
            .next()
            .map(|pull_request| map_github_pull_request(repo_web_base_url.as_str(), pull_request)))
    }

    pub fn create_review(&self, input: &CreateReviewInput) -> Result<CreateReviewResult> {
        let (owner, repo_name) = github_owner_and_name(&input.repo)?;
        let repo_web_base_url = input.repo.web_base_url.clone();
        let title = input.title.clone();
        let source_branch = input.source_branch.clone();
        let target_branch = input.target_branch.clone();
        let body = input.body.clone();
        let draft = input.draft;
        let pull_request = self.run(async move {
            let octocrab = self.build_octocrab()?;
            let pulls = octocrab.pulls(owner, repo_name);
            let mut request = pulls.create(title, source_branch, target_branch);
            if let Some(body) = body.as_deref() {
                request = request.body(body);
            }
            request
                .draft(draft)
                .send()
                .await
                .context("failed to create GitHub pull request")
        })?;

        Ok(CreateReviewResult {
            review: map_github_pull_request(repo_web_base_url.as_str(), pull_request),
        })
    }

    fn build_octocrab(&self) -> Result<Octocrab> {
        OctocrabBuilder::default()
            .base_uri(self.api_base_url.as_str())
            .context("failed to configure GitHub API base URI")?
            .personal_token(self.token.clone())
            .build()
            .context("failed to build octocrab GitHub client")
    }

    fn run<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to initialize tokio runtime for GitHub API call")?;
        runtime.block_on(future)
    }
}

fn github_owner_and_name(repo: &crate::models::ForgeRepoRef) -> Result<(&str, &str)> {
    validate_github_repo(repo.provider)?;
    let owner = repo.github_owner()?;
    let repo_name = repo.name.trim();
    if repo_name.is_empty() {
        bail!("github repository name cannot be empty");
    }
    Ok((owner, repo_name))
}

fn validate_github_repo(provider: ForgeProvider) -> Result<()> {
    if provider != ForgeProvider::GitHub {
        bail!("GitHub review client only supports GitHub repositories");
    }
    Ok(())
}

#[doc(hidden)]
pub fn github_api_base_url(host: &str) -> String {
    if host == "github.com" {
        "https://api.github.com".to_string()
    } else {
        format!("https://{host}/api/v3")
    }
}

fn map_github_pull_request(
    repo_web_base_url: &str,
    pull_request: PullRequest,
) -> OpenReviewSummary {
    let state = if pull_request.merged_at.is_some() {
        ForgeReviewState::Merged
    } else {
        match pull_request.state.unwrap_or(IssueState::Open) {
            IssueState::Open => ForgeReviewState::Open,
            IssueState::Closed => ForgeReviewState::Closed,
            _ => ForgeReviewState::Closed,
        }
    };

    OpenReviewSummary {
        provider: ForgeProvider::GitHub,
        number: pull_request.number,
        title: pull_request
            .title
            .unwrap_or_else(|| format!("Pull Request #{}", pull_request.number)),
        url: pull_request
            .html_url
            .map(|url| url.to_string())
            .unwrap_or_else(|| format!("{repo_web_base_url}/pull/{}", pull_request.number)),
        state,
        draft: pull_request.draft.unwrap_or(false),
        source_branch: pull_request.head.ref_field,
        target_branch: pull_request.base.ref_field,
    }
}
