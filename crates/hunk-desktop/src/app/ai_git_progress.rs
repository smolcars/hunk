#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiGitProgressAction {
    CommitAndPush,
    OpenPr,
    DeleteWorktree,
}

impl AiGitProgressAction {
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::CommitAndPush => "Commit and Push",
            Self::OpenPr => "Open PR",
            Self::DeleteWorktree => "Delete Worktree",
        }
    }

    pub(crate) const fn summary(self) -> &'static str {
        match self {
            Self::CommitAndPush => "Publishing the current AI thread to the active branch.",
            Self::OpenPr => "Publishing changes and opening the review in your browser.",
            Self::DeleteWorktree => {
                "Archiving the thread, removing the managed worktree checkout, and cleaning up its AI state."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiGitProgressStep {
    GeneratingBranchName,
    CreatingReviewBranch,
    GeneratingCommitMessage,
    CreatingCommit,
    PushingBranch,
    PreparingReviewUrl,
    OpeningBrowser,
    ArchivingThread,
    RemovingWorktree,
}

impl AiGitProgressStep {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::GeneratingBranchName => "Generating review branch name...",
            Self::CreatingReviewBranch => "Creating review branch...",
            Self::GeneratingCommitMessage => "Generating commit message...",
            Self::CreatingCommit => "Creating commit...",
            Self::PushingBranch => "Pushing branch...",
            Self::PreparingReviewUrl => "Preparing review URL...",
            Self::OpeningBrowser => "Opening review in browser...",
            Self::ArchivingThread => "Archiving thread...",
            Self::RemovingWorktree => "Deleting worktree checkout...",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiGitProgressState {
    pub(crate) epoch: usize,
    pub(crate) action: AiGitProgressAction,
    pub(crate) steps: Vec<AiGitProgressStep>,
    pub(crate) step: AiGitProgressStep,
    pub(crate) detail: Option<String>,
}

impl AiGitProgressState {
    pub(crate) fn new(
        epoch: usize,
        action: AiGitProgressAction,
        steps: Vec<AiGitProgressStep>,
        step: AiGitProgressStep,
        detail: Option<String>,
    ) -> Self {
        Self {
            epoch,
            action,
            steps,
            step,
            detail,
        }
    }

    pub(crate) fn apply(&mut self, step: AiGitProgressStep, detail: Option<String>) {
        self.step = step;
        self.detail = detail;
    }
}

pub(crate) fn ai_commit_and_push_progress_steps() -> Vec<AiGitProgressStep> {
    vec![
        AiGitProgressStep::GeneratingCommitMessage,
        AiGitProgressStep::CreatingCommit,
        AiGitProgressStep::PushingBranch,
    ]
}

pub(crate) fn ai_open_pr_progress_steps(create_review_branch: bool) -> Vec<AiGitProgressStep> {
    let mut steps = Vec::new();
    if create_review_branch {
        steps.push(AiGitProgressStep::GeneratingBranchName);
        steps.push(AiGitProgressStep::CreatingReviewBranch);
    }
    steps.extend([
        AiGitProgressStep::GeneratingCommitMessage,
        AiGitProgressStep::CreatingCommit,
        AiGitProgressStep::PushingBranch,
        AiGitProgressStep::PreparingReviewUrl,
        AiGitProgressStep::OpeningBrowser,
    ]);
    steps
}

pub(crate) fn ai_delete_worktree_progress_steps() -> Vec<AiGitProgressStep> {
    vec![
        AiGitProgressStep::ArchivingThread,
        AiGitProgressStep::RemovingWorktree,
    ]
}
