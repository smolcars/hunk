#[derive(Clone, Copy)]
enum ReviewUrlAction {
    Open,
    Copy,
}

#[derive(Debug, Clone)]
struct GitHubReviewDialogContext {
    repo_root: PathBuf,
    review_remote: ReviewRemote,
    base_repo: ForgeRepoRef,
    source_head_owner: String,
    source_branch: String,
    target_branch: String,
    title: String,
    body: Option<String>,
    action_label: String,
}

#[derive(Debug, Clone)]
struct GitHubReviewDialogValues {
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
    token_cache_entry: Option<(String, String)>,
}

#[derive(Debug, Clone)]
struct ResolvedGitHubReviewRepos {
    review_remote: ReviewRemote,
    base_repo: ForgeRepoRef,
    source_head_owner: String,
}

const GITHUB_TOKEN_ENV_KEYS: &[&str] = &["HUNK_GITHUB_TOKEN", "GITHUB_TOKEN"];

#[derive(Debug, Clone)]
enum GitHubTokenSource {
    Immediate(String),
    StoredCredential(String),
}

fn resolve_github_token_source(
    token_source: GitHubTokenSource,
) -> anyhow::Result<(String, Option<(String, String)>)> {
    match token_source {
        GitHubTokenSource::Immediate(token) => Ok((token, None)),
        GitHubTokenSource::StoredCredential(credential_id) => {
            let token = load_forge_secret(credential_id.as_str())?.ok_or_else(|| {
                anyhow::anyhow!(
                    "No saved GitHub token found for the selected credential. Enter a token to continue."
                )
            })?;
            Ok((token.clone(), Some((credential_id, token))))
        }
    }
}

