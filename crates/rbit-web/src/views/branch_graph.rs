//! A bounded commit graph, preserving merge parents and distinct branch lanes.

use crate::{
    urls::RepoUrls,
    views::{refs::RefRow, repo::CommitView},
};
use rbit_git::CommitSummary;
use std::collections::HashMap;

#[derive(Debug)]
pub struct GraphEdge {
    pub x1: usize,
    pub x2: usize,
}

#[derive(Debug)]
pub struct GraphRow {
    pub commit: CommitView,
    pub branches: Vec<(String, String)>,
    pub edges: Vec<GraphEdge>,
    pub x: usize,
    pub incoming: Vec<usize>,
}

#[derive(Debug, Default)]
pub struct BranchGraph {
    pub rows: Vec<GraphRow>,
    pub width: usize,
    pub truncated: bool,
}

impl BranchGraph {
    pub fn new(commits: &[CommitSummary], refs: &[RefRow], urls: &RepoUrls, truncated: bool) -> Self {
        // Date ordering alone can put parents before their children when clocks
        // differ. Topologically order the selected window, retaining date priority.
        let indices: HashMap<_, _> = commits.iter().enumerate().map(|(i, c)| (c.id.as_str(), i)).collect();
        let mut children = vec![0; commits.len()];
        for commit in commits {
            for parent in &commit.parent_ids {
                if let Some(&i) = indices.get(parent.as_str()) {
                    children[i] += 1;
                }
            }
        }
        let mut order = Vec::new();
        let mut emitted = vec![false; commits.len()];
        while let Some(i) = (0..commits.len()).find(|&i| !emitted[i] && children[i] == 0) {
            emitted[i] = true;
            order.push(i);
            for parent in &commits[i].parent_ids {
                if let Some(&index) = indices.get(parent.as_str()) {
                    children[index] -= 1;
                }
            }
        }
        let mut lanes: Vec<String> = Vec::new();
        let mut rows = Vec::new();
        let mut max_lanes = 1;
        for i in order {
            let commit = &commits[i];
            let incoming = (0..lanes.len()).map(x).collect();
            let column = lanes.iter().position(|id| id == &commit.id).unwrap_or_else(|| {
                lanes.push(commit.id.clone());
                lanes.len() - 1
            });
            let before = lanes.clone();
            lanes.remove(column);
            for (offset, parent) in commit.parent_ids.iter().enumerate() {
                if !lanes.contains(parent) {
                    lanes.insert((column + offset).min(lanes.len()), parent.clone());
                }
            }
            max_lanes = max_lanes.max(before.len()).max(lanes.len());
            let mut edges = Vec::new();
            for (source, id) in before.iter().enumerate() {
                if source == column {
                    for parent in &commit.parent_ids {
                        if let Some(destination) = lanes.iter().position(|id| id == parent) {
                            edges.push(GraphEdge { x1: x(source), x2: x(destination) });
                        }
                    }
                } else if let Some(destination) = lanes.iter().position(|next| next == id) {
                    edges.push(GraphEdge { x1: x(source), x2: x(destination) });
                }
            }
            rows.push(GraphRow {
                commit: CommitView::new(commit, urls),
                branches: refs.iter().filter(|row| row.commit.id == commit.id).map(|row| (row.name.clone(), row.url.clone())).collect(),
                edges,
                x: x(column),
                incoming,
            });
        }
        Self { rows, width: max_lanes * 20 + 12, truncated }
    }
}

fn x(column: usize) -> usize {
    column * 20 + 16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(id: &str, parents: &[&str]) -> CommitSummary {
        CommitSummary {
            id: id.into(),
            short_id: id.into(),
            summary: id.into(),
            body: None,
            author_name: "Alice".into(),
            author_email: "alice@example.com".into(),
            authored_at: rbit_core::Timestamp(1_700_000_000),
            parent_ids: parents.iter().map(|id| (*id).into()).collect(),
        }
    }

    #[test]
    fn graph_preserves_merge_edges_and_orders_skewed_dates() {
        let commits = vec![commit("root", &[]), commit("merge", &["left", "right"]), commit("right", &["root"]), commit("left", &["root"])];
        let graph = BranchGraph::new(&commits, &[], &RepoUrls::new("acme", "rocket"), false);
        assert_eq!(graph.rows.iter().map(|row| row.commit.id.as_str()).collect::<Vec<_>>(), ["merge", "right", "left", "root"]);
        assert_eq!(graph.rows[0].edges.len(), 2);
        assert_eq!(graph.rows[0].incoming, Vec::<usize>::new());
        assert_eq!(graph.rows[3].incoming.len(), 1);
        assert!(graph.rows[3].edges.is_empty());
        assert!(graph.width >= 52);
    }
}
