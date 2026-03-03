use std::collections::{BTreeMap, BTreeSet};

use crate::jj::{GraphEdge, GraphNode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphLaneRow {
    pub node_id: String,
    pub node_lane: usize,
    pub lane_count: usize,
    pub top_vertical: Vec<bool>,
    pub bottom_vertical: Vec<bool>,
    pub horizontal: Vec<bool>,
    pub secondary_parent_lanes: Vec<usize>,
}

pub fn build_graph_lane_rows(nodes: &[GraphNode], edges: &[GraphEdge]) -> Vec<GraphLaneRow> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let node_ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    let row_index_by_id = nodes
        .iter()
        .enumerate()
        .map(|(ix, node)| (node.id.clone(), ix))
        .collect::<BTreeMap<_, _>>();

    let mut parent_ids_by_node = BTreeMap::<String, Vec<String>>::new();
    for edge in edges {
        if !node_ids.contains(edge.from.as_str()) || !node_ids.contains(edge.to.as_str()) {
            continue;
        }
        parent_ids_by_node
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
    }
    for parent_ids in parent_ids_by_node.values_mut() {
        parent_ids.sort_by(|left, right| {
            row_index_by_id
                .get(left)
                .copied()
                .unwrap_or(usize::MAX)
                .cmp(&row_index_by_id.get(right).copied().unwrap_or(usize::MAX))
                .then_with(|| left.cmp(right))
        });
        parent_ids.dedup();
    }

    let mut lanes: Vec<Option<String>> = Vec::new();
    let mut rows = Vec::with_capacity(nodes.len());

    for node in nodes {
        let node_lane = lane_index_for_node(&mut lanes, node.id.as_str());
        let parent_ids = parent_ids_by_node
            .get(node.id.as_str())
            .cloned()
            .unwrap_or_default();

        let mut lanes_after = lanes.clone();
        let mut secondary_parent_lanes = Vec::new();
        if parent_ids.is_empty() {
            lanes_after[node_lane] = None;
        } else {
            lanes_after[node_lane] = Some(parent_ids[0].clone());
            for parent_id in parent_ids.iter().skip(1) {
                let lane_ix = lanes_after
                    .iter()
                    .position(|candidate| candidate.as_deref() == Some(parent_id.as_str()))
                    .unwrap_or_else(|| first_empty_lane_index(&mut lanes_after));
                lanes_after[lane_ix] = Some(parent_id.clone());
                if lane_ix != node_lane {
                    secondary_parent_lanes.push(lane_ix);
                }
            }
        }

        while lanes_after.last().is_some_and(|slot| slot.is_none()) {
            lanes_after.pop();
        }
        secondary_parent_lanes.sort_unstable();
        secondary_parent_lanes.dedup();

        let lane_count = lanes
            .len()
            .max(lanes_after.len())
            .max(node_lane.saturating_add(1))
            .max(1);
        let mut top_vertical = vec![false; lane_count];
        let mut bottom_vertical = vec![false; lane_count];
        for (ix, lane_node) in lanes.iter().enumerate() {
            top_vertical[ix] = lane_node.is_some();
        }
        for (ix, lane_node) in lanes_after.iter().enumerate() {
            bottom_vertical[ix] = lane_node.is_some();
        }

        let mut horizontal = vec![false; lane_count];
        for secondary_lane in &secondary_parent_lanes {
            let start = (*secondary_lane).min(node_lane);
            let end = (*secondary_lane).max(node_lane);
            for cell in horizontal.iter_mut().take(end + 1).skip(start) {
                *cell = true;
            }
        }

        rows.push(GraphLaneRow {
            node_id: node.id.clone(),
            node_lane,
            lane_count,
            top_vertical,
            bottom_vertical,
            horizontal,
            secondary_parent_lanes,
        });

        lanes = lanes_after;
    }

    rows
}

fn lane_index_for_node(lanes: &mut Vec<Option<String>>, node_id: &str) -> usize {
    if let Some(ix) = lanes
        .iter()
        .position(|lane_node| lane_node.as_deref() == Some(node_id))
    {
        return ix;
    }
    if let Some(ix) = lanes.iter().position(Option::is_none) {
        return ix;
    }
    lanes.push(None);
    lanes.len().saturating_sub(1)
}

fn first_empty_lane_index(lanes: &mut Vec<Option<String>>) -> usize {
    if let Some(ix) = lanes.iter().position(Option::is_none) {
        ix
    } else {
        lanes.push(None);
        lanes.len().saturating_sub(1)
    }
}
