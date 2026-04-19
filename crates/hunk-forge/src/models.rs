use anyhow::{Result, anyhow};
use hunk_git::branch::ReviewRemote;
use hunk_git::config::ReviewProviderKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeProvider {
    GitHub,
    GitLab,
}

impl From<ReviewProviderKind> for ForgeProvider {
    fn from(value: ReviewProviderKind) -> Self {
        match value {
            ReviewProviderKind::GitHub => Self::GitHub,
            ReviewProviderKind::GitLab => Self::GitLab,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeRepoRef {
    pub provider: ForgeProvider,
    pub host: String,
    pub authority: String,
    pub namespace: String,
    pub name: String,
    pub path: String,
    pub web_base_url: String,
}

impl ForgeRepoRef {
    pub fn github_owner(&self) -> Result<&str> {
        if self.provider != ForgeProvider::GitHub {
            return Err(anyhow!(
                "github owner is only available for GitHub repositories"
            ));
        }
        Ok(self.namespace.as_str())
    }
}

impl TryFrom<&ReviewRemote> for ForgeRepoRef {
    type Error = anyhow::Error;

    fn try_from(value: &ReviewRemote) -> Result<Self> {
        let mut segments = value
            .repository_path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        if segments.len() < 2 {
            return Err(anyhow!(
                "remote repository path '{}' is not a forge repository path",
                value.repository_path
            ));
        }

        let name = segments
            .pop()
            .ok_or_else(|| anyhow!("missing repository name in remote path"))?
            .to_string();
        let namespace = segments.join("/");
        if namespace.is_empty() {
            return Err(anyhow!(
                "remote repository path '{}' is missing a namespace",
                value.repository_path
            ));
        }

        if value.provider == ReviewProviderKind::GitHub && segments.len() != 1 {
            return Err(anyhow!(
                "github repository path '{}' must contain exactly two segments",
                value.repository_path
            ));
        }

        Ok(Self {
            provider: value.provider.into(),
            host: value.host.clone(),
            authority: value.authority.clone(),
            namespace,
            name,
            path: value.repository_path.clone(),
            web_base_url: value.base_url.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForgeReviewState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenReviewSummary {
    pub provider: ForgeProvider,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: ForgeReviewState,
    pub draft: bool,
    pub source_branch: String,
    pub target_branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenReviewQuery {
    pub base_repo: ForgeRepoRef,
    pub head_repo: ForgeRepoRef,
    pub source_branch: String,
    pub target_branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateReviewInput {
    pub base_repo: ForgeRepoRef,
    pub head_repo: ForgeRepoRef,
    pub source_branch: String,
    pub target_branch: String,
    pub title: String,
    pub body: Option<String>,
    pub draft: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateReviewResult {
    pub review: OpenReviewSummary,
}
