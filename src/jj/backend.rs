use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use futures::executor::{block_on, block_on_stream};
use jj_lib::commit::Commit;
use jj_lib::config::{ConfigLayer, ConfigSource, StackedConfig};
use jj_lib::conflicts::{
    ConflictMarkerStyle, ConflictMaterializeOptions, MaterializedTreeDiffEntry,
    materialized_diff_stream,
};
use jj_lib::copies::CopyRecords;
use jj_lib::diff_presentation::LineCompareMode;
use jj_lib::diff_presentation::unified::{
    DiffLineType, UnifiedDiffHunk, git_diff_part, unified_diff_hunks,
};
use jj_lib::git::{
    self, GitProgress, GitSidebandLineTerminator, GitSubprocessCallback, GitSubprocessOptions,
    REMOTE_NAME_FOR_LOCAL_GIT_REPO,
};
use jj_lib::matchers::{EverythingMatcher, FilesMatcher, NothingMatcher};
use jj_lib::merge::{Diff, MergedTreeValue};
use jj_lib::object_id::ObjectId as _;
use jj_lib::op_store::RefTarget;
use jj_lib::ref_name::{RefName, RefNameBuf, RemoteName, WorkspaceName};
use jj_lib::refs::{BookmarkPushAction, classify_bookmark_push_action};
use jj_lib::repo::{ReadonlyRepo, Repo as _, StoreFactories};
use jj_lib::repo_path::RepoPathBuf;
use jj_lib::revset::RevsetExpression;
use jj_lib::rewrite::{
    CommitWithSelection, MoveCommitsLocation, MoveCommitsTarget, RebaseOptions, move_commits,
    restore_tree, squash_commits,
};
use jj_lib::settings::UserSettings;
use jj_lib::str_util::StringExpression;
use jj_lib::working_copy::SnapshotOptions;
use jj_lib::workspace::{Workspace, default_working_copy_factories};

use super::*;

pub(super) struct RepoContext {
    pub(super) root: PathBuf,
    pub(super) settings: UserSettings,
    pub(super) workspace: Workspace,
    pub(super) repo: Arc<ReadonlyRepo>,
    pub(super) nested_repo_roots_cache: OnceCell<BTreeSet<String>>,
}

pub(super) struct RenderedPatch {
    pub(super) patch: String,
}

struct NoopGitSubprocessCallback;

impl GitSubprocessCallback for NoopGitSubprocessCallback {
    fn needs_progress(&self) -> bool {
        false
    }

    fn progress(&mut self, _: &GitProgress) -> io::Result<()> {
        Ok(())
    }

    fn local_sideband(&mut self, _: &[u8], _: Option<GitSidebandLineTerminator>) -> io::Result<()> {
        Ok(())
    }

    fn remote_sideband(
        &mut self,
        _: &[u8],
        _: Option<GitSidebandLineTerminator>,
    ) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn load_repo_context(cwd: &Path, refresh_snapshot: bool) -> Result<RepoContext> {
    let root = discover_repo_root(cwd)?;
    load_repo_context_at_root(&root, refresh_snapshot)
}

pub(super) fn load_repo_context_at_root(
    repo_root: &Path,
    refresh_snapshot: bool,
) -> Result<RepoContext> {
    let root = discover_repo_root(repo_root)?;
    let settings = load_user_settings(Some(&root))?;
    let store_factories = StoreFactories::default();
    let working_copy_factories = default_working_copy_factories();

    let workspace = Workspace::load(&settings, &root, &store_factories, &working_copy_factories)
        .with_context(|| format!("failed to load jj workspace at {}", root.display()))?;
    let repo = workspace
        .repo_loader()
        .load_at_head()
        .context("failed to load jj repository")?;

    let mut context = RepoContext {
        root,
        settings,
        workspace,
        repo,
        nested_repo_roots_cache: OnceCell::new(),
    };
    if refresh_snapshot {
        refresh_working_copy_snapshot(&mut context)?;
    }
    Ok(context)
}

include!("backend/settings.rs");
include!("backend/snapshot_diff.rs");
include!("backend/graph.rs");
include!("backend/operations.rs");
include!("backend/workspace.rs");

#[cfg(test)]
mod parity_tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::jj::{
        checkout_or_create_bookmark, commit_staged, load_snapshot, move_bookmark_to_revision,
    };

    use super::*;

    #[test]
    fn resolve_remote_url_from_jj_lib_matches_cli_remote_list() {
        let fixture = TempRepo::new("unit-remote-url-parity");

        run_jj(
            fixture.path(),
            [
                "git",
                "remote",
                "add",
                "origin",
                "https://gitlab.com/example-org/hunk.git",
            ],
        );

        let context = load_repo_context_at_root(fixture.path(), false)
            .expect("repo context should load for remote URL test");
        let resolved = resolve_remote_url_from_jj_lib(&context, "origin")
            .expect("jj-lib remote resolution should succeed")
            .expect("origin should resolve from jj-lib");
        let cli = remote_url_from_cli_list(fixture.path(), "origin")
            .expect("origin should resolve from jj git remote list");

        assert_eq!(resolved, cli);
    }

