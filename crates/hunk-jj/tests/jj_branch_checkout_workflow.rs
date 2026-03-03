use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_domain::config::{ReviewProviderKind, ReviewProviderMapping};
use hunk_jj::jj::{
    abandon_bookmark_head, checkout_or_create_bookmark,
    checkout_or_create_bookmark_with_change_transfer, commit_staged, describe_bookmark_head,
    load_snapshot, move_bookmark_to_revision, rename_bookmark, reorder_bookmark_tip_older,
    restore_working_copy_from_revision, review_url_for_bookmark,
    review_url_for_bookmark_with_provider_map, squash_bookmark_head_into_parent,
};

#[test]
fn checkout_existing_bookmark_switches_without_crashing() {
    let fixture = TempRepo::new("checkout-existing-bookmark");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");

    checkout_or_create_bookmark(fixture.path(), "master")
        .expect("creating master bookmark should succeed");

    checkout_or_create_bookmark(fixture.path(), "feature")
        .expect("creating feature bookmark should succeed");

    write_file(fixture.path().join("tracked.txt"), "line one\nline two\n");
    commit_staged(fixture.path(), "feature commit").expect("feature commit should succeed");

    checkout_or_create_bookmark(fixture.path(), "master")
        .expect("switching to existing master bookmark should succeed");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load after checkout");
    assert_eq!(snapshot.branch_name, "master");
    assert!(
        snapshot.files.is_empty(),
        "switching to an existing bookmark should not surface committed diff as working changes"
    );
}

#[test]
fn committing_on_checked_out_bookmark_advances_that_bookmark() {
    let fixture = TempRepo::new("checkout-bookmark-commit-advance");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "master")
        .expect("creating master bookmark should succeed");

    write_file(fixture.path().join("tracked.txt"), "line one\nline two\n");
    commit_staged(fixture.path(), "master update should move bookmark")
        .expect("commit on checked-out bookmark should succeed");

    let master_log = run_jj_capture(
        fixture.path(),
        ["log", "-r", "master", "-n", "1", "--no-graph"],
    );
    assert!(
        master_log.contains("master update should move bookmark"),
        "master bookmark should point to latest commit after commit_staged"
    );
}

