#[derive(Debug, Clone, PartialEq, Eq)]
enum RepoTreePromptAction {
    CreateFile { base_dir: Option<String> },
    CreateFolder { base_dir: Option<String> },
    RenameFile { path: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RecentCommitsRefreshPriority {
    Background,
    UserInitiated,
}

impl RecentCommitsRefreshPriority {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::UserInitiated => "user",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RecentCommitsRefreshRequest {
    force: bool,
    priority: RecentCommitsRefreshPriority,
}

impl RecentCommitsRefreshRequest {
    const fn background() -> Self {
        Self {
            force: false,
            priority: RecentCommitsRefreshPriority::Background,
        }
    }

    const fn user(force: bool) -> Self {
        Self {
            force,
            priority: RecentCommitsRefreshPriority::UserInitiated,
        }
    }

    fn merge(self, other: Self) -> Self {
        Self {
            force: self.force || other.force,
            priority: if self.priority >= other.priority {
                self.priority
            } else {
                other.priority
            },
        }
    }

    fn is_more_urgent_than(self, other: Self) -> bool {
        self.priority > other.priority
            || (self.priority == other.priority && self.force && !other.force)
    }
}

#[derive(Clone)]
struct RepoTreeInlineEditState {
    action: RepoTreePromptAction,
    input_state: Entity<InputState>,
}

#[derive(Debug, Clone)]
struct RepoTreeContextMenuState {
    target_path: Option<String>,
    target_kind: RepoTreeNodeKind,
    position: Point<gpui::Pixels>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum AiComposerDraftKey {
    Thread(String),
    Workspace(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiNewThreadStartMode {
    Local,
    Worktree,
}

impl AiNewThreadStartMode {
    const fn label(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Worktree => "Worktree",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct AiComposerDraft {
    prompt: String,
    local_images: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct AiPendingThreadStart {
    workspace_key: String,
    prompt: String,
    local_images: Vec<PathBuf>,
    started_at: Instant,
    start_mode: AiNewThreadStartMode,
    thread_id: Option<String>,
}

#[derive(Debug, Clone)]
struct AiWorkspaceState {
    connection_state: AiConnectionState,
    bootstrap_loading: bool,
    status_message: Option<String>,
    error_message: Option<String>,
    state_snapshot: hunk_codex::state::AiState,
    selected_thread_id: Option<String>,
    new_thread_draft_active: bool,
    new_thread_start_mode: AiNewThreadStartMode,
    worktree_base_branch_name: Option<String>,
    pending_new_thread_selection: bool,
    pending_thread_start: Option<AiPendingThreadStart>,
    timeline_follow_output: bool,
    thread_title_refresh_state_by_thread: BTreeMap<String, AiThreadTitleRefreshState>,
    timeline_visible_turn_limit_by_thread: BTreeMap<String, usize>,
    in_progress_turn_started_at: BTreeMap<String, Instant>,
    expanded_timeline_row_ids: BTreeSet<String>,
    pending_approvals: Vec<AiPendingApproval>,
    pending_user_inputs: Vec<AiPendingUserInputRequest>,
    pending_user_input_answers: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    account: Option<codex_app_server_protocol::Account>,
    requires_openai_auth: bool,
    pending_chatgpt_login_id: Option<String>,
    pending_chatgpt_auth_url: Option<String>,
    rate_limits: Option<codex_app_server_protocol::RateLimitSnapshot>,
    models: Vec<codex_app_server_protocol::Model>,
    experimental_features: Vec<codex_app_server_protocol::ExperimentalFeature>,
    collaboration_modes: Vec<codex_app_server_protocol::CollaborationModeMask>,
    include_hidden_models: bool,
    selected_model: Option<String>,
    selected_effort: Option<String>,
    selected_collaboration_mode: AiCollaborationModeSelection,
    selected_service_tier: AiServiceTierSelection,
    mad_max_mode: bool,
}

impl Default for AiWorkspaceState {
    fn default() -> Self {
        Self {
            connection_state: AiConnectionState::Disconnected,
            bootstrap_loading: false,
            status_message: None,
            error_message: None,
            state_snapshot: hunk_codex::state::AiState::default(),
            selected_thread_id: None,
            new_thread_draft_active: false,
            new_thread_start_mode: AiNewThreadStartMode::Local,
            worktree_base_branch_name: None,
            pending_new_thread_selection: false,
            pending_thread_start: None,
            timeline_follow_output: true,
            thread_title_refresh_state_by_thread: BTreeMap::new(),
            timeline_visible_turn_limit_by_thread: BTreeMap::new(),
            in_progress_turn_started_at: BTreeMap::new(),
            expanded_timeline_row_ids: BTreeSet::new(),
            pending_approvals: Vec::new(),
            pending_user_inputs: Vec::new(),
            pending_user_input_answers: BTreeMap::new(),
            account: None,
            requires_openai_auth: false,
            pending_chatgpt_login_id: None,
            pending_chatgpt_auth_url: None,
            rate_limits: None,
            models: Vec::new(),
            experimental_features: Vec::new(),
            collaboration_modes: Vec::new(),
            include_hidden_models: true,
            selected_model: None,
            selected_effort: None,
            selected_collaboration_mode: AiCollaborationModeSelection::Default,
            selected_service_tier: AiServiceTierSelection::Standard,
            mad_max_mode: false,
        }
    }
}

struct AiHiddenRuntimeHandle {
    command_tx: mpsc::Sender<AiWorkerCommand>,
    worker_thread: JoinHandle<()>,
    event_task: Task<()>,
    generation: usize,
}

#[derive(Debug, Clone, Default)]
struct GitWorkspaceState {
    root: Option<PathBuf>,
    branch_name: String,
    branch_has_upstream: bool,
    branch_ahead_count: usize,
    branch_behind_count: usize,
    working_copy_commit_id: Option<String>,
    branches: Vec<LocalBranch>,
    files: Vec<ChangedFile>,
    file_status_by_path: BTreeMap<String, FileStatus>,
    file_line_stats: BTreeMap<String, LineStats>,
    overall_line_stats: LineStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiThreadTitleRefreshState {
    key: String,
    attempts: u8,
    in_flight: bool,
    last_attempt_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LineStatsRefreshScope {
    Full,
    Paths(BTreeSet<String>),
}

impl LineStatsRefreshScope {
    const fn label(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Paths(_) => "paths",
        }
    }

    fn path_count(&self) -> usize {
        match self {
            Self::Full => 0,
            Self::Paths(paths) => paths.len(),
        }
    }

    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Full, _) | (_, Self::Full) => Self::Full,
            (Self::Paths(mut left), Self::Paths(right)) => {
                left.extend(right);
                Self::Paths(left)
            }
        }
    }
}

#[derive(Debug, Clone)]
struct PendingLineStatsRefresh {
    repo_root: PathBuf,
    request: SnapshotRefreshRequest,
    scope: LineStatsRefreshScope,
    snapshot_epoch: usize,
    cold_start: bool,
}

impl PendingLineStatsRefresh {
    fn merge(self, newer: Self) -> Self {
        let scope = if self.repo_root == newer.repo_root {
            self.scope.merge(newer.scope)
        } else {
            newer.scope
        };
        Self {
            repo_root: newer.repo_root,
            request: self.request.merge(newer.request),
            scope,
            snapshot_epoch: newer.snapshot_epoch,
            cold_start: newer.cold_start,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AiTimelineRowSource {
    Item { item_key: String },
    Group { group_id: String },
    TurnDiff { turn_key: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiTimelineRow {
    id: String,
    thread_id: String,
    turn_id: String,
    last_sequence: u64,
    source: AiTimelineRowSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiTimelineGroup {
    id: String,
    thread_id: String,
    turn_id: String,
    last_sequence: u64,
    kind: String,
    status: hunk_codex::state::ItemStatus,
    title: String,
    summary: Option<String>,
    child_row_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiTextSelectionSurfaceSpec {
    surface_id: String,
    text: String,
    separator_before: String,
}

impl AiTextSelectionSurfaceSpec {
    fn new(surface_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            surface_id: surface_id.into(),
            text: text.into(),
            separator_before: String::new(),
        }
    }

    fn with_separator_before(mut self, separator_before: impl Into<String>) -> Self {
        self.separator_before = separator_before.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiTextSelectionSurfaceRange {
    surface_id: String,
    range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiTextSelection {
    row_id: String,
    surface_ranges: Vec<AiTextSelectionSurfaceRange>,
    full_text: String,
    anchor: usize,
    head: usize,
    dragging: bool,
}

impl AiTextSelection {
    fn new(
        row_id: String,
        surfaces: &[AiTextSelectionSurfaceSpec],
        surface_id: &str,
        index: usize,
    ) -> Self {
        let mut full_text = String::new();
        let mut surface_ranges = Vec::with_capacity(surfaces.len());
        let mut anchor = None;

        for surface in surfaces {
            full_text.push_str(surface.separator_before.as_str());
            let start = full_text.len();
            full_text.push_str(surface.text.as_str());
            let end = full_text.len();
            surface_ranges.push(AiTextSelectionSurfaceRange {
                surface_id: surface.surface_id.clone(),
                range: start..end,
            });
            if surface.surface_id == surface_id {
                anchor = Some(start + index.min(surface.text.len()));
            }
        }

        let clamped_index = anchor.unwrap_or(0).min(full_text.len());
        Self {
            row_id,
            surface_ranges,
            full_text,
            anchor: clamped_index,
            head: clamped_index,
            dragging: true,
        }
    }

    fn range(&self) -> Range<usize> {
        if self.head >= self.anchor {
            self.anchor..self.head
        } else {
            self.head..self.anchor
        }
    }

    fn selected_text(&self) -> Option<String> {
        let range = self.range();
        (!range.is_empty()).then(|| self.full_text[range].to_string())
    }

    fn range_for_surface(&self, surface_id: &str) -> Option<Range<usize>> {
        let surface = self
            .surface_ranges
            .iter()
            .find(|surface| surface.surface_id == surface_id)?;
        let selection_range = self.range();
        let start = selection_range.start.max(surface.range.start);
        let end = selection_range.end.min(surface.range.end);
        if start >= end {
            return None;
        }
        Some((start - surface.range.start)..(end - surface.range.start))
    }

    fn set_head_for_surface(&mut self, surface_id: &str, index: usize) {
        let Some(surface) = self
            .surface_ranges
            .iter()
            .find(|surface| surface.surface_id == surface_id)
        else {
            return;
        };
        self.head = surface.range.start + index.min(surface.range.len());
    }

    fn select_all(&mut self) {
        self.anchor = 0;
        self.head = self.full_text.len();
        self.dragging = false;
    }
}
