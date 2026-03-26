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

struct FileEditorTab {
    id: usize,
    path: String,
    files_editor: native_files_editor::SharedFilesEditor,
    loading: bool,
    error: Option<String>,
    dirty: bool,
    last_saved_text: Option<String>,
    reload_epoch: usize,
    reload_task: Task<()>,
    save_loading: bool,
    save_epoch: usize,
    save_task: Task<()>,
    markdown_preview_task: Task<()>,
    markdown_preview_blocks: Vec<MarkdownPreviewBlock>,
    markdown_preview_loading: bool,
    markdown_preview_revision: usize,
    markdown_preview: bool,
}

impl FileEditorTab {
    fn new(id: usize, path: String) -> Self {
        Self {
            id,
            path,
            files_editor: Rc::new(RefCell::new(crate::app::native_files_editor::FilesEditor::new())),
            loading: false,
            error: None,
            dirty: false,
            last_saved_text: None,
            reload_epoch: 0,
            reload_task: Task::ready(()),
            save_loading: false,
            save_epoch: 0,
            save_task: Task::ready(()),
            markdown_preview_task: Task::ready(()),
            markdown_preview_blocks: Vec::new(),
            markdown_preview_loading: false,
            markdown_preview_revision: 0,
            markdown_preview: false,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum AiComposerStatusKey {
    Draft(AiComposerDraftKey),
    Workspace(Option<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiComposerStatusTone {
    Danger,
    Warning,
}

fn ai_composer_status_tone(status: &str) -> Option<AiComposerStatusTone> {
    let lower = status.to_ascii_lowercase();
    if lower.contains("connected over websocket")
        || lower.contains("starting codex app server")
        || lower.starts_with("attached ")
        || lower.starts_with("submitted user input")
        || lower.starts_with("approval policy ")
    {
        return None;
    }

    if lower.contains("interrupt")
        || lower.contains("failed")
        || lower.contains("disconnected")
        || lower.contains("error")
    {
        return Some(AiComposerStatusTone::Danger);
    }

    if lower.contains("cannot")
        || lower.contains("disabled while a task is in progress")
        || lower.contains("remove attachments")
        || lower.contains("select a thread")
        || lower.contains("open a workspace")
        || lower.contains("no in-progress")
        || lower.contains("no supported")
        || lower.contains("unsupported")
        || lower.contains("skipped")
        || lower.contains("already attached")
        || lower.contains("no files were supported")
        || lower.contains("user input request no longer exists")
    {
        return Some(AiComposerStatusTone::Warning);
    }

    None
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
    skill_bindings: Vec<AiComposerSkillBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiValidatedPrompt {
    prompt: String,
    local_images: Vec<PathBuf>,
    selected_skills: Vec<AiPromptSkillReference>,
    skill_bindings: Vec<AiComposerSkillBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiComposerCompletionSyncKey {
    prompt: String,
    cursor: usize,
    session_settings_locked: bool,
    skills_generation: usize,
}

#[derive(Debug, Clone, Default)]
struct AiResolvedCurrentState {
    current_thread_id: Option<String>,
    current_thread_workspace_root: Option<PathBuf>,
    workspace_root: Option<PathBuf>,
    workspace_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiPromptSkillReference {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum AiTerminalSessionStatus {
    #[default]
    Idle,
    Running,
    Completed,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspaceTerminalKind {
    Ai,
    Files,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum FilesTerminalRestoreTarget {
    Editor,
    #[default]
    WorkspaceRoot,
}

#[derive(Debug, Clone, Default)]
struct AiTerminalSessionState {
    cwd: Option<PathBuf>,
    transcript: String,
    screen: Option<Arc<TerminalScreenSnapshot>>,
    last_command: Option<String>,
    status: AiTerminalSessionStatus,
    exit_code: Option<i32>,
    status_message: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct AiThreadTerminalState {
    open: bool,
    follow_output: bool,
    session: AiTerminalSessionState,
    pending_input: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct FilesProjectTerminalState {
    open: bool,
    follow_output: bool,
    session: AiTerminalSessionState,
    pending_input: Option<String>,
    restore_target: FilesTerminalRestoreTarget,
}

struct AiHiddenTerminalRuntimeHandle {
    runtime: AiTerminalRuntimeHandle,
    event_task: Task<()>,
}

#[derive(Debug, Clone)]
struct AiVisibleThreadProjectSection {
    project_root: PathBuf,
    project_label: String,
    threads: Vec<hunk_codex::state::ThreadSummary>,
    total_thread_count: usize,
    hidden_thread_count: usize,
    expanded: bool,
}

#[derive(Debug, Clone)]
enum AiThreadSidebarRowKind {
    ProjectHeader {
        project_root: PathBuf,
        project_label: String,
        total_thread_count: usize,
    },
    Thread {
        thread: hunk_codex::state::ThreadSummary,
        workspace_label: String,
    },
    EmptyProject {
        project_root: PathBuf,
    },
    ProjectFooter {
        project_root: PathBuf,
        hidden_thread_count: usize,
        expanded: bool,
    },
}

#[derive(Debug, Clone)]
struct AiThreadSidebarRow {
    kind: AiThreadSidebarRowKind,
}

#[derive(Debug, Clone)]
struct AiVisibleFrameState {
    project_count: usize,
    visible_thread_count: usize,
    threads_loading: bool,
    active_branch: String,
    active_workspace_label: String,
    pending_approvals: Arc<[AiPendingApproval]>,
    pending_user_inputs: Arc<[AiPendingUserInputRequest]>,
    selected_thread_id: Option<String>,
    pending_thread_start: Option<AiPendingThreadStart>,
    selected_thread_start_mode: Option<AiNewThreadStartMode>,
    show_worktree_base_branch_picker: bool,
    selected_worktree_base_branch: String,
    timeline_total_turn_count: usize,
    timeline_visible_turn_count: usize,
    timeline_hidden_turn_count: usize,
    timeline_visible_row_ids: Arc<[String]>,
    timeline_loading: bool,
    show_select_thread_empty_state: bool,
    show_no_turns_empty_state: bool,
    composer_feedback: Option<AiComposerFeedbackState>,
    composer_attachment_paths: Arc<[PathBuf]>,
    composer_send_waiting_on_connection: bool,
    composer_interrupt_available: bool,
    queued_message_count: usize,
    model_supports_image_inputs: bool,
    review_action_blocker: Option<String>,
    ai_publish_blocker: Option<String>,
    ai_publish_disabled: bool,
    ai_open_pr_disabled: bool,
    ai_managed_worktree_target: Option<WorkspaceTargetSummary>,
    ai_delete_worktree_blocker: Option<String>,
    terminal_cwd_label: String,
}

#[derive(Debug, Clone, Copy, Default)]
struct AiPerfDurationStats {
    count: u32,
    total_us: u64,
    max_us: u64,
}

impl AiPerfDurationStats {
    fn record(&mut self, duration: Duration) {
        let micros = duration.as_micros().min(u128::from(u64::MAX)) as u64;
        self.count = self.count.saturating_add(1);
        self.total_us = self.total_us.saturating_add(micros);
        self.max_us = self.max_us.max(micros);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiPerfTimelineRowKind {
    Message,
    Tool,
    Group,
    Diff,
    Plan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiPerfSidebarRowKind {
    ProjectHeader,
    Thread,
    EmptyProject,
    ProjectFooter,
}

#[derive(Debug, Clone, Default)]
struct AiPerfWindow {
    app_render: AiPerfDurationStats,
    footer_render: AiPerfDurationStats,
    visible_frame_build: AiPerfDurationStats,
    visible_frame_cache_hits: u32,
    visible_frame_invalidations: u32,
    visible_frame_invalidation_reasons: BTreeMap<&'static str, u32>,
    visible_frame_timeline_rows: AiPerfDurationStats,
    visible_frame_composer_feedback: AiPerfDurationStats,
    thread_sidebar_rebuild: AiPerfDurationStats,
    thread_sidebar_render: AiPerfDurationStats,
    thread_sidebar_visible_rows_total: u64,
    thread_sidebar_row_render: AiPerfDurationStats,
    thread_sidebar_project_header_row_render: AiPerfDurationStats,
    thread_sidebar_thread_row_render: AiPerfDurationStats,
    thread_sidebar_empty_project_row_render: AiPerfDurationStats,
    thread_sidebar_project_footer_row_render: AiPerfDurationStats,
    timeline_index_rebuild: AiPerfDurationStats,
    timeline_list_sync_count: u32,
    timeline_list_sync_row_ids_changed: u32,
    timeline_list_sync_follow_output_changed: u32,
    timeline_list_sync_visible_rows_total: u64,
    timeline_list_render: AiPerfDurationStats,
    timeline_list_render_visible_rows_total: u64,
    timeline_row_render: AiPerfDurationStats,
    timeline_row_skipped: u32,
    message_row_render: AiPerfDurationStats,
    tool_row_render: AiPerfDurationStats,
    group_row_render: AiPerfDurationStats,
    diff_row_render: AiPerfDurationStats,
    plan_row_render: AiPerfDurationStats,
    markdown_cache_hits: u32,
    markdown_cache_misses: u32,
    markdown_comrak_parse: AiPerfDurationStats,
    markdown_transform: AiPerfDurationStats,
    markdown_code_highlight: AiPerfDurationStats,
    markdown_code_block_count_total: u64,
    markdown_code_char_count_total: u64,
    markdown_parse: AiPerfDurationStats,
    markdown_selection_surfaces: AiPerfDurationStats,
    markdown_render_build: AiPerfDurationStats,
    markdown_render_block_count_total: u64,
    markdown_render_char_count_total: u64,
}

#[derive(Debug, Clone)]
struct AiPerfMetrics {
    window_started_at: Instant,
    window: AiPerfWindow,
}

impl Default for AiPerfMetrics {
    fn default() -> Self {
        Self {
            window_started_at: Instant::now(),
            window: AiPerfWindow::default(),
        }
    }
}

#[derive(Debug, Clone)]
struct AiComposerFeedbackActivity {
    label: String,
    started_at: Instant,
    animation_key: String,
}

#[derive(Debug, Clone)]
enum AiComposerFeedbackState {
    Status {
        message: String,
        tone: AiComposerStatusTone,
    },
    Activity(AiComposerFeedbackActivity),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiComposerSkillBinding {
    token: String,
    range: Range<usize>,
    reference: AiPromptSkillReference,
}

#[derive(Debug, Clone)]
struct AiPendingThreadStart {
    workspace_key: String,
    prompt: String,
    local_images: Vec<PathBuf>,
    skill_bindings: Vec<AiComposerSkillBinding>,
    started_at: Instant,
    start_mode: AiNewThreadStartMode,
    thread_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiPendingSteer {
    thread_id: String,
    turn_id: String,
    prompt: String,
    local_images: Vec<PathBuf>,
    selected_skills: Vec<AiPromptSkillReference>,
    skill_bindings: Vec<AiComposerSkillBinding>,
    accepted_after_sequence: u64,
    started_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum AiQueuedUserMessageStatus {
    #[default]
    Queued,
    PendingConfirmation { accepted_after_sequence: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiQueuedUserMessage {
    thread_id: String,
    prompt: String,
    local_images: Vec<PathBuf>,
    selected_skills: Vec<AiPromptSkillReference>,
    skill_bindings: Vec<AiComposerSkillBinding>,
    queued_at: Instant,
    status: AiQueuedUserMessageStatus,
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
    pending_steers: Vec<AiPendingSteer>,
    queued_messages: Vec<AiQueuedUserMessage>,
    interrupt_restore_queued_thread_ids: BTreeSet<String>,
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
    skills: Vec<codex_app_server_protocol::SkillMetadata>,
    include_hidden_models: bool,
    selected_model: Option<String>,
    selected_effort: Option<String>,
    selected_collaboration_mode: AiCollaborationModeSelection,
    selected_service_tier: AiServiceTierSelection,
    review_mode_thread_ids: BTreeSet<String>,
    mad_max_mode: bool,
    draft_workspace_root_override: Option<PathBuf>,
    terminal_open: bool,
    terminal_follow_output: bool,
    terminal_height_px: f32,
    terminal_input_draft: String,
    terminal_session: AiTerminalSessionState,
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
            pending_steers: Vec::new(),
            queued_messages: Vec::new(),
            interrupt_restore_queued_thread_ids: BTreeSet::new(),
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
            skills: Vec::new(),
            include_hidden_models: true,
            selected_model: None,
            selected_effort: None,
            selected_collaboration_mode: AiCollaborationModeSelection::Default,
            selected_service_tier: AiServiceTierSelection::Standard,
            review_mode_thread_ids: BTreeSet::new(),
            mad_max_mode: false,
            draft_workspace_root_override: None,
            terminal_open: false,
            terminal_follow_output: true,
            terminal_height_px: 220.0,
            terminal_input_draft: String::new(),
            terminal_session: AiTerminalSessionState::default(),
        }
    }
}

struct AiHiddenRuntimeHandle {
    command_tx: mpsc::Sender<AiWorkerCommand>,
    worker_thread: JoinHandle<()>,
    event_task: Task<()>,
    generation: usize,
}

struct AiTerminalRuntimeHandle {
    thread_id: String,
    handle: TerminalSessionHandle,
    generation: usize,
}

struct FilesTerminalRuntimeHandle {
    project_key: String,
    handle: TerminalSessionHandle,
    generation: usize,
}

struct FilesHiddenTerminalRuntimeHandle {
    runtime: FilesTerminalRuntimeHandle,
    event_task: Task<()>,
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
    TurnPlan { turn_key: String },
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

#[derive(Debug, Clone)]
struct AiMarkdownRowCacheEntry {
    markdown: String,
    blocks: Arc<[MarkdownPreviewBlock]>,
    selection_surfaces: Arc<[AiTextSelectionSurfaceSpec]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiTextSelectionSurfaceRange {
    surface_id: String,
    range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
struct AiPressedMarkdownLink {
    surface_id: String,
    raw_target: String,
    mouse_down_position: gpui::Point<gpui::Pixels>,
    dragged: bool,
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

        let clamped_index = clamp_utf8_boundary(&full_text, anchor.unwrap_or(0));
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
        let anchor = clamp_utf8_boundary(&self.full_text, self.anchor);
        let head = clamp_utf8_boundary(&self.full_text, self.head);
        if head >= anchor {
            anchor..head
        } else {
            head..anchor
        }
    }

    fn selected_text(&self) -> Option<String> {
        let range = self.range();
        if range.is_empty() {
            return None;
        }
        self.full_text.get(range).map(ToOwned::to_owned)
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
        let next_head = surface.range.start + index.min(surface.range.len());
        self.head = clamp_utf8_boundary(&self.full_text, next_head);
    }

    fn select_all(&mut self) {
        self.anchor = 0;
        self.head = self.full_text.len();
        self.dragging = false;
    }
}

fn clamp_utf8_boundary(text: &str, index: usize) -> usize {
    let mut clamped = index.min(text.len());
    while clamped > 0 && !text.is_char_boundary(clamped) {
        clamped -= 1;
    }
    clamped
}
