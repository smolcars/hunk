#[derive(Clone, Copy)]
enum ReviewUrlAction {
    Open,
    Copy,
}

#[derive(Debug, Clone)]
struct GitHubReviewDialogContext {
    repo_root: PathBuf,
    review_remote: ReviewRemote,
    repo: ForgeRepoRef,
    source_branch: String,
    target_branch: String,
    title: String,
    body: Option<String>,
    action_label: String,
}

#[derive(Debug, Clone)]
struct GitHubReviewDialogValues {
    token: String,
    target_branch: String,
    title: String,
    body: Option<String>,
}

#[derive(Debug, Clone)]
struct GitHubReviewOpenDialogRequest {
    repo_root: PathBuf,
    branch_name: String,
    title: String,
    body: Option<String>,
    action_label: String,
}

#[derive(Debug, Clone)]
struct GitHubReviewOperationResult {
    review: OpenReviewSummary,
    repo_root: PathBuf,
    source_branch: String,
    existed: bool,
}

const GITHUB_TOKEN_ENV_KEYS: &[&str] = &["HUNK_GITHUB_TOKEN", "GITHUB_TOKEN"];

impl DiffViewer {
    fn review_summary_cache_key(repo_root: &std::path::Path, branch_name: &str) -> String {
        format!("{}::{}", repo_root.display(), branch_name.trim())
    }

    fn cached_review_summary_for_branch(
        &self,
        repo_root: &std::path::Path,
        branch_name: &str,
    ) -> Option<&OpenReviewSummary> {
        let key = Self::review_summary_cache_key(repo_root, branch_name);
        self.review_summary_by_branch_key.get(key.as_str())
    }

    fn cache_review_summary_for_branch(
        &mut self,
        repo_root: &std::path::Path,
        branch_name: &str,
        review: Option<OpenReviewSummary>,
    ) {
        let key = Self::review_summary_cache_key(repo_root, branch_name);
        if let Some(review) = review {
            self.review_summary_by_branch_key
                .insert(key.clone(), review);
            self.review_summary_miss_by_branch_key.remove(key.as_str());
        } else {
            self.review_summary_by_branch_key.remove(key.as_str());
            self.review_summary_miss_by_branch_key.insert(key);
        }
    }

    fn clear_review_summary_miss_for_branch(
        &mut self,
        repo_root: &std::path::Path,
        branch_name: &str,
    ) {
        let key = Self::review_summary_cache_key(repo_root, branch_name);
        self.review_summary_miss_by_branch_key.remove(key.as_str());
    }

    pub(super) fn selected_git_workspace_review_summary(&self) -> Option<&OpenReviewSummary> {
        let repo_root = self.selected_git_workspace_root()?;
        let branch_name = self.git_workspace.branch_name.trim();
        if branch_name.is_empty() || matches!(branch_name, "detached" | "unknown") {
            return None;
        }
        self.cached_review_summary_for_branch(repo_root.as_path(), branch_name)
    }

    fn review_summary_for_workspace_branch(
        &self,
        repo_root: &std::path::Path,
        branch_name: &str,
    ) -> Option<OpenReviewSummary> {
        self.cached_review_summary_for_branch(repo_root, branch_name)
            .cloned()
    }

    fn github_token_for_host(&self, host: &str) -> Option<String> {
        self.github_tokens_by_host
            .get(host)
            .cloned()
            .or_else(|| {
                GITHUB_TOKEN_ENV_KEYS
                    .iter()
                    .find_map(|key| std::env::var(key).ok().filter(|value| !value.trim().is_empty()))
            })
    }

    fn remember_github_token(&mut self, host: &str, token: &str) {
        let host = host.trim();
        let token = token.trim();
        if host.is_empty() || token.is_empty() {
            return;
        }
        self.github_tokens_by_host
            .insert(host.to_string(), token.to_string());
    }

    fn preferred_review_base_branch(
        &self,
        repo_root: &std::path::Path,
        source_branch: &str,
    ) -> String {
        let resolved = resolve_default_base_branch_name(repo_root)
            .ok()
            .flatten()
            .filter(|candidate| !candidate.trim().is_empty())
            .unwrap_or_else(|| "main".to_string());
        if resolved != source_branch {
            return resolved;
        }
        if source_branch != "main" {
            return "main".to_string();
        }
        if source_branch != "master" {
            return "master".to_string();
        }
        resolved
    }

    pub(super) fn open_review_summary_in_browser(
        &mut self,
        review: &OpenReviewSummary,
        cx: &mut Context<Self>,
    ) {
        match open_url_in_browser(review.url.as_str()) {
            Ok(()) => {
                self.git_status_message = Some(format!(
                    "Opened {} #{} in browser",
                    match review.provider {
                        hunk_forge::ForgeProvider::GitHub => "PR",
                        hunk_forge::ForgeProvider::GitLab => "MR",
                    },
                    review.number
                ));
            }
            Err(err) => {
                let summary = err.to_string();
                self.git_status_message = Some(format!("Open URL failed: {summary}"));
                Self::push_error_notification(format!("Open review URL failed: {summary}"), cx);
            }
        }
        cx.notify();
    }

