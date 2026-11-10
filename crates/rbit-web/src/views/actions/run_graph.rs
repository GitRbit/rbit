//! The run's job graph, as GitHub draws it: a column per `needs` depth, a box per workflow
//! job (a matrix lists its jobs in its box), and lines from each job to the jobs that need
//! it. The boxes are HTML; the lines are one SVG behind them, laid out here.
//!
//! The template sizes everything in rem from these numbers (pixels at a 16 px rem): a column
//! is `w-52` with `gap-12` between columns, boxes `gap-4` apart in a column and centred in
//! it; a box is a 1 px border around an `h-8` heading and `h-7` rows. The SVG's viewBox
//! is in the same pixels and stretched over the boxes, so the lines meet them at any size.
//! Right to left, the columns follow the page's direction and [`JobGraph::mirrored`]
//! draws the lines from the right.
use std::collections::HashMap;

use rbit_core::actions::Job;

use super::{
    ActionsUrls, StateLook,
    run_layout::{combined_status, job_clock, needs_depths},
};
use crate::views::when::Took;

const COLUMN: f64 = 208.0;
const COLUMN_GAP: f64 = 48.0;
const BOX_GAP: f64 = 16.0;
const HEADING: f64 = 32.0;
const ROW: f64 = 28.0;
const BORDER: f64 = 1.0;
/// Matrix jobs listed in a box; the rest are counted.
pub const MATRIX_ROWS: usize = 6;

/// A job in a matrix box.
#[derive(Debug, Clone)]
pub struct GraphJob {
    pub name: String,
    pub url: String,
    pub look: StateLook,
    pub duration: Option<Took>,
    pub since: Option<i64>,
}

/// One workflow job: a single job, or a matrix with its jobs.
#[derive(Debug, Clone)]
pub struct GraphNode {
    /// The job's name, or a matrix's key.
    pub title: String,
    /// The job, for a single job.
    pub url: Option<String>,
    pub look: StateLook,
    pub duration: Option<Took>,
    pub since: Option<i64>,
    /// A matrix's jobs, at most [`MATRIX_ROWS`].
    pub jobs: Vec<GraphJob>,
    /// How many jobs the matrix made.
    pub count: u64,
    /// How many of them are not listed.
    pub hidden: u64,
    top: f64,
}

impl GraphNode {
    pub fn is_matrix(&self) -> bool {
        self.url.is_none()
    }

    fn height(&self) -> f64 {
        let rows = self.jobs.len() + usize::from(self.hidden > 0);
        if self.is_matrix() { 2.0 * BORDER + HEADING + ROW * float(rows) } else { 2.0 * BORDER + HEADING }
    }

    /// Where lines meet the box: the middle of its heading.
    fn anchor(&self) -> f64 {
        self.top + BORDER + HEADING / 2.0
    }
}

/// The jobs of one `needs` depth.
#[derive(Debug, Clone)]
pub struct GraphColumn {
    pub nodes: Vec<GraphNode>,
}

/// The job graph of a run.
#[derive(Debug, Clone)]
pub struct JobGraph {
    pub columns: Vec<GraphColumn>,
    /// SVG path data of the lines, in the viewBox's pixels.
    pub edges: Vec<String>,
    /// The lines' ends, `(x1, y1, x2, y2)`, from left to right.
    lines: Vec<(f64, f64, f64, f64)>,
    pub width: f64,
    pub height: f64,
}