fn next_forge_credential_id(
    provider: hunk_forge::ForgeProvider,
    host: &str,
    seed: &str,
) -> String {
    let provider_label = match provider {
        hunk_forge::ForgeProvider::GitHub => "github",
        hunk_forge::ForgeProvider::GitLab => "gitlab",
    };
    let host_fragment = host
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let seed_fragment = seed
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{provider_label}-{host_fragment}-{seed_fragment}-{nonce}")
}

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

    fn forge_credentials(&self) -> Vec<ForgeCredentialMetadata> {
        self.config
            .forge_credentials
            .iter()
            .map(|credential| ForgeCredentialMetadata {
                id: credential.id.clone(),
                provider: credential.provider.into(),
                host: credential.host.clone(),
                kind: match credential.kind {
                    hunk_domain::config::ForgeCredentialKind::PersonalAccessToken => {
                        ForgeCredentialKind::PersonalAccessToken
                    }
                    hunk_domain::config::ForgeCredentialKind::GitHubComSession => {
                        ForgeCredentialKind::GitHubComSession
                    }
                },
                account_label: credential.account_label.clone(),
                account_login: credential.account_login.clone(),
                is_default_for_host: credential.is_default_for_host,
            })
            .collect()
    }

    fn forge_repo_credential_bindings(&self) -> Vec<ForgeRepoCredentialBinding> {
        self.config
            .forge_repo_credential_bindings
            .iter()
            .map(|binding| ForgeRepoCredentialBinding {
                provider: binding.provider.into(),
                host: binding.host.clone(),
                repo_path: binding.repo_path.clone(),
                credential_id: binding.credential_id.clone(),
            })
            .collect()
    }

    fn resolved_github_credential_for_repo(
        &self,
        repo: &ForgeRepoRef,
    ) -> Option<hunk_forge::ResolvedForgeCredential> {
        resolve_credential_for_repo(
            repo,
            self.forge_credentials().as_slice(),
            self.forge_repo_credential_bindings().as_slice(),
        )
    }

    fn configured_github_credential_count_for_host(&self, host: &str) -> usize {
        self.config
            .forge_credentials
            .iter()
            .filter(|credential| {
                credential.provider == hunk_domain::config::ReviewProviderKind::GitHub
                    && credential.host == host
            })
            .count()
    }

    fn has_configured_github_credentials_for_host(&self, host: &str) -> bool {
        self.configured_github_credential_count_for_host(host) > 0
    }

    fn forge_token_for_credential(&self, credential_id: &str) -> Option<String> {
        self.forge_tokens_by_credential_id.get(credential_id).cloned()
    }

    fn load_forge_token_for_credential(
        &mut self,
        credential_id: &str,
    ) -> anyhow::Result<Option<String>> {
        if let Some(token) = self.forge_token_for_credential(credential_id) {
            return Ok(Some(token));
        }
        let token = load_forge_secret(credential_id)?;
        if let Some(secret) = token.as_ref() {
            self.forge_tokens_by_credential_id
                .insert(credential_id.to_string(), secret.clone());
        }
        Ok(token)
    }

    fn github_token_source_for_repo(&self, repo: &ForgeRepoRef) -> Option<GitHubTokenSource> {
        if let Some(resolved) = self.resolved_github_credential_for_repo(repo) {
            if let Some(token) = self.forge_token_for_credential(resolved.credential_id.as_str()) {
                return Some(GitHubTokenSource::Immediate(token));
            }
            return Some(GitHubTokenSource::StoredCredential(resolved.credential_id));
        }
        if self.has_configured_github_credentials_for_host(repo.host.as_str()) {
            return None;
        }
        GITHUB_TOKEN_ENV_KEYS
            .iter()
            .find_map(|key| std::env::var(key).ok().filter(|value| !value.trim().is_empty()))
            .map(GitHubTokenSource::Immediate)
    }

    fn create_default_github_credential(&mut self, repo: &ForgeRepoRef) -> String {
        let credential_id = next_forge_credential_id(
            hunk_forge::ForgeProvider::GitHub,
            repo.host.as_str(),
            "default",
        );
        self.config
            .forge_credentials
            .push(hunk_domain::config::ForgeCredentialConfig {
                id: credential_id.clone(),
                provider: hunk_domain::config::ReviewProviderKind::GitHub,
                host: repo.host.clone(),
                kind: hunk_domain::config::ForgeCredentialKind::PersonalAccessToken,
                account_label: "default".to_string(),
                account_login: None,
                is_default_for_host: true,
            });
        self.persist_config();
        credential_id
    }

    fn create_repo_bound_github_credential(&mut self, repo: &ForgeRepoRef) -> String {
        let credential_id = next_forge_credential_id(
            hunk_forge::ForgeProvider::GitHub,
            repo.host.as_str(),
            repo.path.as_str(),
        );
        self.config
            .forge_credentials
            .push(hunk_domain::config::ForgeCredentialConfig {
                id: credential_id.clone(),
                provider: hunk_domain::config::ReviewProviderKind::GitHub,
                host: repo.host.clone(),
                kind: hunk_domain::config::ForgeCredentialKind::PersonalAccessToken,
                account_label: repo.path.clone(),
                account_login: None,
                is_default_for_host: false,
            });
        self.config.forge_repo_credential_bindings.retain(|binding| {
            !(binding.provider == hunk_domain::config::ReviewProviderKind::GitHub
                && binding.host == repo.host
                && binding.repo_path == repo.path)
        });
        self.config
            .forge_repo_credential_bindings
            .push(hunk_domain::config::ForgeRepoCredentialBindingConfig {
                provider: hunk_domain::config::ReviewProviderKind::GitHub,
                host: repo.host.clone(),
                repo_path: repo.path.clone(),
                credential_id: credential_id.clone(),
            });
        self.persist_config();
        credential_id
    }

    fn remember_github_token_for_repo(&mut self, repo: &ForgeRepoRef, token: &str) -> Option<String> {
        let token = token.trim();
        if token.is_empty() {
            return None;
        }

        let resolved = self.resolved_github_credential_for_repo(repo);
        let configured_credential_count = self.configured_github_credential_count_for_host(repo.host.as_str());
        let credential_id = match resolved {
            Some(resolved) if resolved.resolution == ForgeCredentialResolution::RepoBinding => {
                resolved.credential_id
            }
            Some(resolved) => {
                let existing_token = if let Some(token) =
                    self.forge_token_for_credential(resolved.credential_id.as_str())
                {
                    Some(token)
                } else {
                    match self.load_forge_token_for_credential(resolved.credential_id.as_str()) {
                        Ok(token) => token,
                        Err(err) => {
                            warn!(
                                "failed to load stored forge credential {} while comparing tokens: {err:#}",
                                resolved.credential_id
                            );
                            None
                        }
                    }
                };
                if existing_token.is_none() || existing_token.as_deref() == Some(token) {
                    resolved.credential_id
                } else {
                    self.create_repo_bound_github_credential(repo)
                }
            }
            None if configured_credential_count == 0 => self.create_default_github_credential(repo),
            None => self.create_repo_bound_github_credential(repo),
        };

        self.forge_tokens_by_credential_id
            .insert(credential_id.clone(), token.to_string());
        Some(credential_id)
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

    fn resolve_github_review_repos_for_branch(
        &self,
        repo_root: &std::path::Path,
        branch_name: &str,
    ) -> Result<ResolvedGitHubReviewRepos, String> {
        let provider_mappings = self.config.review_provider_mappings.clone();
        let review_remote = review_remote_for_branch_with_provider_map(
            repo_root,
            branch_name,
            provider_mappings.as_slice(),
        )
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "No review remote found for the active branch.".to_string())?;

        if review_remote.provider != hunk_git::config::ReviewProviderKind::GitHub {
            return Err("In-app PR creation is currently implemented for GitHub only.".to_string());
        }

        let head_repo = ForgeRepoRef::try_from(&review_remote).map_err(|err| err.to_string())?;
        let base_remote = review_remote_for_named_remote_with_provider_map(
            repo_root,
            "upstream",
            provider_mappings.as_slice(),
        )
        .map_err(|err| err.to_string())?
        .filter(|candidate| {
            candidate.provider == hunk_git::config::ReviewProviderKind::GitHub
                && candidate.repository_path != review_remote.repository_path
        })
        .unwrap_or_else(|| review_remote.clone());
        let base_repo = ForgeRepoRef::try_from(&base_remote).map_err(|err| err.to_string())?;
        let source_head_owner = head_repo.github_owner().map_err(|err| err.to_string())?;

        Ok(ResolvedGitHubReviewRepos {
            review_remote: base_remote,
            base_repo,
            source_head_owner: source_head_owner.to_string(),
        })
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
        if self.review_summary_lookup_in_flight.contains(cache_key.as_str()) {
            return;
        }

        let Ok(resolved_repos) =
            self.resolve_github_review_repos_for_branch(repo_root.as_path(), normalized_branch)
        else {
            return;
        };
        let Some(token_source) = self.github_token_source_for_repo(&resolved_repos.base_repo) else {
            return;
        };

        self.review_summary_lookup_in_flight.insert(cache_key.clone());
        let source_branch = normalized_branch.to_string();
        let source_branch_for_update = source_branch.clone();
        self.review_summary_lookup_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let (token, token_cache_entry) = resolve_github_token_source(token_source)?;
                    let client =
                        GitHubReviewClient::new(resolved_repos.base_repo.authority.as_str(), token.as_str())?;
                    let review = client.find_open_review(&OpenReviewQuery {
                        repo: resolved_repos.base_repo,
                        source_branch: source_branch.clone(),
                        source_head_owner: Some(resolved_repos.source_head_owner),
                        target_branch: None,
                    })?;
                    Ok::<_, anyhow::Error>((review, token_cache_entry))
                })
                .await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.review_summary_lookup_in_flight.remove(cache_key.as_str());
                    match result {
                        Ok((review, token_cache_entry)) => {
                            if let Some((credential_id, token)) = token_cache_entry {
                                this.forge_tokens_by_credential_id.insert(credential_id, token);
                            }
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
        let token_source = self
            .github_token_source_for_repo(&context.base_repo)
            .ok_or_else(|| {
                "GitHub auth is required. Use the GitHub control in the Git tab to sign in or enter a token first.".to_string()
            })?;
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
        self.run_github_review_lookup_or_create(
            GitHubReviewDialogContext {
                target_branch: target_branch.to_string(),
                title: title.to_string(),
                body: values.body,
                ..context
            },
            token_source,
            cx,
        );
        Ok(())
    }

    fn run_github_review_lookup_or_create(
        &mut self,
        context: GitHubReviewDialogContext,
        token_source: GitHubTokenSource,
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
                        let (token, token_cache_entry) = resolve_github_token_source(token_source)?;
                        let client =
                            GitHubReviewClient::new(
                                context.base_repo.authority.as_str(),
                                token.as_str(),
                            )?;
                        let existing = client.find_open_review(&OpenReviewQuery {
                            repo: context.base_repo.clone(),
                            source_branch: context.source_branch.clone(),
                            source_head_owner: Some(context.source_head_owner.clone()),
                            target_branch: Some(context.target_branch.clone()),
                        })?;
                        if let Some(review) = existing {
                            return Ok(GitHubReviewOperationResult {
                                review,
                                repo_root: context.repo_root.clone(),
                                source_branch: context.source_branch.clone(),
                                existed: true,
                                token_cache_entry,
                            });
                        }

                        let created = client.create_review(&CreateReviewInput {
                            repo: context.base_repo,
                            source_branch: context.source_branch.clone(),
                            source_head_owner: Some(context.source_head_owner),
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
                            token_cache_entry,
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
                            if let Some((credential_id, token)) = result.token_cache_entry {
                                this.forge_tokens_by_credential_id.insert(credential_id, token);
                            }
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
            context.base_repo.path
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
                        .justify_end()
                        .child(
                            h_flex().gap_2().child(
                                Button::new("github-review-cancel")
                                    .label("Cancel")
                                    .outline()
                                    .on_click(|_, window, cx| {
                                        window.close_dialog(cx);
                                    }),
                            ),
                        )
                        .child(
                            Button::new("github-review-submit")
                                .label("Find / Create PR")
                                .primary()
                                .on_click({
                                    let view = view.clone();
                                    let context = context.clone();
                                    let title_input = title_input.clone();
                                    let base_branch_input = base_branch_input.clone();
                                    let body_input = body_input.clone();
                                    move |_, window, cx| {
                                        let values = GitHubReviewDialogValues {
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
        let resolved_repos = self.resolve_github_review_repos_for_branch(
            request.repo_root.as_path(),
            request.branch_name.as_str(),
        )?;
        if self.github_token_source_for_repo(&resolved_repos.base_repo).is_none() {
            return Err(
                "GitHub auth is required. Use the GitHub control in the Git tab to sign in or enter a token first."
                    .to_string(),
            );
        }
        let target_branch = self.preferred_review_base_branch(
            request.repo_root.as_path(),
            request.branch_name.as_str(),
        );
        self.open_github_review_dialog(
            window,
            GitHubReviewDialogContext {
                repo_root: request.repo_root,
                review_remote: resolved_repos.review_remote,
                base_repo: resolved_repos.base_repo,
                source_head_owner: resolved_repos.source_head_owner,
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
