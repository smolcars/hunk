use anyhow::Result;

use crate::github::GitHubReviewClient;
use crate::gitlab::GitLabReviewClient;
use crate::models::{
    CreateReviewInput, CreateReviewResult, ForgeProvider, ForgeRepoRef, OpenReviewQuery,
    OpenReviewSummary,
};

#[derive(Debug)]
pub enum ForgeReviewClient {
    GitHub(GitHubReviewClient),
    GitLab(GitLabReviewClient),
}

impl ForgeReviewClient {
    pub fn new(repo: &ForgeRepoRef, token: &str) -> Result<Self> {
        match repo.provider {
            ForgeProvider::GitHub => Ok(Self::GitHub(GitHubReviewClient::for_repo(repo, token)?)),
            ForgeProvider::GitLab => Ok(Self::GitLab(GitLabReviewClient::new(repo, token)?)),
        }
    }

    pub fn find_open_review(&self, query: &OpenReviewQuery) -> Result<Option<OpenReviewSummary>> {
        match self {
            Self::GitHub(client) => client.find_open_review(query),
            Self::GitLab(client) => client.find_open_review(query),
        }
    }

    pub fn create_review(&self, input: &CreateReviewInput) -> Result<CreateReviewResult> {
        match self {
            Self::GitHub(client) => client.create_review(input),
            Self::GitLab(client) => client.create_review(input),
        }
    }
}
