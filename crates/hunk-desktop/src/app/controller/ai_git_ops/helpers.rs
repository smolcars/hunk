fn send_ai_git_progress(
    progress_tx: &mpsc::UnboundedSender<AiGitProgressEvent>,
    step: AiGitProgressStep,
    detail: Option<String>,
) {
    if progress_tx
        .unbounded_send(AiGitProgressEvent { step, detail })
        .is_err()
    {
        debug!("dropping AI git progress update because the receiver is gone");
    }
}

fn ai_branch_progress_detail(label: &str, branch_name: &str) -> String {
    format!("{label}: {branch_name}")
}

fn ai_thread_progress_detail(label: &str, thread_id: &str) -> String {
    format!("{label}: {thread_id}")
}

fn ai_commit_progress_detail(subject: &str) -> String {
    format!("Commit: {subject}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiOpenPrBranchStrategy {
    CreateReviewBranch,
    ReuseCurrentBranch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiCreateBranchAndPushStrategy {
    CreateBranchImmediately,
    PromptForPushTarget,
}

fn ai_open_pr_branch_strategy(
    repo_root: &std::path::Path,
    branch_name: &str,
) -> AiOpenPrBranchStrategy {
    let default_branch_name = resolve_default_base_branch_name(repo_root).ok().flatten();
    ai_open_pr_branch_strategy_for_branch(branch_name, default_branch_name.as_deref())
}

fn ai_open_pr_branch_strategy_for_branch(
    branch_name: &str,
    default_branch_name: Option<&str>,
) -> AiOpenPrBranchStrategy {
    let branch_name = branch_name.trim();
    let default_branch_name = default_branch_name.map(str::trim);
    if default_branch_name == Some(branch_name) {
        AiOpenPrBranchStrategy::CreateReviewBranch
    } else {
        AiOpenPrBranchStrategy::ReuseCurrentBranch
    }
}

fn ai_create_branch_and_push_strategy(
    repo_root: &std::path::Path,
    branch_name: &str,
) -> AiCreateBranchAndPushStrategy {
    let default_branch_name = resolve_default_base_branch_name(repo_root).ok().flatten();
    ai_create_branch_and_push_strategy_for_branch(branch_name, default_branch_name.as_deref())
}

fn ai_create_branch_and_push_strategy_for_branch(
    branch_name: &str,
    default_branch_name: Option<&str>,
) -> AiCreateBranchAndPushStrategy {
    match ai_open_pr_branch_strategy_for_branch(branch_name, default_branch_name) {
        AiOpenPrBranchStrategy::CreateReviewBranch => {
            AiCreateBranchAndPushStrategy::CreateBranchImmediately
        }
        AiOpenPrBranchStrategy::ReuseCurrentBranch => {
            AiCreateBranchAndPushStrategy::PromptForPushTarget
        }
    }
}

fn ai_publish_blocker_reason(
    context: Result<AiThreadGitActionContext, String>,
) -> Option<String> {
    context.err()
}

fn resolve_ai_commit_message_for_working_copy(
    generation_config: AiCodexGenerationConfig<'_>,
    repo_root: &std::path::Path,
    branch_name: &str,
    fallback_commit_message: &AiCommitMessage,
) -> AiCommitMessage {
    let working_copy_context =
        working_copy_context_for_ai(repo_root, 200, 40_000).ok().flatten();
    let Some(working_copy_context) = working_copy_context else {
        return fallback_commit_message.clone();
    };

    try_ai_commit_message(
        generation_config,
        AiCommitGenerationContext {
            branch_name,
            changed_files_summary: working_copy_context.changed_files_summary.as_str(),
            diff_patch: working_copy_context.diff_patch.as_str(),
        },
    )
    .unwrap_or_else(|| fallback_commit_message.clone())
}

fn try_ai_commit_message_for_staged_index(
    generation_config: AiCodexGenerationConfig<'_>,
    repo_root: &std::path::Path,
    branch_name: &str,
) -> anyhow::Result<AiCommitMessage> {
    let staged_context = staged_index_context_for_ai(repo_root, 200, 40_000)?;
    let Some(staged_context) = staged_context else {
        return Err(anyhow::anyhow!("no staged changes to summarize"));
    };

    try_ai_commit_message(
        generation_config,
        AiCommitGenerationContext {
            branch_name,
            changed_files_summary: staged_context.changed_files_summary.as_str(),
            diff_patch: staged_context.diff_patch.as_str(),
        },
    )
    .ok_or_else(|| anyhow::anyhow!("AI commit message generation returned no structured output"))
}

fn activate_new_ai_review_branch(
    repo_root: &std::path::Path,
    requested_branch_name: &str,
) -> anyhow::Result<String> {
    let mut attempt = 0usize;
    loop {
        attempt = attempt.saturating_add(1);
        let candidate_branch_name = if attempt == 1 {
            requested_branch_name.to_string()
        } else {
            format!("{requested_branch_name}-{attempt}")
        };
        match checkout_or_create_branch_with_change_transfer(
            repo_root,
            candidate_branch_name.as_str(),
            true,
        ) {
            Ok(()) => return Ok(candidate_branch_name),
            Err(err) => {
                if err.to_string().contains("already exists") && attempt < 20 {
                    continue;
                }
                return Err(err);
            }
        }
    }
}

fn push_current_branch_with_publish_fallback(
    repo_root: &std::path::Path,
    branch_name: &str,
) -> anyhow::Result<()> {
    match push_current_branch(repo_root, branch_name, true) {
        Ok(()) => Ok(()),
        Err(err) if err.to_string().contains("publish this branch before pushing") => {
            push_current_branch(repo_root, branch_name, false)
        }
        Err(err) if err.to_string().contains("already published") => {
            push_current_branch(repo_root, branch_name, true)
        }
        Err(err) => Err(err),
    }
}

impl DiffViewer {
    fn spawn_ai_git_action_with_progress<T, F, H>(
        &mut self,
        epoch: usize,
        cx: &mut Context<Self>,
        run: F,
        apply: H,
    ) where
        T: Send + 'static,
        F: FnOnce(mpsc::UnboundedSender<AiGitProgressEvent>) -> T + Send + 'static,
        H: FnOnce(&mut DiffViewer, T, Duration, Duration, &mut Context<DiffViewer>)
            + Send
            + 'static,
    {
        let started_at = Instant::now();
        let mut apply = Some(apply);

        self.git_action_task = cx.spawn(async move |this, cx| {
            let (progress_tx, mut progress_rx) = mpsc::unbounded::<AiGitProgressEvent>();
            let git_task = cx.background_executor().spawn(async move {
                let execution_started_at = Instant::now();
                let result = run(progress_tx);
                (execution_started_at.elapsed(), result)
            });

            while let Some(update) = progress_rx.next().await {
                let Some(this) = this.upgrade() else {
                    break;
                };
                this.update(cx, move |this, cx| {
                    this.apply_ai_git_progress(epoch, update, cx);
                });
            }

            let (execution_elapsed, result) = git_task.await;
            let total_elapsed = started_at.elapsed();

            if let Some(this) = this.upgrade()
                && let Some(apply) = apply.take()
            {
                this.update(cx, move |this, cx| {
                    apply(this, result, execution_elapsed, total_elapsed, cx);
                });
            }
        });
    }
}

#[cfg(test)]
mod ai_git_ops_tests {
    use super::*;

    fn test_git_action_context(start_mode: AiNewThreadStartMode) -> AiThreadGitActionContext {
        AiThreadGitActionContext {
            repo_root: std::path::PathBuf::from("/repo"),
            thread_id: "thread-1".to_string(),
            branch_name: "feature/ai-thread".to_string(),
            start_mode,
        }
    }

    #[test]
    fn publish_blocker_allows_local_threads() {
        assert_eq!(
            ai_publish_blocker_reason(Ok(test_git_action_context(AiNewThreadStartMode::Local))),
            None
        );
    }

    #[test]
    fn publish_blocker_allows_worktree_threads() {
        assert_eq!(
            ai_publish_blocker_reason(Ok(test_git_action_context(
                AiNewThreadStartMode::Worktree,
            ))),
            None
        );
    }

    #[test]
    fn publish_blocker_preserves_context_errors() {
        assert_eq!(
            ai_publish_blocker_reason(Err("Select a thread before publishing.".to_string())),
            Some("Select a thread before publishing.".to_string())
        );
    }

    #[test]
    fn open_pr_branch_strategy_creates_review_branch_for_default_branch() {
        assert_eq!(
            ai_open_pr_branch_strategy_for_branch("main", Some("main")),
            AiOpenPrBranchStrategy::CreateReviewBranch
        );
    }

    #[test]
    fn open_pr_branch_strategy_reuses_non_default_branch() {
        assert_eq!(
            ai_open_pr_branch_strategy_for_branch("feature/ai-thread", Some("main")),
            AiOpenPrBranchStrategy::ReuseCurrentBranch
        );
    }

    #[test]
    fn open_pr_branch_strategy_reuses_when_default_branch_is_unknown() {
        assert_eq!(
            ai_open_pr_branch_strategy_for_branch("feature/ai-thread", None),
            AiOpenPrBranchStrategy::ReuseCurrentBranch
        );
    }

    #[test]
    fn create_branch_and_push_strategy_creates_immediately_for_default_branch() {
        assert_eq!(
            ai_create_branch_and_push_strategy_for_branch("main", Some("main")),
            AiCreateBranchAndPushStrategy::CreateBranchImmediately
        );
    }

    #[test]
    fn create_branch_and_push_strategy_prompts_for_non_default_branch() {
        assert_eq!(
            ai_create_branch_and_push_strategy_for_branch("feature/ai-thread", Some("main")),
            AiCreateBranchAndPushStrategy::PromptForPushTarget
        );
    }

    #[test]
    fn create_branch_and_push_strategy_prompts_when_default_branch_is_unknown() {
        assert_eq!(
            ai_create_branch_and_push_strategy_for_branch("feature/ai-thread", None),
            AiCreateBranchAndPushStrategy::PromptForPushTarget
        );
    }
}
