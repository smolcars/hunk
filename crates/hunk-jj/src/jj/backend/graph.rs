use std::cmp::Ordering;
use std::collections::BinaryHeap;

use jj_lib::backend::CommitId;

#[derive(Debug)]
struct PendingCommit {
    id: CommitId,
    id_hex: String,
    unix_time: i64,
}

#[derive(Debug)]
struct GraphCommitWindow {
    commits: Vec<Commit>,
    has_more: bool,
}

#[derive(Debug)]
struct GraphSeedCandidate {
    id: CommitId,
    priority: u8,
    name: String,
    remote: Option<String>,
}

const GRAPH_MAX_LOCAL_BOOKMARK_SEEDS: usize = 64;
const GRAPH_MAX_REMOTE_BOOKMARK_SEEDS: usize = 24;

impl PartialEq for PendingCommit {
    fn eq(&self, other: &Self) -> bool {
        self.id_hex == other.id_hex
    }
}

impl Eq for PendingCommit {}

impl PartialOrd for PendingCommit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PendingCommit {
    fn cmp(&self, other: &Self) -> Ordering {
        self.unix_time
            .cmp(&other.unix_time)
            .then_with(|| self.id_hex.cmp(&other.id_hex))
    }
}

pub(super) fn build_graph_snapshot_from_context(
    context: &RepoContext,
    options: GraphSnapshotOptions,
) -> Result<GraphSnapshot> {
    let options = normalized_graph_options(options);
    let wc_commit = current_wc_commit(context)?;
    let working_copy_commit_id = wc_commit.id().hex().to_string();
    let working_copy_parent_commit_id = wc_commit.parent_ids().first().map(|id| id.hex());

    let current_bookmarks = current_bookmarks_from_context(context)?;
    let active_bookmark = {
        let branch_name = select_snapshot_branch_name(
            &current_bookmarks,
            load_active_bookmark_preference(&context.root),
            git_head_branch_name_from_context(context),
        );
        (branch_name != "detached").then_some(branch_name)
    };
    let active_bookmark_target_id = active_bookmark
        .as_ref()
        .and_then(|name| local_bookmark_target_hex(context, name.as_str()));

    let seed_ids = graph_seed_commit_ids(
        context,
        active_bookmark.as_deref(),
        working_copy_parent_commit_id.as_deref(),
        options.include_remote_bookmarks,
    )?;
    let graph_window = load_graph_window_commits(context, &seed_ids, options)?;
    let graph_commits = graph_window.commits;

    let bookmark_refs_by_commit = graph_bookmarks_by_commit(
        context,
        active_bookmark.as_deref(),
        options.include_remote_bookmarks,
    )?;

    let node_ids = graph_commits
        .iter()
        .map(|commit| commit.id().hex())
        .collect::<BTreeSet<_>>();

    let mut nodes = graph_commits
        .iter()
        .map(|commit| {
            let id = commit.id().hex();
            let mut bookmarks = bookmark_refs_by_commit.get(id.as_str()).cloned().unwrap_or_default();
            sort_graph_bookmark_refs(&mut bookmarks);
            GraphNode {
                id: id.clone(),
                subject: graph_commit_subject(commit),
                unix_time: commit.committer().timestamp.timestamp.0 / 1000,
                bookmarks,
                is_working_copy_parent: working_copy_parent_commit_id.as_deref() == Some(id.as_str()),
                is_active_bookmark_target: active_bookmark_target_id.as_deref() == Some(id.as_str()),
            }
        })
        .collect::<Vec<_>>();

    nodes.sort_by(|left, right| {
        right
            .unix_time
            .cmp(&left.unix_time)
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut edges = Vec::new();
    for commit in &graph_commits {
        let from = commit.id().hex();
        for parent_id in commit.parent_ids() {
            let to = parent_id.hex();
            if !node_ids.contains(to.as_str()) {
                continue;
            }
            edges.push(GraphEdge {
                from: from.clone(),
                to,
            });
        }
    }
    edges.sort_by(|left, right| {
        left.from
            .cmp(&right.from)
            .then_with(|| left.to.cmp(&right.to))
    });

    Ok(GraphSnapshot {
        root: context.root.clone(),
        active_bookmark,
        working_copy_commit_id,
        working_copy_parent_commit_id,
        nodes,
        edges,
        has_more: graph_window.has_more,
        next_offset: graph_next_offset(graph_commits.len(), options, graph_window.has_more),
    })
}

fn normalized_graph_options(options: GraphSnapshotOptions) -> GraphSnapshotOptions {
    GraphSnapshotOptions {
        max_nodes: options.max_nodes.max(1),
        offset: options.offset,
        include_remote_bookmarks: options.include_remote_bookmarks,
    }
}

fn graph_seed_commit_ids(
    context: &RepoContext,
    active_bookmark: Option<&str>,
    wc_parent_id: Option<&str>,
    include_remote_bookmarks: bool,
) -> Result<Vec<CommitId>> {
    let mut seen = BTreeSet::new();
    let mut ids = Vec::new();
    let view = context.repo.view();
    let current_bookmarks = current_bookmarks_from_context(context)?;

    let mut local_candidates = Vec::new();
    for (name, target) in view.local_bookmarks() {
        let Some(commit_id) = target.as_normal().cloned() else {
            continue;
        };
        let bookmark_name = name.as_str().to_string();
        let priority = if active_bookmark == Some(bookmark_name.as_str()) {
            3
        } else if current_bookmarks.contains(bookmark_name.as_str()) {
            2
        } else {
            1
        };
        local_candidates.push(GraphSeedCandidate {
            id: commit_id,
            priority,
            name: bookmark_name,
            remote: None,
        });
    }
    local_candidates.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.hex().cmp(&right.id.hex()))
    });
    let mut local_seed_count = 0usize;
    for candidate in local_candidates {
        if push_unique_graph_seed(&mut ids, &mut seen, candidate.id) {
            local_seed_count = local_seed_count.saturating_add(1);
            if local_seed_count >= GRAPH_MAX_LOCAL_BOOKMARK_SEEDS {
                break;
            }
        }
    }

    if include_remote_bookmarks {
        let mut remote_candidates = Vec::new();
        for (remote, _) in view.remote_views() {
            if remote == REMOTE_NAME_FOR_LOCAL_GIT_REPO {
                continue;
            }
            let remote_name = remote.as_str().to_string();
            for (name, targets) in view.local_remote_bookmarks(remote) {
                let Some(commit_id) = targets.remote_ref.target.as_normal().cloned() else {
                    continue;
                };
                let bookmark_name = name.as_str().to_string();
                let mut priority = 0_u8;
                if active_bookmark == Some(bookmark_name.as_str()) {
                    priority = priority.saturating_add(3);
                }
                if current_bookmarks.contains(bookmark_name.as_str()) {
                    priority = priority.saturating_add(2);
                }
                if targets.remote_ref.is_tracked() {
                    priority = priority.saturating_add(1);
                }
                remote_candidates.push(GraphSeedCandidate {
                    id: commit_id,
                    priority,
                    name: bookmark_name,
                    remote: Some(remote_name.clone()),
                });
            }
        }
        remote_candidates.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.remote.cmp(&right.remote))
                .then_with(|| left.id.hex().cmp(&right.id.hex()))
        });
        let mut remote_seed_count = 0usize;
        for candidate in remote_candidates {
            if push_unique_graph_seed(&mut ids, &mut seen, candidate.id) {
                remote_seed_count = remote_seed_count.saturating_add(1);
                if remote_seed_count >= GRAPH_MAX_REMOTE_BOOKMARK_SEEDS {
                    break;
                }
            }
        }
    }

    if let Some(wc_parent_id) = wc_parent_id {
        let Some(commit_id) = CommitId::try_from_hex(wc_parent_id) else {
            return Err(anyhow!("invalid working-copy parent id '{wc_parent_id}'"));
        };
        push_unique_graph_seed(&mut ids, &mut seen, commit_id);
    }

    if ids.is_empty() {
        let wc_commit = current_wc_commit(context)?;
        ids.push(wc_commit.id().clone());
    }

    Ok(ids)
}