    pub(super) fn copy_review_summary_url(
        &mut self,
        review: &OpenReviewSummary,
        cx: &mut Context<Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(review.url.clone()));
        let message = format!(
            "Copied {} #{} URL",
            match review.provider {
                hunk_forge::ForgeProvider::GitHub => "PR",
                hunk_forge::ForgeProvider::GitLab => "MR",
            },
            review.number
        );
        self.git_status_message = Some(message.clone());
        Self::push_success_notification(message, cx);
        cx.notify();
    }

    fn maybe_queue_review_summary_lookup(
        &mut self,
        repo_root: PathBuf,
        branch_name: String,
        cx: &mut Context<Self>,
    ) {
        let normalized_branch = branch_name.trim();
        if normalized_branch.is_empty() || matches!(normalized_branch, "detached" | "unknown") {
            return;
        }
        let cache_key = Self::review_summary_cache_key(repo_root.as_path(), normalized_branch);
        if self.review_summary_by_branch_key.contains_key(cache_key.as_str())
            || self
                .review_summary_miss_by_branch_key
                .contains(cache_key.as_str())
            || self.review_summary_lookup_in_flight.contains(cache_key.as_str())
        {
            return;
        }

        let provider_mappings = self.config.review_provider_mappings.clone();
        let Ok(Some(review_remote)) = review_remote_for_branch_with_provider_map(
            repo_root.as_path(),
            normalized_branch,
            &provider_mappings,
        ) else {
            return;
        };
        if review_remote.provider != hunk_git::config::ReviewProviderKind::GitHub {
            return;
        }
        let Some(token) = self.github_token_for_host(review_remote.host.as_str()) else {
            return;
        };
        let Ok(repo) = ForgeRepoRef::try_from(&review_remote) else {
            return;
        };

        let target_branch = self.preferred_review_base_branch(repo_root.as_path(), normalized_branch);
        self.review_summary_lookup_in_flight.insert(cache_key.clone());
        let source_branch = normalized_branch.to_string();
        let source_branch_for_update = source_branch.clone();
        self.review_summary_lookup_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let client =
                        GitHubReviewClient::new(review_remote.host.as_str(), token.as_str())?;
                    client.find_open_review(&OpenReviewQuery {
                        repo,
                        source_branch: source_branch.clone(),
                        target_branch: Some(target_branch),
                    })
                })
                .await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.review_summary_lookup_in_flight.remove(cache_key.as_str());
                    match result {
                        Ok(review) => {
                            this.cache_review_summary_for_branch(
                                repo_root.as_path(),
                                source_branch_for_update.as_str(),
                                review,
                            );
                        }
                        Err(err) => {
                            error!(
                                "background review summary lookup failed for {}: {err:#}",
                                source_branch_for_update
                            );
                        }
                    }
                    cx.notify();
                });
            }
        });
    }

    fn submit_github_review_dialog(
        &mut self,
        context: GitHubReviewDialogContext,
        values: GitHubReviewDialogValues,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let token = if values.token.trim().is_empty() {
            self.github_token_for_host(context.review_remote.host.as_str())
                .ok_or_else(|| "GitHub token is required.".to_string())?
        } else {
            values.token.trim().to_string()
        };
        let target_branch = values.target_branch.trim();
        if target_branch.is_empty() {
            return Err("Base branch is required.".to_string());
        }
        let title = values.title.trim();
        if title.is_empty() {
            return Err("Pull request title is required.".to_string());
        }
        if target_branch == context.source_branch {
            return Err("Base branch must differ from the source branch.".to_string());
        }

        self.remember_github_token(context.review_remote.host.as_str(), token.as_str());
        self.run_github_review_lookup_or_create(
            GitHubReviewDialogContext {
                target_branch: target_branch.to_string(),
                title: title.to_string(),
                body: values.body,
                ..context
            },
            token,
            cx,
        );
        Ok(())
    }

    fn run_github_review_lookup_or_create(
        &mut self,
        context: GitHubReviewDialogContext,
        token: String,
        cx: &mut Context<Self>,
    ) {
        let epoch = self.begin_git_action(context.action_label.clone(), cx);
        let started_at = Instant::now();
        self.git_action_task = cx.spawn(async move |this, cx| {
            let (execution_elapsed, result) = cx
                .background_executor()
                .spawn(async move {
                    let execution_started_at = Instant::now();
                    let result = (|| -> anyhow::Result<GitHubReviewOperationResult> {
                        let client =
                            GitHubReviewClient::new(context.repo.host.as_str(), token.as_str())?;
                        let existing = client.find_open_review(&OpenReviewQuery {
                            repo: context.repo.clone(),
                            source_branch: context.source_branch.clone(),
                            target_branch: Some(context.target_branch.clone()),
                        })?;
                        if let Some(review) = existing {
                            return Ok(GitHubReviewOperationResult {
                                review,
                                repo_root: context.repo_root.clone(),
                                source_branch: context.source_branch.clone(),
                                existed: true,
                            });
                        }

                        let created = client.create_review(&CreateReviewInput {
                            repo: context.repo,
                            source_branch: context.source_branch.clone(),
                            target_branch: context.target_branch,
                            title: context.title,
                            body: context.body,
                            draft: false,
                        })?;
                        Ok(GitHubReviewOperationResult {
                            review: created.review,
                            repo_root: context.repo_root,
                            source_branch: context.source_branch,
                            existed: false,
                        })
                    })();
                    (execution_started_at.elapsed(), result)
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    let total_elapsed = started_at.elapsed();
                    this.finish_git_action();
                    match result {
                        Ok(result) => {
                            debug!(
                                "github review action complete: epoch={} branch={} existed={} lookup_elapsed_ms={} total_elapsed_ms={}",
                                epoch,
                                result.source_branch,
                                result.existed,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            this.cache_review_summary_for_branch(
                                result.repo_root.as_path(),
                                result.source_branch.as_str(),
                                Some(result.review.clone()),
                            );
                            let message = if result.existed {
                                format!(
                                    "Using existing GitHub PR #{} for {}",
                                    result.review.number, result.source_branch
                                )
                            } else {
                                format!(
                                    "Created GitHub PR #{} for {}",
                                    result.review.number, result.source_branch
                                )
                            };
                            this.git_status_message = Some(message.clone());
                            Self::push_success_notification(message, cx);
                        }
                        Err(err) => {
                            error!(
                                "github review action failed: epoch={} err={err:#}",
                                epoch
                            );
                            let summary = err.to_string();
                            this.git_status_message = Some(format!("Git error: {err:#}"));
                            Self::push_error_notification(
                                format!("GitHub pull request failed: {summary}"),
                                cx,
                            );
                        }
                    }
                    cx.notify();
                });
            }
        });
    }

    fn open_github_review_dialog(
        &mut self,
        window: &mut Window,
        context: GitHubReviewDialogContext,
        cx: &mut Context<Self>,
    ) {
        let cached_token = self.github_token_for_host(context.review_remote.host.as_str());
        let token_placeholder = if cached_token.is_some() {
            "GitHub token (leave blank to reuse cached token)"
        } else {
            "GitHub token"
        };
        let token_input = cx.new(|cx| InputState::new(window, cx).placeholder(token_placeholder));
        let title_input = cx.new(|cx| InputState::new(window, cx).placeholder("Pull request title"));
        let base_branch_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Base branch"));
        let body_input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(6)
                .placeholder("Description (optional)")
        });
        title_input.update(cx, |input, cx| {
            input.set_value(context.title.clone(), window, cx);
        });
        base_branch_input.update(cx, |input, cx| {
            input.set_value(context.target_branch.clone(), window, cx);
        });
        if let Some(body) = context.body.clone().filter(|body| !body.trim().is_empty()) {
            body_input.update(cx, |input, cx| {
                input.set_value(body, window, cx);
            });
        }

        let description = format!(
            "{} from '{}' into '{}' on {}.",
            if self.cached_review_summary_for_branch(
                context.repo_root.as_path(),
                context.source_branch.as_str(),
            )
            .is_some()
            {
                "Review the current pull request"
            } else {
                "Create or find a pull request"
            },
            context.source_branch,
            context.target_branch,
            context.repo.path
        );
        let host_hint = format!("Host: {}", context.review_remote.host);
        let view = cx.entity();

        gpui_component::WindowExt::open_alert_dialog(window, cx, move |alert, _, cx| {
            alert
                .width(px(620.0))
                .title("GitHub Pull Request")
                .description(description.clone())
                .child(
                    v_flex()
                        .w_full()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(host_hint.clone()),
                        )
                        .child(
                            v_flex()
                                .w_full()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Token"),
                                )
                                .child(
                                    gpui_component::input::Input::new(&token_input)
                                        .appearance(true)
                                        .w_full()
                                        .with_size(gpui_component::Size::Medium),
                                ),
                        )
                        .child(
                            v_flex()
                                .w_full()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Title"),
                                )
                                .child(
                                    gpui_component::input::Input::new(&title_input)
                                        .appearance(true)
                                        .w_full()
                                        .with_size(gpui_component::Size::Medium),
                                ),
                        )
                        .child(
                            v_flex()
                                .w_full()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Base Branch"),
                                )
                                .child(
                                    gpui_component::input::Input::new(&base_branch_input)
                                        .appearance(true)
                                        .w_full()
                                        .with_size(gpui_component::Size::Medium),
                                ),
                        )
                        .child(
                            v_flex()
                                .w_full()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Description"),
                                )
                                .child(
                                    gpui_component::input::Input::new(&body_input)
                                        .appearance(true)
                                        .w_full()
                                        .with_size(gpui_component::Size::Medium)
                                        .h(px(144.0)),
                                ),
                        ),
                )
                .footer(
                    DialogFooter::new()
                        .w_full()
                        .justify_between()
                        .items_start()
                        .gap_3()
                        .child(
                            Button::new("github-review-cancel")
                                .label("Cancel")
                                .outline()
                                .on_click(|_, window, cx| {
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("github-review-submit")
                                .label("Find / Create PR")
                                .primary()
                                .on_click({
                                    let view = view.clone();
                                    let context = context.clone();
                                    let token_input = token_input.clone();
                                    let title_input = title_input.clone();
                                    let base_branch_input = base_branch_input.clone();
                                    let body_input = body_input.clone();
                                    move |_, window, cx| {
                                        let values = GitHubReviewDialogValues {
                                            token: token_input.read(cx).value().to_string(),
                                            target_branch: base_branch_input
                                                .read(cx)
                                                .value()
                                                .to_string(),
                                            title: title_input.read(cx).value().to_string(),
                                            body: {
                                                let raw = body_input.read(cx).value().to_string();
                                                let trimmed = raw.trim().to_string();
                                                (!trimmed.is_empty()).then_some(trimmed)
                                            },
                                        };
                                        let result = view.update(cx, |this, cx| {
                                            this.submit_github_review_dialog(
                                                context.clone(),
                                                values,
                                                cx,
                                            )
                                        });
                                        match result {
                                            Ok(()) => window.close_dialog(cx),
                                            Err(message) => {
                                                view.update(cx, |this, cx| {
                                                    this.set_git_warning_message(
                                                        message,
                                                        Some(window),
                                                        cx,
                                                    );
                                                });
                                            }
                                        }
                                    }
                                }),
                        ),
                )
        });
    }

    fn open_github_review_dialog_for_branch(
        &mut self,
        request: GitHubReviewOpenDialogRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let provider_mappings = self.config.review_provider_mappings.clone();
        let review_remote = review_remote_for_branch_with_provider_map(
            request.repo_root.as_path(),
            request.branch_name.as_str(),
            provider_mappings.as_slice(),
        )
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "No review remote found for the active branch.".to_string())?;

        if review_remote.provider != hunk_git::config::ReviewProviderKind::GitHub {
            return Err("In-app PR creation is currently implemented for GitHub only.".to_string());
        }

        let repo = ForgeRepoRef::try_from(&review_remote).map_err(|err| err.to_string())?;
        let target_branch = self.preferred_review_base_branch(
            request.repo_root.as_path(),
            request.branch_name.as_str(),
        );
        self.open_github_review_dialog(
            window,
            GitHubReviewDialogContext {
                repo_root: request.repo_root,
                review_remote,
                repo,
                source_branch: request.branch_name,
                target_branch,
                title: request.title,
                body: request.body,
                action_label: request.action_label,
            },
            cx,
        );
        Ok(())
    }
}

