//! A bounded commit graph, preserving merge parents and distinct branch lanes.

use crate::{urls::RepoUrls, views::repo::CommitView};
use rbit_git::{CommitSummary, RefSummary};
use std::{collections::HashMap, fmt::Write};

type BranchLinks = Vec<(String, String)>;

#[derive(Debug)]
pub struct GraphEdge {
    pub x1: usize,
    pub x2: usize,
}

#[derive(Debug)]
pub struct GraphRow {
    pub commit: CommitView,
    pub branches: Vec<(String, String)>,
    pub more_branches: usize,
    pub paths: String,
    pub x: usize,
}

#[derive(Debug, Default)]
pub struct BranchGraph {
    pub rows: Vec<GraphRow>,
    pub width: usize,
    pub truncated: bool,
}

impl BranchGraph {
    pub fn new(commits: &[CommitSummary], refs: &[RefSummary], urls: &RepoUrls, truncated: bool) -> Self {
        let mut labels: HashMap<&str, (BranchLinks, usize)> = HashMap::new();
        for reference in refs {
            let (visible, extra) = labels.entry(&reference.target).or_default();
            if visible.len() < 8 {
                visible.push((reference.name.clone(), urls.tree(&format!("refs/heads/{}", reference.name), "")));
            } else {
                *extra += 1;
            }
        }
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
            let mut paths = String::new();
            for column in 0..lanes.len() {
                let _ = write!(paths, "M{} 0V24", x(column));
            }
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
            let (branches, more_branches) = labels.remove(commit.id.as_str()).unwrap_or_default();
            rows.push(GraphRow {
                commit: CommitView::new(commit, urls),
                branches,
                more_branches,
                paths: {
                    for edge in edges {
                        let _ = write!(paths, "M{} 24C{} 36 {} 36 {} 48", edge.x1, edge.x1, edge.x2, edge.x2);
                    }
                    paths
                },
                x: x(column),
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
        assert_eq!(graph.rows[0].paths, "M16 24C16 36 16 36 16 48M16 24C16 36 36 36 36 48");
        assert_eq!(graph.rows[3].paths, "M16 0V24");
        assert!(graph.width >= 52);
    }

    #[test]
    fn branch_labels_are_bounded_without_losing_the_count() {
        let refs = (0..1020)
            .map(|i| RefSummary { name: format!("branch-{i}"), kind: rbit_git::RefKind::Branch, target: "root".into() })
            .collect::<Vec<_>>();
        let graph = BranchGraph::new(&[commit("root", &[])], &refs, &RepoUrls::new("o", "r"), false);
        assert_eq!(graph.rows[0].branches.len(), 8);
        assert_eq!(graph.rows[0].more_branches, 1012);
    }
}
