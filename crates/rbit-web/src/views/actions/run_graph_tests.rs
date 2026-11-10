use rbit_core::actions::Status;

use super::*;
use crate::views::actions::run_layout::tests::{job, matrix};

#[test]
fn columns_follow_needs_depth_and_lines_join_needs() {
    let jobs = [
        job(1, "lint", &[], Status::Success),
        job(2, "build", &[], Status::Success),
        matrix(3, "test", "test (linux)", &["build"], Status::Running),
        matrix(4, "test", "test (windows)", &["build"], Status::Failure),
        job(5, "deploy", &["test", "lint"], Status::Blocked),
    ];
    let graph = JobGraph::new(&jobs, 7, &ActionsUrls::new("/a/b")).unwrap();
    let titles: Vec<Vec<&str>> = graph.columns.iter().map(|column| column.nodes.iter().map(|node| node.title.as_str()).collect()).collect();
    assert_eq!(titles, [vec!["lint", "build"], vec!["test"], vec!["deploy"]]);
    let test = &graph.columns[1].nodes[0];
    assert!(test.is_matrix() && test.count == 2 && test.jobs.len() == 2 && test.hidden == 0);
    assert_eq!(test.look.key, "failure");
    assert_eq!(test.jobs[0].url, "/a/b/actions/runs/7/jobs/3");
    assert_eq!(graph.columns[0].nodes[0].url.as_deref(), Some("/a/b/actions/runs/7/jobs/1"));
    // build → test, lint → deploy (across a column), test → deploy.
    assert_eq!(graph.edges.len(), 3);
    assert_eq!(graph.width, 3.0 * COLUMN + 2.0 * COLUMN_GAP);
    // The tallest column is the matrix box: a heading and two rows inside a border.
    assert_eq!(graph.height, 2.0 * BORDER + HEADING + 2.0 * ROW);
    // build sits second in the first column; its line leaves from the middle of its heading.
    let build = &graph.columns[0].nodes[1];
    let first = (graph.height - (2.0 * (2.0 * BORDER + HEADING) + BOX_GAP)) / 2.0;
    assert_eq!(build.top, first + 2.0 * BORDER + HEADING + BOX_GAP);
    let from = format!("M{} {}", COLUMN, build.top + BORDER + HEADING / 2.0);
    assert!(graph.edges.iter().any(|edge| edge.starts_with(&from)), "{:?}", graph.edges);
}

#[test]
fn big_matrices_list_some_jobs_and_count_the_rest() {
    let jobs: Vec<_> = (0..10).map(|index| matrix(index, "test", &format!("test {index}"), &[], Status::Success)).collect();
    let graph = JobGraph::new(&jobs, 1, &ActionsUrls::new("/a/b")).unwrap();
    let node = &graph.columns[0].nodes[0];
    assert_eq!((node.jobs.len(), node.hidden, node.count), (6, 4, 10));
    assert_eq!(graph.height, 2.0 * BORDER + HEADING + ROW * 7.0, "a row says how many more");
    assert_eq!(graph.edges, Vec::<String>::new());
}

#[test]
fn no_jobs_no_graph() {
    assert!(JobGraph::new(&[], 1, &ActionsUrls::new("/a/b")).is_none());
}

#[test]
fn right_to_left_lines_start_on_the_right() {
    let jobs = [job(1, "build", &[], Status::Success), job(2, "test", &["build"], Status::Running)];
    let graph = JobGraph::new(&jobs, 1, &ActionsUrls::new("/a/b")).unwrap();
    assert_eq!(graph.edges, ["M208 17 C232 17 232 17 256 17"]);
    assert_eq!(graph.mirrored().edges, ["M256 17 C232 17 232 17 208 17"]);
}