fn push_unique_graph_seed(
    ids: &mut Vec<CommitId>,
    seen: &mut BTreeSet<String>,
    commit_id: CommitId,
) -> bool {
    if seen.insert(commit_id.hex()) {
        ids.push(commit_id);
        return true;
    }
    false
}

fn load_graph_window_commits(
    context: &RepoContext,
    seed_ids: &[CommitId],
    options: GraphSnapshotOptions,
) -> Result<GraphCommitWindow> {
    let target_len = options
        .offset
        .saturating_add(options.max_nodes)
        .saturating_add(1);
    let mut queue = BinaryHeap::new();
    let mut enqueued = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut commits = Vec::new();

    for seed_id in seed_ids {
        enqueue_pending_commit(context, seed_id, &mut queue, &mut enqueued)?;
    }

    while let Some(next) = queue.pop() {
        if !visited.insert(next.id_hex.clone()) {
            continue;
        }
        let commit = context
            .repo
            .store()
            .get_commit(&next.id)
            .with_context(|| format!("failed to load commit {}", next.id_hex))?;

        for parent_id in commit.parent_ids() {
            enqueue_pending_commit(context, parent_id, &mut queue, &mut enqueued)?;
        }

        commits.push(commit);
        if commits.len() >= target_len {
            break;
        }
    }

    let has_more = commits.len() > options.offset.saturating_add(options.max_nodes);
    let start = options.offset.min(commits.len());
    let end = start.saturating_add(options.max_nodes).min(commits.len());
    Ok(GraphCommitWindow {
        commits: commits[start..end].to_vec(),
        has_more,
    })
}

