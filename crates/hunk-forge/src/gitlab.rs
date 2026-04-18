use anyhow::{Context as _, Result, anyhow, bail};
use gitlab::GitlabBuilder;
use gitlab::api::Query as _;
use gitlab::api::merge_requests::{MergeRequestScope, MergeRequestState};
use gitlab::api::projects::Project;
use gitlab::api::projects::merge_requests::{CreateMergeRequest, MergeRequests};
use serde::Deserialize;

use crate::models::{
    CreateReviewInput, CreateReviewResult, ForgeProvider, ForgeRepoRef, ForgeReviewState,
    OpenReviewQuery, OpenReviewSummary,
};

#[derive(Debug)]
pub struct GitLabReviewClient {
    client: gitlab::Gitlab,
}

impl GitLabReviewClient {
    pub fn new(repo: &ForgeRepoRef, token: &str) -> Result<Self> {
        validate_gitlab_repo(repo.provider)?;
        let token = token.trim();
        if token.is_empty() {
            return Err(anyhow!("gitlab token cannot be empty"));
        }

        let mut builder = GitlabBuilder::new(repo.authority.as_str(), token.to_string());
        if repo.web_base_url.starts_with("http://") {
            builder.insecure();
        }

        let client = builder.build().context("failed to build GitLab client")?;
        Ok(Self { client })
    }

    pub fn find_open_review(&self, query: &OpenReviewQuery) -> Result<Option<OpenReviewSummary>> {
        validate_gitlab_repo(query.base_repo.provider)?;
        validate_gitlab_repo(query.head_repo.provider)?;

        let head_project_id = self.project_id_for_repo(&query.head_repo)?;
        let normalized_target = query
            .target_branch
            .as_deref()
            .map(str::trim)
            .filter(|branch| !branch.is_empty())
            .map(ToOwned::to_owned);

        let mut endpoint = MergeRequests::builder();
        endpoint
            .project(query.base_repo.path.as_str())
            .state(MergeRequestState::Opened)
            .scope(MergeRequestScope::All)
            .source_branch(query.source_branch.as_str());
        if let Some(target_branch) = normalized_target.as_deref() {
            endpoint.target_branch(target_branch);
        }
        let endpoint = endpoint
            .build()
            .context("failed to build GitLab merge request query")?;
        let merge_requests: Vec<GitLabMergeRequest> = endpoint
            .query(&self.client)
            .context("failed to query GitLab merge requests")?;

        Ok(select_open_merge_request(
            merge_requests,
            normalized_target.as_deref(),
            head_project_id,
        )
        .map(|merge_request| map_gitlab_merge_request(&query.base_repo, merge_request)))
    }

    pub fn create_review(&self, input: &CreateReviewInput) -> Result<CreateReviewResult> {
        validate_gitlab_repo(input.base_repo.provider)?;
        validate_gitlab_repo(input.head_repo.provider)?;

        let head_project_id = self.project_id_for_repo(&input.head_repo)?;
        let base_project_id = self.project_id_for_repo(&input.base_repo)?;

        let mut endpoint = CreateMergeRequest::builder();
        endpoint
            .project(head_project_id)
            .source_branch(input.source_branch.as_str())
            .target_branch(input.target_branch.as_str())
            .title(normalize_gitlab_review_title(
                input.title.as_str(),
                input.draft,
            ));
        if let Some(body) = input
            .body
            .as_deref()
            .map(str::trim)
            .filter(|body| !body.is_empty())
        {
            endpoint.description(body);
        }
        if head_project_id != base_project_id {
            endpoint.target_project_id(base_project_id);
        }
        let endpoint = endpoint
            .build()
            .context("failed to build GitLab merge request create request")?;
        let merge_request: GitLabMergeRequest = endpoint
            .query(&self.client)
            .context("failed to create GitLab merge request")?;

        Ok(CreateReviewResult {
            review: map_gitlab_merge_request(&input.base_repo, merge_request),
        })
    }

    fn project_id_for_repo(&self, repo: &ForgeRepoRef) -> Result<u64> {
        let endpoint = Project::builder()
            .project(repo.path.as_str())
            .build()
            .context("failed to build GitLab project lookup")?;
        let project: GitLabProject = endpoint
            .query(&self.client)
            .with_context(|| format!("failed to load GitLab project '{}'", repo.path))?;
        Ok(project.id)
    }
}

fn validate_gitlab_repo(provider: ForgeProvider) -> Result<()> {
    if provider != ForgeProvider::GitLab {
        bail!("GitLab review client only supports GitLab repositories");
    }
    Ok(())
}

fn select_open_merge_request(
    merge_requests: Vec<GitLabMergeRequest>,
    target_branch: Option<&str>,
    head_project_id: u64,
) -> Option<GitLabMergeRequest> {
    let normalized_target = target_branch
        .map(str::trim)
        .filter(|branch| !branch.is_empty());

    if let Some(target_branch) = normalized_target
        && let Some(index) = merge_requests.iter().position(|merge_request| {
            merge_request.target_branch == target_branch
                && merge_request.source_project_matches(head_project_id)
        })
    {
        return merge_requests.into_iter().nth(index);
    }

    if let Some(index) = merge_requests
        .iter()
        .position(|merge_request| merge_request.source_project_matches(head_project_id))
    {
        return merge_requests.into_iter().nth(index);
    }

    if let Some(target_branch) = normalized_target
        && let Some(index) = merge_requests
            .iter()
            .position(|merge_request| merge_request.target_branch == target_branch)
    {
        return merge_requests.into_iter().nth(index);
    }

    merge_requests.into_iter().next()
}

fn map_gitlab_merge_request(
    repo: &ForgeRepoRef,
    merge_request: GitLabMergeRequest,
) -> OpenReviewSummary {
    let draft = merge_request.is_draft();
    let number = merge_request.iid;
    OpenReviewSummary {
        provider: ForgeProvider::GitLab,
        number,
        title: merge_request
            .title
            .unwrap_or_else(|| format!("Merge Request !{number}")),
        url: merge_request
            .web_url
            .unwrap_or_else(|| format!("{}/-/merge_requests/{number}", repo.web_base_url)),
        state: match merge_request.state.as_deref() {
            Some("merged") => ForgeReviewState::Merged,
            Some("closed") => ForgeReviewState::Closed,
            _ => ForgeReviewState::Open,
        },
        draft,
        source_branch: merge_request.source_branch,
        target_branch: merge_request.target_branch,
    }
}

fn normalize_gitlab_review_title(title: &str, draft: bool) -> String {
    let normalized = title.trim();
    if !draft || is_gitlab_draft_title(normalized) {
        return normalized.to_string();
    }

    format!("Draft: {normalized}")
}

fn is_gitlab_draft_title(title: &str) -> bool {
    let lowered = title.trim_start().to_ascii_lowercase();
    lowered.starts_with("draft:") || lowered.starts_with("wip:")
}

#[derive(Debug, Deserialize)]
struct GitLabProject {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct GitLabMergeRequest {
    iid: u64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    web_url: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    draft: Option<bool>,
    #[serde(default)]
    work_in_progress: Option<bool>,
    source_branch: String,
    target_branch: String,
    #[serde(default)]
    source_project_id: Option<u64>,
}

impl GitLabMergeRequest {
    fn is_draft(&self) -> bool {
        self.draft
            .or(self.work_in_progress)
            .unwrap_or_else(|| self.title.as_deref().is_some_and(is_gitlab_draft_title))
    }

    fn source_project_matches(&self, head_project_id: u64) -> bool {
        self.source_project_id == Some(head_project_id)
    }
}