    #[test]
    fn reorder_bookmark_tip_with_jj_lib_matches_cli_rebase_insert_after() {
        let jjlib_fixture = TempRepo::new("unit-reorder-parity-jjlib");
        let cli_fixture = TempRepo::new("unit-reorder-parity-cli");

        seed_three_revision_stack(jjlib_fixture.path(), "stack");
        seed_three_revision_stack(cli_fixture.path(), "stack");

        let mut context = load_repo_context_at_root(jjlib_fixture.path(), true)
            .expect("jj-lib context should load before reorder");
        reorder_bookmark_tip_older(&mut context, "stack").expect("jj-lib reorder should succeed");
        reorder_bookmark_tip_with_cli_equivalent(cli_fixture.path(), "stack");

        let jjlib_snapshot =
            load_snapshot(jjlib_fixture.path()).expect("jj-lib snapshot should load after reorder");
        let cli_snapshot =
            load_snapshot(cli_fixture.path()).expect("cli snapshot should load after reorder");

        let jjlib_subjects: Vec<_> = jjlib_snapshot
            .bookmark_revisions
            .iter()
            .take(3)
            .map(|revision| revision.subject.clone())
            .collect();
        let cli_subjects: Vec<_> = cli_snapshot
            .bookmark_revisions
            .iter()
            .take(3)
            .map(|revision| revision.subject.clone())
            .collect();
        assert_eq!(jjlib_subjects, cli_subjects);
    }

    struct TempRepo {
        path: PathBuf,
    }

    impl TempRepo {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("hunk-{prefix}-{unique}"));
            fs::create_dir_all(&path).expect("temp repo directory should be created");

            run_jj(&path, ["git", "init", "--colocate"]);
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn seed_three_revision_stack(repo_root: &Path, bookmark_name: &str) {
        write_file(repo_root.join("tracked-1.txt"), "line one\n");
        commit_staged(repo_root, "initial commit").expect("initial commit should succeed");
        checkout_or_create_bookmark(repo_root, bookmark_name)
            .expect("creating bookmark should succeed");

        write_file(repo_root.join("tracked-2.txt"), "line two\n");
        commit_staged(repo_root, "stack second commit").expect("second commit should succeed");

        write_file(repo_root.join("tracked-3.txt"), "line three\n");
        commit_staged(repo_root, "stack third commit").expect("third commit should succeed");
    }

    fn reorder_bookmark_tip_with_cli_equivalent(repo_root: &Path, bookmark_name: &str) {
        let before = load_snapshot(repo_root).expect("snapshot should load before CLI reorder");
        let tip_id = before
            .bookmark_revisions
            .first()
            .map(|revision| revision.id.clone())
            .expect("bookmark should include a tip revision");
        let anchor_id = before
            .bookmark_revisions
            .get(2)
            .map(|revision| revision.id.clone())
            .unwrap_or_else(|| root_revision_id(repo_root));

        run_jj(
            repo_root,
            ["rebase", "-r", tip_id.as_str(), "-A", anchor_id.as_str()],
        );

        let wc_parent = run_jj_capture(
            repo_root,
            [
                "log",
                "-r",
                "parents(@)",
                "-n",
                "1",
                "--no-graph",
                "-T",
                "commit_id",
            ],
        )
        .trim()
        .to_string();
        move_bookmark_to_revision(repo_root, bookmark_name, wc_parent.as_str())
            .expect("bookmark should move to parent of working copy after CLI reorder");
    }

    fn root_revision_id(repo_root: &Path) -> String {
        run_jj_capture(
            repo_root,
            [
                "log",
                "-r",
                "root()",
                "-n",
                "1",
                "--no-graph",
                "-T",
                "commit_id",
            ],
        )
        .trim()
        .to_string()
    }

    fn write_file(path: PathBuf, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directories should be created");
        }
        fs::write(path, contents).expect("file should be written");
    }

    fn remote_url_from_cli_list(repo_root: &Path, remote_name: &str) -> Option<String> {
        let output = run_jj_capture(repo_root, ["git", "remote", "list"]);
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let mut parts = trimmed.split_whitespace();
            let Some(name) = parts.next() else {
                continue;
            };
            let Some(url) = parts.next() else {
                continue;
            };
            if name == remote_name {
                return Some(url.to_string());
            }
        }
        None
    }

    fn run_jj<const N: usize>(cwd: &Path, args: [&str; N]) {
        let status = Command::new("jj")
            .args(args)
            .current_dir(cwd)
            .status()
            .expect("jj command should run");
        assert!(status.success(), "jj command failed");
    }

    fn run_jj_capture<const N: usize>(cwd: &Path, args: [&str; N]) -> String {
        let output = Command::new("jj")
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("jj command should run");
        assert!(
            output.status.success(),
            "jj command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }
}
