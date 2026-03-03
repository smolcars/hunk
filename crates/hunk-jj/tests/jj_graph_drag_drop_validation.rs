use hunk_jj::jj::{
    GraphBookmarkRef, GraphBookmarkScope, GraphNode, graph_bookmark_drop_validation,
};

fn local_bookmark(name: &str) -> GraphBookmarkRef {
    GraphBookmarkRef {
        name: name.to_string(),
        remote: None,
        scope: GraphBookmarkScope::Local,
        is_active: false,
        tracked: false,
        needs_push: false,
        conflicted: false,
    }
}

fn remote_bookmark(name: &str, remote: &str) -> GraphBookmarkRef {
    GraphBookmarkRef {
        name: name.to_string(),
        remote: Some(remote.to_string()),
        scope: GraphBookmarkScope::Remote,
        is_active: false,
        tracked: true,
        needs_push: false,
        conflicted: false,
    }
}

fn node(id: &str, unix_time: i64, bookmarks: Vec<GraphBookmarkRef>) -> GraphNode {
    GraphNode {
        id: id.to_string(),
        subject: format!("subject-{id}"),
        unix_time,
        bookmarks,
        is_working_copy_parent: false,
        is_active_bookmark_target: false,
    }
}

#[test]
fn drag_drop_validation_accepts_local_bookmark_drop_to_different_node() {
    let nodes = vec![
        node("tip", 100, vec![local_bookmark("feature")]),
        node("target", 90, Vec::new()),
    ];

    let result = graph_bookmark_drop_validation(
        &nodes,
        "feature",
        None,
        GraphBookmarkScope::Local,
        "target",
    );
    assert!(result.is_ok());
}

#[test]
fn drag_drop_validation_rejects_remote_bookmark_drop() {
    let nodes = vec![
        node("tip", 100, vec![remote_bookmark("feature", "origin")]),
        node("target", 90, Vec::new()),
    ];

    let err = graph_bookmark_drop_validation(
        &nodes,
        "feature",
        Some("origin"),
        GraphBookmarkScope::Remote,
        "target",
    )
    .expect_err("remote bookmark drag-drop should be rejected");
    assert!(
        err.contains("Only local bookmarks"),
        "error should explain local-only drag constraint"
    );
}

#[test]
fn drag_drop_validation_rejects_same_target_node() {
    let nodes = vec![node("tip", 100, vec![local_bookmark("feature")])];

    let err =
        graph_bookmark_drop_validation(&nodes, "feature", None, GraphBookmarkScope::Local, "tip")
            .expect_err("dropping onto the same target should be rejected");
    assert!(
        err.contains("already targeting"),
        "error should explain same-target rejection"
    );
}

#[test]
fn drag_drop_validation_rejects_target_not_in_graph_window() {
    let nodes = vec![node("tip", 100, vec![local_bookmark("feature")])];

    let err = graph_bookmark_drop_validation(
        &nodes,
        "feature",
        None,
        GraphBookmarkScope::Local,
        "missing",
    )
    .expect_err("dropping outside graph window should be rejected");
    assert!(
        err.contains("current graph window"),
        "error should explain graph-window target requirement"
    );
}
