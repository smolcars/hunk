use hunk_jj::jj::{GraphBookmarkRef, GraphBookmarkScope, GraphEdge, GraphNode};
use hunk_jj::jj_graph_tree::build_graph_lane_rows;

fn node(id: &str, unix_time: i64) -> GraphNode {
    GraphNode {
        id: id.to_string(),
        subject: id.to_string(),
        unix_time,
        bookmarks: Vec::<GraphBookmarkRef>::new(),
        is_working_copy_parent: false,
        is_active_bookmark_target: false,
    }
}

#[test]
fn tree_layout_linear_history_uses_single_lane() {
    let nodes = vec![node("c3", 30), node("c2", 20), node("c1", 10)];
    let edges = vec![
        GraphEdge {
            from: "c3".to_string(),
            to: "c2".to_string(),
        },
        GraphEdge {
            from: "c2".to_string(),
            to: "c1".to_string(),
        },
    ];

    let rows = build_graph_lane_rows(&nodes, &edges);
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row.lane_count == 1));
    assert!(rows.iter().all(|row| row.node_lane == 0));
    assert!(
        rows.iter()
            .all(|row| row.horizontal.iter().all(|value| !*value))
    );
    assert_eq!(rows[0].top_vertical, vec![false]);
    assert_eq!(rows[0].bottom_vertical, vec![true]);
    assert_eq!(rows[1].top_vertical, vec![true]);
    assert_eq!(rows[1].bottom_vertical, vec![true]);
    assert_eq!(rows[2].top_vertical, vec![true]);
    assert_eq!(rows[2].bottom_vertical, vec![false]);
}

#[test]
fn tree_layout_branch_and_merge_allocates_secondary_lane() {
    let nodes = vec![
        node("merge", 50),
        node("left", 40),
        node("right", 30),
        node("base", 20),
        node("root", 10),
    ];
    let edges = vec![
        GraphEdge {
            from: "merge".to_string(),
            to: "left".to_string(),
        },
        GraphEdge {
            from: "merge".to_string(),
            to: "right".to_string(),
        },
        GraphEdge {
            from: "left".to_string(),
            to: "base".to_string(),
        },
        GraphEdge {
            from: "right".to_string(),
            to: "base".to_string(),
        },
        GraphEdge {
            from: "base".to_string(),
            to: "root".to_string(),
        },
    ];

    let rows = build_graph_lane_rows(&nodes, &edges);
    let merge_row = rows
        .iter()
        .find(|row| row.node_id == "merge")
        .expect("merge row should exist");
    assert_eq!(merge_row.node_lane, 0);
    assert_eq!(merge_row.secondary_parent_lanes, vec![1]);
    assert_eq!(merge_row.lane_count, 2);
    assert!(merge_row.horizontal[0]);
    assert!(merge_row.horizontal[1]);

    let right_row = rows
        .iter()
        .find(|row| row.node_id == "right")
        .expect("right row should exist");
    assert_eq!(right_row.node_lane, 1);
    assert!(rows.iter().any(|row| row.lane_count >= 2));
}

#[test]
fn tree_layout_ignores_parent_edges_outside_window() {
    let nodes = vec![node("tip", 10)];
    let edges = vec![GraphEdge {
        from: "tip".to_string(),
        to: "outside".to_string(),
    }];

    let rows = build_graph_lane_rows(&nodes, &edges);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].lane_count, 1);
    assert_eq!(rows[0].node_lane, 0);
    assert_eq!(rows[0].secondary_parent_lanes, Vec::<usize>::new());
    assert_eq!(rows[0].top_vertical, vec![false]);
    assert_eq!(rows[0].bottom_vertical, vec![false]);
}

#[test]
fn tree_layout_remote_bookmarks_do_not_change_lane_math() {
    let mut nodes = vec![node("tip", 20), node("base", 10)];
    nodes[0].bookmarks.push(GraphBookmarkRef {
        name: "main".to_string(),
        remote: Some("origin".to_string()),
        scope: GraphBookmarkScope::Remote,
        is_active: false,
        tracked: true,
        needs_push: false,
        conflicted: false,
    });
    let edges = vec![GraphEdge {
        from: "tip".to_string(),
        to: "base".to_string(),
    }];

    let rows = build_graph_lane_rows(&nodes, &edges);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.lane_count == 1));
}
