use hunk_jj::jj::{is_valid_bookmark_name, sanitize_bookmark_name};

#[test]
fn sanitizes_space_separated_branch_name() {
    let branch = sanitize_bookmark_name("some random branch");
    assert_eq!(branch, "some-random-branch");
    assert!(is_valid_bookmark_name(&branch));
}

#[test]
fn sanitizes_branch_with_special_characters() {
    let branch = sanitize_bookmark_name("Feature: Add [WIP]?");
    assert_eq!(branch, "feature-add-wip");
    assert!(is_valid_bookmark_name(&branch));
}

#[test]
fn preserves_path_like_branch_shape() {
    let branch = sanitize_bookmark_name("feature/my cool branch");
    assert_eq!(branch, "feature/my-cool-branch");
    assert!(is_valid_bookmark_name(&branch));
}

#[test]
fn falls_back_for_empty_name() {
    let branch = sanitize_bookmark_name("   ");
    assert_eq!(branch, "bookmark");
    assert!(is_valid_bookmark_name(&branch));
}

#[test]
fn avoids_reserved_head_name() {
    let branch = sanitize_bookmark_name("HEAD");
    assert_eq!(branch, "head-bookmark");
    assert!(is_valid_bookmark_name(&branch));
}

#[test]
fn avoids_reserved_internal_bookmark_names() {
    let detached = sanitize_bookmark_name("detached");
    assert_eq!(detached, "detached-bookmark");
    assert!(is_valid_bookmark_name(&detached));

    let unknown = sanitize_bookmark_name("UNKNOWN");
    assert_eq!(unknown, "unknown-bookmark");
    assert!(is_valid_bookmark_name(&unknown));
}

#[test]
fn rejects_invalid_bookmark_names() {
    let invalid = [
        "",
        "feature//two",
        "feature..two",
        "feature.lock",
        "feature@{1}",
        "/leading",
        "trailing/",
        ".leadingdot",
        "trailingdot.",
        "white space",
        "feature/@/x",
        "feature:colon",
        "detached",
        "unknown",
        "DETACHED",
    ];

    for name in invalid {
        assert!(
            !is_valid_bookmark_name(name),
            "expected invalid bookmark name: {name}"
        );
    }
}

#[test]
fn accepts_valid_bookmark_names() {
    let valid = [
        "bookmark",
        "feature/new-ui",
        "release/v1.2.3",
        "hotfix_bug-123",
        "abc/def_ghi.jkl",
    ];

    for name in valid {
        assert!(
            is_valid_bookmark_name(name),
            "expected valid bookmark name: {name}"
        );
    }
}