impl JobGraph {
    /// The graph of run `number`'s `jobs`; `None` without jobs.
    pub fn new(jobs: &[Job], number: i64, urls: &ActionsUrls) -> Option<Self> {
        if jobs.is_empty() {
            return None;
        }
        let depths = needs_depths(jobs);
        // Workflow jobs by key, in the workflow's order.
        let mut keys: Vec<(&str, Vec<&Job>)> = Vec::new();
        for job in jobs {
            match keys.iter_mut().find(|(key, _)| *key == job.key) {
                Some((_, members)) => members.push(job),
                None => keys.push((job.key.as_str(), vec![job])),
            }
        }
        let width = depths.values().copied().max().unwrap_or(0) + 1;
        let mut columns: Vec<GraphColumn> = (0..width).map(|_| GraphColumn { nodes: Vec::new() }).collect();
        let mut place: HashMap<&str, (usize, usize)> = HashMap::new();
        for (key, members) in &keys {
            let depth = depths.get(key).copied().unwrap_or(0).min(width - 1);
            place.insert(key, (depth, columns[depth].nodes.len()));
            columns[depth].nodes.push(node(key, members, number, urls));
        }
        let height = columns.iter().map(column_height).fold(0.0, f64::max);
        for column in &mut columns {
            let mut top = (height - column_height(column)) / 2.0;
            for node in &mut column.nodes {
                node.top = top;
                top += node.height() + BOX_GAP;
            }
        }
        let mut lines = Vec::new();
        for (key, members) in &keys {
            let (to_column, to_index) = place[key];
            let mut needs: Vec<&str> = members.iter().flat_map(|job| job.needs.iter().map(String::as_str)).collect();
            needs.sort_unstable();
            needs.dedup();
            for need in needs {
                let Some(&(from_column, from_index)) = place.get(need) else { continue };
                if from_column >= to_column {
                    continue;
                }
                let (x1, y1) = (x(from_column) + COLUMN, columns[from_column].nodes[from_index].anchor());
                let (x2, y2) = (x(to_column), columns[to_column].nodes[to_index].anchor());
                lines.push((x1, y1, x2, y2));
            }
        }
        let edges = lines.iter().map(|&line| path(line, 1.0)).collect();
        Some(Self { width: x(width) - COLUMN_GAP, height, columns, edges, lines })
    }

    /// The graph for a right-to-left page: the first column on the right.
    #[must_use]
    pub fn mirrored(mut self) -> Self {
        let width = self.width;
        self.edges = self.lines.iter().map(|&(x1, y1, x2, y2)| path((width - x1, y1, width - x2, y2), -1.0)).collect();
        self
    }
}

/// A line from `(x1, y1)` to `(x2, y2)` leaving and entering level, towards `towards`
/// (1 to the right, -1 to the left).
fn path((x1, y1, x2, y2): (f64, f64, f64, f64), towards: f64) -> String {
    let (c1, c2) = (x1 + towards * COLUMN_GAP / 2.0, x2 - towards * COLUMN_GAP / 2.0);
    format!("M{x1} {y1} C{c1} {y1} {c2} {y2} {x2} {y2}")
}

fn x(column: usize) -> f64 {
    float(column) * (COLUMN + COLUMN_GAP)
}

/// A count as a coordinate; graphs never come near `u32::MAX` boxes.
fn float(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

fn column_height(column: &GraphColumn) -> f64 {
    let boxes: f64 = column.nodes.iter().map(GraphNode::height).sum();
    boxes + BOX_GAP * float(column.nodes.len().saturating_sub(1))
}

fn node(key: &str, members: &[&Job], number: i64, urls: &ActionsUrls) -> GraphNode {
    let count = u64::try_from(members.len()).unwrap_or(u64::MAX);
    if let [job] = members
        && job.matrix.is_none()
    {
        let (duration, since) = job_clock(job);
        return GraphNode {
            title: job.name.clone(),
            url: Some(urls.job(number, job.id.0)),
            look: StateLook::of(job.status),
            duration,
            since,
            jobs: Vec::new(),
            count,
            hidden: 0,
            top: 0.0,
        };
    }
    let jobs = members
        .iter()
        .take(MATRIX_ROWS)
        .map(|job| {
            let (duration, since) = job_clock(job);
            GraphJob { name: job.name.clone(), url: urls.job(number, job.id.0), look: StateLook::of(job.status), duration, since }
        })
        .collect::<Vec<_>>();
    GraphNode {
        title: key.to_owned(),
        url: None,
        look: StateLook::of(combined_status(members.iter().map(|job| job.status))),
        duration: None,
        since: None,
        hidden: count - u64::try_from(jobs.len()).unwrap_or(0),
        jobs,
        count,
        top: 0.0,
    }
}

#[cfg(test)]
#[path = "run_graph_tests.rs"]
mod tests;