fn graph_next_offset(node_count: usize, options: GraphSnapshotOptions, has_more: bool) -> Option<usize> {
    if !has_more {
        return None;
    }
    Some(options.offset.saturating_add(node_count))
}

fn enqueue_pending_commit(
    context: &RepoContext,
    commit_id: &CommitId,
    queue: &mut BinaryHeap<PendingCommit>,
    enqueued: &mut BTreeSet<String>,
) -> Result<()> {
    let id_hex = commit_id.hex();
    if !enqueued.insert(id_hex.clone()) {
        return Ok(());
    }

    let commit = context
        .repo
        .store()
        .get_commit(commit_id)
        .with_context(|| format!("failed to load commit {}", id_hex))?;
    queue.push(PendingCommit {
        id: commit_id.clone(),
        id_hex,
        unix_time: commit.committer().timestamp.timestamp.0 / 1000,
    });
    Ok(())
}

fn graph_bookmarks_by_commit(
    context: &RepoContext,
    active_bookmark: Option<&str>,
    include_remote_bookmarks: bool,
) -> Result<BTreeMap<String, Vec<GraphBookmarkRef>>> {
    let mut by_commit = BTreeMap::<String, Vec<GraphBookmarkRef>>::new();
    let view = context.repo.view();

    for (name, target) in view.local_bookmarks() {
        let bookmark_name = name.as_str().to_string();
        let (tracked, needs_push) = bookmark_remote_sync_state(context, bookmark_name.as_str());
        let target_ids = ref_target_commit_ids(target);
        if target_ids.is_empty() {
            continue;
        }
        for target_id in target_ids {
            by_commit
                .entry(target_id)
                .or_default()
                .push(GraphBookmarkRef {
                    name: bookmark_name.clone(),
                    remote: None,
                    scope: GraphBookmarkScope::Local,
                    is_active: active_bookmark == Some(bookmark_name.as_str()),
                    tracked,
                    needs_push: needs_push > 0,
                    conflicted: target.has_conflict(),
                });
        }
    }

    if include_remote_bookmarks {
        for (remote, _) in view.remote_views() {
            if remote == REMOTE_NAME_FOR_LOCAL_GIT_REPO {
                continue;
            }
            let remote_name = remote.as_str().to_string();
            for (name, targets) in view.local_remote_bookmarks(remote) {
                let target_ids = ref_target_commit_ids(&targets.remote_ref.target);
                if target_ids.is_empty() {
                    continue;
                }
                let bookmark_name = name.as_str().to_string();
                let needs_push =
                    matches!(classify_bookmark_push_action(targets), BookmarkPushAction::Update(_));
                for target_id in target_ids {
                    by_commit
                        .entry(target_id)
                        .or_default()
                        .push(GraphBookmarkRef {
                            name: bookmark_name.clone(),
                            remote: Some(remote_name.clone()),
                            scope: GraphBookmarkScope::Remote,
                            is_active: active_bookmark == Some(bookmark_name.as_str()),
                            tracked: targets.remote_ref.is_tracked(),
                            needs_push,
                            conflicted: targets.remote_ref.target.has_conflict(),
                        });
                }
            }
        }
    }

    Ok(by_commit)
}

fn graph_commit_subject(commit: &Commit) -> String {
    commit
        .description()
        .lines()
        .next()
        .map(str::trim)
        .filter(|subject| !subject.is_empty())
        .unwrap_or("(no description)")
        .to_string()
}

fn sort_graph_bookmark_refs(bookmarks: &mut [GraphBookmarkRef]) {
    bookmarks.sort_by(|left, right| {
        graph_bookmark_scope_rank(left.scope)
            .cmp(&graph_bookmark_scope_rank(right.scope))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.remote.cmp(&right.remote))
    });
}

fn graph_bookmark_scope_rank(scope: GraphBookmarkScope) -> u8 {
    match scope {
        GraphBookmarkScope::Local => 0,
        GraphBookmarkScope::Remote => 1,
    }
}

fn ref_target_commit_ids(target: &RefTarget) -> Vec<String> {
    if let Some(normal_id) = target.as_normal() {
        return vec![normal_id.hex()];
    }

    target.added_ids().map(|id| id.hex()).collect()
}