fn with_review_title_prefill(url: String, title: &str) -> String {
    let normalized_title = normalized_review_title_subject(title);
    let Some(title) = normalized_title else {
        return url;
    };

    if url.contains("/-/merge_requests/new") {
        return append_query_param(url, "merge_request[title]", title.as_str());
    }

    if url.contains("/compare/") {
        let with_quick_pull = append_query_param(url, "quick_pull", "1");
        return append_query_param(with_quick_pull, "title", title.as_str());
    }

    url
}

fn append_query_param(url: String, key: &str, value: &str) -> String {
    let mut out = url;
    let separator = if out.contains('?') {
        if out.ends_with('?') || out.ends_with('&') {
            ""
        } else {
            "&"
        }
    } else {
        "?"
    };
    out.push_str(separator);
    out.push_str(percent_encode_url_component(key).as_str());
    out.push('=');
    out.push_str(percent_encode_url_component(value).as_str());
    out
}

fn normalized_review_title_subject(raw: &str) -> Option<String> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return None;
    }
    if normalized.starts_with('(') && normalized.contains("no description") {
        return None;
    }
    Some(normalized.to_string())
}

fn percent_encode_url_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        let is_unreserved =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            encoded.push(byte as char);
        } else {
            encoded.push_str(format!("%{byte:02X}").as_str());
        }
    }
    encoded
}
