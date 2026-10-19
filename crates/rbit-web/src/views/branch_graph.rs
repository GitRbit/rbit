//! A bounded commit graph, preserving merge parents and distinct branch lanes.

use crate::{urls::RepoUrls, views::repo::CommitView};
use rbit_git::{CommitSummary, RefSummary};
use std::collections::HashMap;

type BranchLinks = Vec<(String, String)>;

#[derive(Debug)]
pub struct GraphEdge {
    pub path: String,
    pub lane: usize,
    pub color: usize,
}

#[derive(Debug)]
pub struct GraphRow {
    pub commit: CommitView,
    pub branches: Vec<(String, String)>,
    pub more_branches: usize,
    pub edges: Vec<GraphEdge>,
    pub x: usize,
    pub lane: usize,
    pub color: usize,
    pub is_merge: bool,
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
        // A track keeps its identity as columns compact. The first parent keeps
        // its child's color; additional merge parents get separate tracks.
        let mut lanes: Vec<(String, usize)> = Vec::new();
        let mut next_lane = 0;
        let mut rows = Vec::new();
        let mut max_lanes = 1;
        for i in order {
            let commit = &commits[i];
            let mut edges = Vec::new();
            for (column, (_, lane)) in lanes.iter().enumerate() {
                edges.push(GraphEdge { path: format!("M{} 0V36", x(column)), lane: *lane, color: lane % 8 });
            }
            let column = lanes.iter().position(|(id, _)| id == &commit.id).unwrap_or_else(|| {
                lanes.push((commit.id.clone(), next_lane));
                next_lane += 1;
                lanes.len() - 1
            });
            let lane = lanes[column].1;
            let before = lanes.clone();
            lanes.remove(column);
            for (offset, parent) in commit.parent_ids.iter().enumerate() {
                if !lanes.iter().any(|(id, _)| id == parent) {
                    let parent_lane = if offset == 0 {
                        lane
                    } else {
                        let id = next_lane;
                        next_lane += 1;
                        id
                    };
                    lanes.insert((column + offset).min(lanes.len()), (parent.clone(), parent_lane));
                }
            }
            max_lanes = max_lanes.max(before.len()).max(lanes.len());
            for (source, (id, source_lane)) in before.iter().enumerate() {
                if source == column {
                    for parent in &commit.parent_ids {
                        if let Some(destination) = lanes.iter().position(|(id, _)| id == parent) {
                            let parent_lane = lanes[destination].1;
                            edges.push(edge(source, destination, parent_lane));
                        }
                    }
                } else if let Some(destination) = lanes.iter().position(|(next, _)| next == id) {
                    edges.push(edge(source, destination, *source_lane));
                }
            }
            let (branches, more_branches) = labels.remove(commit.id.as_str()).unwrap_or_default();
            rows.push(GraphRow {
                commit: CommitView::new(commit, urls),
                branches,
                more_branches,
                edges,
                x: x(column),
                lane,
                color: lane % 8,
                is_merge: commit.parent_ids.len() > 1,
            });
        }
        Self { rows, width: max_lanes * 20 + 12, truncated }
    }
}

fn edge(source: usize, destination: usize, lane: usize) -> GraphEdge {
    GraphEdge { path: format!("M{} 36C{} 54 {} 54 {} 72", x(source), x(source), x(destination), x(destination)), lane, color: lane % 8 }
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
        assert_eq!(
            graph.rows[0].edges.iter().map(|edge| edge.path.as_str()).collect::<Vec<_>>(),
            ["M16 36C16 54 16 54 16 72", "M16 36C16 54 36 54 36 72"]
        );
        assert_eq!(graph.rows[3].edges[0].path, "M16 0V36");
        assert!(graph.rows[0].is_merge);
        assert!(graph.width >= 52);
    }

    #[test]
    fn track_colors_follow_first_parents_and_survive_column_compaction() {
        let commits = vec![commit("merge", &["left", "right"]), commit("left", &["root"]), commit("right", &["root"]), commit("root", &[])];
        let graph = BranchGraph::new(&commits, &[], &RepoUrls::new("o", "r"), false);
        let [merge, left, right, root] = graph.rows.as_slice() else { panic!("expected four rows") };
        assert_eq!(merge.lane, left.lane);
        assert_eq!(left.lane, root.lane);
        assert_ne!(left.color, right.color);
        assert_eq!(right.edges[0].lane, root.lane);
        assert!(right.edges.iter().any(|edge| edge.path == "M36 36C36 54 16 54 16 72" && edge.lane == root.lane));
        assert_eq!(root.x, 16);
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