#[test]
fn creating_bookmark_can_move_uncommitted_changes_off_current_bookmark() {
    let fixture = TempRepo::new("create-bookmark-move-uncommitted");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("creating main bookmark should succeed");

    write_file(fixture.path().join("tracked.txt"), "line one\nline two\n");
    checkout_or_create_bookmark_with_change_transfer(fixture.path(), "feature", true)
        .expect("creating feature bookmark should succeed");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load after branch create");
    assert_eq!(snapshot.branch_name, "feature");
    assert!(
        snapshot.files.iter().any(|file| file.path == "tracked.txt"),
        "uncommitted changes should remain in working copy after moving to feature"
    );

    let bookmark_listing = run_jj_capture(fixture.path(), ["bookmark", "list", "main", "feature"]);
    assert!(
        bookmark_listing.contains("main:"),
        "main bookmark should still exist after creating feature"
    );
    assert!(
        bookmark_listing.contains("feature:"),
        "feature bookmark should exist after creation"
    );

    let main_target = bookmark_listing
        .lines()
        .find(|line| line.starts_with("main:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .expect("main target commit should be listed")
        .to_string();
    let feature_target = bookmark_listing
        .lines()
        .find(|line| line.starts_with("feature:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .expect("feature target commit should be listed")
        .to_string();
    assert_ne!(
        main_target, feature_target,
        "moving changes should leave main and feature on different commits"
    );
}

#[test]
fn switching_to_existing_bookmark_can_move_uncommitted_changes() {
    let fixture = TempRepo::new("switch-bookmark-move-uncommitted");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("creating main bookmark should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature")
        .expect("creating feature bookmark should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("switching back to main should succeed");

    write_file(fixture.path().join("tracked.txt"), "line one\nline two\n");
    checkout_or_create_bookmark_with_change_transfer(fixture.path(), "feature", true)
        .expect("switching to feature with moved changes should succeed");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load after branch switch");
    assert_eq!(snapshot.branch_name, "feature");
    assert!(
        snapshot.files.iter().any(|file| file.path == "tracked.txt"),
        "uncommitted changes should remain in working copy after switching with move enabled"
    );
}

#[test]
fn restoring_working_copy_revision_recovers_changes_after_plain_bookmark_switch() {
    let fixture = TempRepo::new("restore-working-copy-after-switch");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("creating main bookmark should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature")
        .expect("creating feature bookmark should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("switching back to main should succeed");

    write_file(fixture.path().join("tracked.txt"), "line one\nline two\n");
    write_file(fixture.path().join("scratch.txt"), "temporary text\n");
    let before_switch = load_snapshot(fixture.path()).expect("snapshot should load before switch");
    let source_revision = current_working_copy_revision(fixture.path());
    assert!(
        before_switch
            .files
            .iter()
            .any(|file| file.path == "tracked.txt"),
        "tracked change should exist before switching"
    );
    assert!(
        before_switch
            .files
            .iter()
            .any(|file| file.path == "scratch.txt"),
        "untracked change should exist before switching"
    );

    checkout_or_create_bookmark(fixture.path(), "feature")
        .expect("switching to feature without move should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("switching back to main should succeed");

    let after_switch = load_snapshot(fixture.path()).expect("snapshot should load after switch");
    assert!(
        after_switch.files.is_empty(),
        "plain switch should not carry uncommitted changes"
    );

    restore_working_copy_from_revision(fixture.path(), source_revision.as_str())
        .expect("restoring working-copy revision should succeed");
    let after_restore = load_snapshot(fixture.path()).expect("snapshot should load after restore");
    assert!(
        after_restore
            .files
            .iter()
            .any(|file| file.path == "tracked.txt"),
        "tracked change should be restored"
    );
    assert!(
        after_restore
            .files
            .iter()
            .any(|file| file.path == "scratch.txt"),
        "untracked change should be restored"
    );
}

#[test]
fn renaming_bookmark_updates_active_bookmark_and_listing() {
    let fixture = TempRepo::new("rename-bookmark-active");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature-old")
        .expect("creating source bookmark should succeed");

    rename_bookmark(fixture.path(), "feature-old", "feature-new")
        .expect("renaming bookmark should succeed");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load after rename");
    assert_eq!(
        snapshot.branch_name, "feature-new",
        "active bookmark should update to the renamed bookmark"
    );

    let bookmark_listing = run_jj_capture(
        fixture.path(),
        ["bookmark", "list", "feature-old", "feature-new"],
    );
    assert!(
        bookmark_listing.contains("feature-new:"),
        "renamed bookmark should be listed"
    );
    assert!(
        !bookmark_listing.contains("feature-old:"),
        "old bookmark name should no longer exist"
    );
}

#[test]
fn renaming_bookmark_rejects_existing_target() {
    let fixture = TempRepo::new("rename-bookmark-existing-target");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature-old")
        .expect("creating source bookmark should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature-existing")
        .expect("creating target bookmark should succeed");

    let err = rename_bookmark(fixture.path(), "feature-old", "feature-existing")
        .expect_err("renaming should fail when destination bookmark already exists");
    assert!(
        err.to_string().contains("already exists"),
        "error should explain destination bookmark conflict"
    );
}

#[test]
fn snapshot_includes_revision_stack_for_active_bookmark() {
    let fixture = TempRepo::new("bookmark-revision-stack");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "stack")
        .expect("creating stack bookmark should succeed");

    write_file(fixture.path().join("tracked.txt"), "line one\nline two\n");
    commit_staged(fixture.path(), "stack second commit").expect("second commit should succeed");

    write_file(
        fixture.path().join("tracked.txt"),
        "line one\nline two\nline three\n",
    );
    commit_staged(fixture.path(), "stack third commit").expect("third commit should succeed");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load");
    assert_eq!(snapshot.branch_name, "stack");
    assert!(
        snapshot.bookmark_revisions.len() >= 2,
        "revision stack should include at least latest commits"
    );
    assert_eq!(
        snapshot.bookmark_revisions[0].subject, "stack third commit",
        "latest revision should be first in stack"
    );
    assert_eq!(
        snapshot.bookmark_revisions[1].subject, "stack second commit",
        "stack should be ordered from newest to oldest"
    );
}

#[test]
fn describing_bookmark_head_updates_latest_revision_subject() {
    let fixture = TempRepo::new("describe-bookmark-head");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "describe-me")
        .expect("creating bookmark should succeed");

    write_file(fixture.path().join("tracked.txt"), "line one\nline two\n");
    commit_staged(fixture.path(), "original subject").expect("commit should succeed");

    describe_bookmark_head(
        fixture.path(),
        "describe-me",
        "updated revision description",
    )
    .expect("describing bookmark head should succeed");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load");
    assert_eq!(snapshot.branch_name, "describe-me");
    assert_eq!(
        snapshot.bookmark_revisions[0].subject, "updated revision description",
        "latest revision subject should reflect updated description"
    );
}

#[test]
fn abandoning_bookmark_head_moves_stack_to_previous_revision() {
    let fixture = TempRepo::new("abandon-bookmark-head");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "stack")
        .expect("creating stack bookmark should succeed");

    write_file(fixture.path().join("tracked.txt"), "line one\nline two\n");
    commit_staged(fixture.path(), "stack second commit").expect("second commit should succeed");

    write_file(
        fixture.path().join("tracked.txt"),
        "line one\nline two\nline three\n",
    );
    commit_staged(fixture.path(), "stack third commit").expect("third commit should succeed");

    let before = load_snapshot(fixture.path()).expect("snapshot should load before abandon");
    assert_eq!(before.branch_name, "stack");
    assert_eq!(before.bookmark_revisions[0].subject, "stack third commit");

    abandon_bookmark_head(fixture.path(), "stack")
        .expect("abandoning bookmark head should succeed");

    let after = load_snapshot(fixture.path()).expect("snapshot should load after abandon");
    assert_eq!(after.branch_name, "stack");
    assert_eq!(
        after.bookmark_revisions[0].subject, "stack second commit",
        "bookmark should move to previous revision after abandoning tip"
    );
}

#[test]
fn squashing_bookmark_head_into_parent_keeps_combined_changes() {
    let fixture = TempRepo::new("squash-bookmark-head");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "stack")
        .expect("creating stack bookmark should succeed");

    write_file(fixture.path().join("tracked.txt"), "line one\nline two\n");
    commit_staged(fixture.path(), "stack second commit").expect("second commit should succeed");

    write_file(
        fixture.path().join("tracked.txt"),
        "line one\nline two\nline three\n",
    );
    commit_staged(fixture.path(), "stack third commit").expect("third commit should succeed");

    squash_bookmark_head_into_parent(fixture.path(), "stack")
        .expect("squashing bookmark head should succeed");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load after squash");
    assert_eq!(snapshot.branch_name, "stack");
    assert_eq!(
        snapshot.bookmark_revisions[0].subject, "stack second commit",
        "parent revision should become the bookmark head after squash"
    );

    let stack_show = run_jj_capture(fixture.path(), ["show", "-r", "stack", "--git"]);
    assert!(
        stack_show.contains("+line three"),
        "squashed bookmark head should preserve tip changes in parent revision"
    );
}

#[test]
fn review_url_for_github_remote_uses_compare_link() {
    let fixture = TempRepo::new("review-url-github");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature/review-url")
        .expect("creating bookmark should succeed");

    run_jj(
        fixture.path(),
        [
            "git",
            "remote",
            "add",
            "origin",
            "https://github.com/example-org/hunk.git",
        ],
    );

    let review_url = review_url_for_bookmark(fixture.path(), "feature/review-url")
        .expect("review URL should be computed")
        .expect("github remote should produce review URL");

    assert_eq!(
        review_url, "https://github.com/example-org/hunk/compare/feature%2Freview-url?expand=1",
        "github remotes should use compare links for review URL quick action"
    );
}

#[test]
fn review_url_for_gitlab_remote_uses_merge_request_link() {
    let fixture = TempRepo::new("review-url-gitlab");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature/mr")
        .expect("creating bookmark should succeed");

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

    let review_url = review_url_for_bookmark(fixture.path(), "feature/mr")
        .expect("review URL should be computed")
        .expect("gitlab remote should produce review URL");

    assert_eq!(
        review_url,
        "https://gitlab.com/example-org/hunk/-/merge_requests/new?merge_request[source_branch]=feature%2Fmr",
        "gitlab remotes should use merge-request creation links"
    );
}

#[test]
fn review_url_for_self_hosted_gitlab_uses_provider_mapping() {
    let fixture = TempRepo::new("review-url-self-hosted-gitlab");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature/self-hosted")
        .expect("creating bookmark should succeed");

    run_jj(
        fixture.path(),
        [
            "git",
            "remote",
            "add",
            "origin",
            "https://git.company.internal/example-org/hunk.git",
        ],
    );

    let review_url = review_url_for_bookmark_with_provider_map(
        fixture.path(),
        "feature/self-hosted",
        &[ReviewProviderMapping {
            host: "git.company.internal".to_string(),
            provider: ReviewProviderKind::GitLab,
        }],
    )
    .expect("review URL should be computed")
    .expect("self-hosted GitLab remote should produce review URL with mapping");

    assert_eq!(
        review_url,
        "https://git.company.internal/example-org/hunk/-/merge_requests/new?merge_request[source_branch]=feature%2Fself-hosted"
    );
}

#[test]
fn review_url_for_scp_style_remote_is_normalized() {
    let fixture = TempRepo::new("review-url-scp");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature/scp")
        .expect("creating bookmark should succeed");

    run_jj(
        fixture.path(),
        [
            "git",
            "remote",
            "add",
            "origin",
            "git@github.com:example-org/hunk.git",
        ],
    );

    let review_url = review_url_for_bookmark(fixture.path(), "feature/scp")
        .expect("review URL should be computed")
        .expect("scp-style remote should produce review URL");

    assert_eq!(
        review_url,
        "https://github.com/example-org/hunk/compare/feature%2Fscp?expand=1"
    );
}

#[test]
fn review_url_for_scp_style_remote_without_user_is_normalized() {
    let fixture = TempRepo::new("review-url-scp-no-user");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature/scp-no-user")
        .expect("creating bookmark should succeed");

    run_jj(
        fixture.path(),
        [
            "git",
            "remote",
            "add",
            "origin",
            "github.com:example-org/hunk.git",
        ],
    );

    let review_url = review_url_for_bookmark(fixture.path(), "feature/scp-no-user")
        .expect("review URL should be computed")
        .expect("scp-style remote should produce review URL");

    assert_eq!(
        review_url,
        "https://github.com/example-org/hunk/compare/feature%2Fscp-no-user?expand=1"
    );
}

#[test]
fn reordering_bookmark_tip_swaps_top_two_revisions() {
    let fixture = TempRepo::new("reorder-bookmark-tip");

    write_file(fixture.path().join("tracked-1.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "stack")
        .expect("creating stack bookmark should succeed");

    write_file(fixture.path().join("tracked-2.txt"), "line two\n");
    commit_staged(fixture.path(), "stack second commit").expect("second commit should succeed");

    write_file(fixture.path().join("tracked-3.txt"), "line three\n");
    commit_staged(fixture.path(), "stack third commit").expect("third commit should succeed");

    reorder_bookmark_tip_older(fixture.path(), "stack")
        .expect("reordering top revisions should succeed");

    let snapshot = load_snapshot(fixture.path()).expect("snapshot should load after reorder");
    assert_eq!(snapshot.branch_name, "stack");
    assert!(
        snapshot.bookmark_revisions.len() >= 2,
        "stack should still include reordered revisions"
    );
    assert_eq!(
        snapshot.bookmark_revisions[0].subject, "stack second commit",
        "former parent should become the top revision after reorder"
    );
    assert_eq!(
        snapshot.bookmark_revisions[1].subject, "stack third commit",
        "former tip should be directly below the new top revision"
    );
}

#[test]
fn reordering_two_revision_stack_swaps_tip_and_parent() {
    let fixture = TempRepo::new("reorder-two-revisions");

    write_file(fixture.path().join("tracked-1.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "stack")
        .expect("creating stack bookmark should succeed");

    write_file(fixture.path().join("tracked-2.txt"), "line two\n");
    commit_staged(fixture.path(), "stack second commit").expect("second commit should succeed");

    let before = load_snapshot(fixture.path()).expect("snapshot should load before reorder");
    assert!(before.bookmark_revisions.len() >= 2);
    let before_tip_subject = before.bookmark_revisions[0].subject.clone();
    let before_parent_subject = before.bookmark_revisions[1].subject.clone();

    reorder_bookmark_tip_older(fixture.path(), "stack")
        .expect("reordering two-revision stack should succeed");

    let after = load_snapshot(fixture.path()).expect("snapshot should load after reorder");
    assert_eq!(after.bookmark_revisions[0].subject, before_parent_subject);
    assert_eq!(after.bookmark_revisions[1].subject, before_tip_subject);
}

#[test]
fn reordering_requires_at_least_two_revisions() {
    let fixture = TempRepo::new("reorder-requires-two");

    write_file(fixture.path().join("tracked-1.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "stack")
        .expect("creating stack bookmark should succeed");

    let err = reorder_bookmark_tip_older(fixture.path(), "stack")
        .expect_err("reordering with fewer than two revisions should fail");
    assert!(
        err.to_string().contains("at least two revisions")
            || err.to_string().contains("not active"),
        "error should explain stack size or inactive bookmark constraint"
    );
}

#[test]
fn reordering_requires_bookmark_to_be_active() {
    let fixture = TempRepo::new("reorder-requires-active");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("creating main bookmark should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature")
        .expect("creating feature bookmark should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("switching back to main should succeed");

    write_file(fixture.path().join("tracked-two.txt"), "line two\n");
    commit_staged(fixture.path(), "main second commit").expect("main second commit should succeed");

    let err = reorder_bookmark_tip_older(fixture.path(), "feature")
        .expect_err("reorder should fail for a non-active bookmark");
    assert!(
        err.to_string().contains("not active"),
        "error should explain that bookmark must be active"
    );
}

#[test]
fn reordering_bookmark_tip_matches_cli_insert_after_behavior() {
    let jjlib_fixture = TempRepo::new("reorder-parity-jjlib");
    let cli_fixture = TempRepo::new("reorder-parity-cli");

    seed_three_revision_stack(jjlib_fixture.path(), "stack");
    seed_three_revision_stack(cli_fixture.path(), "stack");

    reorder_bookmark_tip_older(jjlib_fixture.path(), "stack")
        .expect("jj-lib reorder should succeed");
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
    assert_eq!(
        jjlib_subjects, cli_subjects,
        "reorder should match jj rebase -r <tip> -A <anchor> behavior"
    );
}

#[test]
fn review_url_uses_same_fetch_remote_as_jj_remote_list() {
    let fixture = TempRepo::new("review-url-fetch-parity");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature/fetch-url")
        .expect("creating bookmark should succeed");

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

    let cli_fetch_url = remote_url_from_cli_list(fixture.path(), "origin")
        .expect("origin remote should be listed by jj git remote list");
    assert_eq!(cli_fetch_url, "https://gitlab.com/example-org/hunk.git");

    let review_url = review_url_for_bookmark(fixture.path(), "feature/fetch-url")
        .expect("review URL should be computed")
        .expect("fetch remote should produce review URL");
    assert_eq!(
        review_url,
        "https://gitlab.com/example-org/hunk/-/merge_requests/new?merge_request[source_branch]=feature%2Ffetch-url",
        "review URL should be derived from the same fetch URL shown by jj remote list"
    );
}

#[test]
fn describing_requires_bookmark_to_be_active() {
    let fixture = TempRepo::new("describe-requires-active");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("creating main bookmark should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature")
        .expect("creating feature bookmark should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("switching back to main should succeed");

    let err = describe_bookmark_head(fixture.path(), "feature", "new description")
        .expect_err("describe should fail for a non-active bookmark");
    assert!(
        err.to_string().contains("not active"),
        "error should explain that bookmark must be active"
    );
}

#[test]
fn commit_staged_ignores_stale_active_bookmark_preference() {
    let fixture = TempRepo::new("commit-ignores-stale-active-preference");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("creating main bookmark should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature")
        .expect("creating feature bookmark should succeed");

    write_file(
        fixture.path().join("tracked.txt"),
        "line one\nfeature diverged\n",
    );
    commit_staged(fixture.path(), "feature diverge").expect("feature commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "main")
        .expect("switching back to main should succeed");

    fs::write(
        fixture.path().join(".jj").join("hunk-active-bookmark"),
        "feature\n",
    )
    .expect("stale active bookmark preference should be written");

    write_file(
        fixture.path().join("tracked.txt"),
        "line one\nmain followup\n",
    );
    commit_staged(fixture.path(), "main followup").expect("main commit should succeed");

    let main_log = run_jj_capture(
        fixture.path(),
        ["log", "-r", "main", "-n", "1", "--no-graph"],
    );
    assert!(
        main_log.contains("main followup"),
        "main bookmark should advance after committing on checked-out main"
    );

    let feature_log = run_jj_capture(
        fixture.path(),
        ["log", "-r", "feature", "-n", "1", "--no-graph"],
    );
    assert!(
        feature_log.contains("feature diverge"),
        "feature bookmark should remain at its prior tip when preference is stale"
    );
    assert!(
        !feature_log.contains("main followup"),
        "feature bookmark should not be moved by stale active bookmark preference"
    );
}

#[test]
fn review_url_for_ssh_scheme_remote_is_normalized() {
    let fixture = TempRepo::new("review-url-ssh-scheme");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature/ssh")
        .expect("creating bookmark should succeed");

    run_jj(
        fixture.path(),
        [
            "git",
            "remote",
            "add",
            "origin",
            "ssh://git@github.com/example-org/hunk.git",
        ],
    );

    let review_url = review_url_for_bookmark(fixture.path(), "feature/ssh")
        .expect("review URL should be computed")
        .expect("ssh scheme remote should produce review URL");
    assert_eq!(
        review_url,
        "https://github.com/example-org/hunk/compare/feature%2Fssh?expand=1"
    );
}

#[test]
fn review_url_for_path_remote_returns_none() {
    let fixture = TempRepo::new("review-url-path-remote");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature/path")
        .expect("creating bookmark should succeed");

    run_jj(
        fixture.path(),
        ["git", "remote", "add", "origin", "../local-bare-remote"],
    );

    let review_url = review_url_for_bookmark(fixture.path(), "feature/path")
        .expect("review URL computation should succeed");
    assert!(
        review_url.is_none(),
        "path remotes should not produce a review URL"
    );
}

#[test]
fn review_url_strips_credentials_from_https_remote() {
    let fixture = TempRepo::new("review-url-strips-credentials");

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "initial commit").expect("initial commit should succeed");
    checkout_or_create_bookmark(fixture.path(), "feature/creds")
        .expect("creating bookmark should succeed");

    run_jj(
        fixture.path(),
        [
            "git",
            "remote",
            "add",
            "origin",
            "https://user:secret-token@github.com/example-org/hunk.git",
        ],
    );

    let review_url = review_url_for_bookmark(fixture.path(), "feature/creds")
        .expect("review URL should be computed")
        .expect("https remote should produce review URL");
    assert_eq!(
        review_url,
        "https://github.com/example-org/hunk/compare/feature%2Fcreds?expand=1"
    );
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

fn write_file(path: PathBuf, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directories should be created");
    }
    fs::write(path, contents).expect("file should be written");
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

fn current_working_copy_revision(cwd: &Path) -> String {
    run_jj_capture(
        cwd,
        ["log", "-r", "@", "-n", "1", "--no-graph", "-T", "commit_id"],
    )
    .trim()
    .to_string()
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
