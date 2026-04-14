use anyhow::{Context as _, Result, anyhow, bail};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use serde::Deserialize;
use serde::Serialize;

use crate::models::{
    CreateReviewInput, CreateReviewResult, ForgeProvider, ForgeReviewState, OpenReviewQuery,
    OpenReviewSummary,
};

const GITHUB_API_VERSION_VALUE: &str = "2022-11-28";
const USER_AGENT_VALUE: &str = "hunk-forge";
const GITHUB_API_VERSION_HEADER: &str = "x-github-api-version";

#[derive(Debug, Clone)]
pub struct GitHubReviewClient {
    api_base_url: String,
    http: Client,
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

        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            HeaderName::from_static(GITHUB_API_VERSION_HEADER),
            HeaderValue::from_static(GITHUB_API_VERSION_VALUE),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(format!("Bearer {token}").as_str())
                .context("failed to build GitHub authorization header")?,
        );

        let http = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build GitHub HTTP client")?;

        Ok(Self {
            api_base_url: github_api_base_url(host.as_str()),
            http,
        })
    }

    pub fn find_open_review(&self, query: &OpenReviewQuery) -> Result<Option<OpenReviewSummary>> {
        validate_github_repo(query.repo.provider)?;
        let owner = query.repo.github_owner()?;
        let endpoint = format!("{}/repos/{}/pulls", self.api_base_url, query.repo.path);
        let head = format!("{owner}:{}", query.source_branch);
        let mut request = self
            .http
            .get(endpoint)
            .query(&[("state", "open"), ("head", head.as_str())]);
        if let Some(base) = query
            .target_branch
            .as_ref()
            .filter(|base| !base.trim().is_empty())
        {
            request = request.query(&[("base", base.as_str())]);
        }
        let response = request
            .send()
            .context("failed to query GitHub pull requests")?
            .error_for_status()
            .context("GitHub pull request lookup failed")?;
        let pull_requests = response
            .json::<Vec<GitHubPullRequest>>()
            .context("failed to decode GitHub pull request response")?;
        Ok(pull_requests
            .into_iter()
            .next()
            .map(map_github_pull_request))
    }

    pub fn create_review(&self, input: &CreateReviewInput) -> Result<CreateReviewResult> {
        validate_github_repo(input.repo.provider)?;
        let endpoint = format!("{}/repos/{}/pulls", self.api_base_url, input.repo.path);
        let request = GitHubCreatePullRequest::from_input(input);
        let response = self
            .http
            .post(endpoint)
            .json(&request)
            .send()
            .context("failed to create GitHub pull request")?
            .error_for_status()
            .context("GitHub pull request creation failed")?;
        let pull_request = response
            .json::<GitHubPullRequest>()
            .context("failed to decode created GitHub pull request")?;
        Ok(CreateReviewResult {
            review: map_github_pull_request(pull_request),
        })
    }
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

fn map_github_pull_request(pull_request: GitHubPullRequest) -> OpenReviewSummary {
    let state = if pull_request.merged_at.is_some() {
        ForgeReviewState::Merged
    } else if pull_request.state.eq_ignore_ascii_case("open") {
        ForgeReviewState::Open
    } else {
        ForgeReviewState::Closed
    };

    OpenReviewSummary {
        provider: ForgeProvider::GitHub,
        number: pull_request.number,
        title: pull_request.title,
        url: pull_request.html_url,
        state,
        draft: pull_request.draft,
        source_branch: pull_request.head.r#ref,
        target_branch: pull_request.base.r#ref,
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[doc(hidden)]
pub struct GitHubCreatePullRequest {
    pub title: String,
    pub head: String,
    pub base: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub draft: bool,
}

impl GitHubCreatePullRequest {
    pub fn from_input(input: &CreateReviewInput) -> Self {
        Self {
            title: input.title.clone(),
            head: input.source_branch.clone(),
            base: input.target_branch.clone(),
            body: input.body.clone(),
            draft: input.draft,
        }
    }
}

#[derive(Debug, Deserialize)]
struct GitHubPullRequest {
    number: u64,
    html_url: String,
    title: String,
    state: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    merged_at: Option<String>,
    head: GitHubBranchRef,
    base: GitHubBranchRef,
}

#[derive(Debug, Deserialize)]
struct GitHubBranchRef {
    #[serde(rename = "ref")]
    r#ref: String,
}
