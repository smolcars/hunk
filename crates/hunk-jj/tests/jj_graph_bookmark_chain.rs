use hunk_jj::jj::{
    GraphBookmarkRef, GraphBookmarkScope, GraphEdge, GraphNode, graph_bookmark_revision_chain,
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
fn bookmark_chain_includes_tip_and_reachable_ancestors() {
    let nodes = vec![
        node("a", 5, vec![local_bookmark("main")]),
        node("b", 4, Vec::new()),
        node("c", 3, Vec::new()),
        node("d", 2, vec![local_bookmark("feature")]),
    ];
    let edges = vec![
        GraphEdge {
            from: "a".to_string(),
            to: "b".to_string(),
        },
        GraphEdge {
            from: "b".to_string(),
            to: "c".to_string(),
        },
        GraphEdge {
            from: "d".to_string(),
            to: "c".to_string(),
        },
    ];

    let chain =
        graph_bookmark_revision_chain(&nodes, &edges, "main", None, GraphBookmarkScope::Local);
    assert_eq!(chain, vec!["a", "b", "c"]);
}

#[test]
fn bookmark_chain_respects_scope_and_remote_identity() {
    let nodes = vec![
        node("local-tip", 10, vec![local_bookmark("main")]),
        node("remote-tip", 12, vec![remote_bookmark("main", "origin")]),
        node("shared-parent", 9, Vec::new()),
    ];
    let edges = vec![
        GraphEdge {
            from: "remote-tip".to_string(),
            to: "shared-parent".to_string(),
        },
        GraphEdge {
            from: "local-tip".to_string(),
            to: "shared-parent".to_string(),
        },
    ];

    let remote_chain = graph_bookmark_revision_chain(
        &nodes,
        &edges,
        "main",
        Some("origin"),
        GraphBookmarkScope::Remote,
    );
    assert_eq!(remote_chain, vec!["remote-tip", "shared-parent"]);

    let local_chain =
        graph_bookmark_revision_chain(&nodes, &edges, "main", None, GraphBookmarkScope::Local);
    assert_eq!(local_chain, vec!["local-tip", "shared-parent"]);
}

#[test]
fn bookmark_chain_returns_empty_for_unknown_bookmark() {
    let nodes = vec![node("a", 5, vec![local_bookmark("main")])];
    let edges = Vec::new();

    let chain = graph_bookmark_revision_chain(
        &nodes,
        &edges,
        "does-not-exist",
        None,
        GraphBookmarkScope::Local,
    );
    assert!(chain.is_empty());
}

#[test]
fn bookmark_chain_includes_all_reachable_parents_for_merges() {
    let nodes = vec![
        node("tip", 15, vec![local_bookmark("merge-bookmark")]),
        node("left", 14, Vec::new()),
        node("right", 13, Vec::new()),
        node("root", 12, Vec::new()),
    ];
    let edges = vec![
        GraphEdge {
            from: "tip".to_string(),
            to: "left".to_string(),
        },
        GraphEdge {
            from: "tip".to_string(),
            to: "right".to_string(),
        },
        GraphEdge {
            from: "left".to_string(),
            to: "root".to_string(),
        },
        GraphEdge {
            from: "right".to_string(),
            to: "root".to_string(),
        },
    ];

    let chain = graph_bookmark_revision_chain(
        &nodes,
        &edges,
        "merge-bookmark",
        None,
        GraphBookmarkScope::Local,
    );
    assert_eq!(chain, vec!["tip", "left", "right", "root"]);
}
