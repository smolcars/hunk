#[allow(dead_code)]
#[path = "../src/app/ai_git_progress.rs"]
mod ai_git_progress;

use ai_git_progress::{
    AiGitProgressAction, AiGitProgressState, AiGitProgressStep, ai_commit_and_push_progress_steps,
    ai_create_branch_and_push_progress_steps, ai_delete_worktree_progress_steps,
    ai_open_pr_progress_steps,
};

#[test]
fn commit_and_push_progress_steps_match_publish_flow() {
    let steps = ai_commit_and_push_progress_steps();

    assert_eq!(
        steps,
        vec![
            AiGitProgressStep::GeneratingCommitMessage,
            AiGitProgressStep::CreatingCommit,
            AiGitProgressStep::PushingBranch,
        ]
    );
}

#[test]
fn open_pr_progress_steps_include_review_branch_generation_when_needed() {
    let steps = ai_open_pr_progress_steps(true);

    assert_eq!(
        steps,
        vec![
            AiGitProgressStep::GeneratingBranchName,
            AiGitProgressStep::CreatingReviewBranch,
            AiGitProgressStep::GeneratingCommitMessage,
            AiGitProgressStep::CreatingCommit,
            AiGitProgressStep::PushingBranch,
            AiGitProgressStep::PreparingReviewUrl,
            AiGitProgressStep::OpeningBrowser,
        ]
    );
}

#[test]
fn create_branch_and_push_progress_steps_include_branch_creation() {
    let steps = ai_create_branch_and_push_progress_steps();

    assert_eq!(
        steps,
        vec![
            AiGitProgressStep::GeneratingBranchName,
            AiGitProgressStep::CreatingReviewBranch,
            AiGitProgressStep::GeneratingCommitMessage,
            AiGitProgressStep::CreatingCommit,
            AiGitProgressStep::PushingBranch,
        ]
    );
}

#[test]
fn open_pr_progress_steps_skip_branch_creation_when_reusing_current_branch() {
    let steps = ai_open_pr_progress_steps(false);

    assert_eq!(
        steps,
        vec![
            AiGitProgressStep::GeneratingCommitMessage,
            AiGitProgressStep::CreatingCommit,
            AiGitProgressStep::PushingBranch,
            AiGitProgressStep::PreparingReviewUrl,
            AiGitProgressStep::OpeningBrowser,
        ]
    );
}

#[test]
fn delete_worktree_progress_steps_archive_before_removal() {
    let steps = ai_delete_worktree_progress_steps();

    assert_eq!(
        steps,
        vec![
            AiGitProgressStep::ArchivingThread,
            AiGitProgressStep::RemovingWorktree,
        ]
    );
}

#[test]
fn progress_state_apply_replaces_current_step_and_detail() {
    let mut state = AiGitProgressState::new(
        7,
        AiGitProgressAction::OpenPr,
        ai_open_pr_progress_steps(true),
        AiGitProgressStep::GeneratingBranchName,
        Some("Current branch: main".to_string()),
    );

    state.apply(
        AiGitProgressStep::CreatingReviewBranch,
        Some("Review branch: ai/local/fix-progress-popup".to_string()),
    );

    assert_eq!(state.epoch, 7);
    assert_eq!(state.action, AiGitProgressAction::OpenPr);
    assert_eq!(state.step, AiGitProgressStep::CreatingReviewBranch);
    assert_eq!(
        state.detail.as_deref(),
        Some("Review branch: ai/local/fix-progress-popup")
    );
}
