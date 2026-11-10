//! The run's annotations, as GitHub lists them on a run's summary: every error, warning
//! and notice its jobs' steps reported (`::error …::`, problem matchers), and every failed
//! step without an error of its own, each linking to its job at the line.
//!
//! A link names the line as `#step-{number}-line-{line}`: the step's number (from 0) and
//! the line's number in that step's log (from 1), the anchors the job page gives its log
//! lines; a failed step links to `#step-{number}`.
use rbit_core::actions::{Job, Status};
use rbit_services::actions::annotations::{AnnotationLevel, RunAnnotations, annotation_text};

use super::{ActionsUrls, run_layout::needs_order};

/// Annotations listed on the run page; the rest are counted.
pub const MAX_LISTED: usize = 50;

/// One annotation on the run page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationView {
    /// `error`, `warning` or `notice`.
    pub level: &'static str,
    pub job: String,
    /// The job, at the annotation's line when it has one.
    pub url: String,
    pub title: Option<String>,
    /// The message; `None` for a failed step, which the page words itself.
    pub message: Option<String>,
    /// The failed step, for a failed step without an error.
    pub failed_step: Option<String>,
    /// `src/main.rs:12`, when the annotation names a file.
    pub location: Option<String>,
}

/// The run page's annotations box.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnnotationsView {
    pub items: Vec<AnnotationView>,
    pub errors: u64,
    pub warnings: u64,
    pub notices: u64,
    /// Annotations not listed.
    pub more: u64,
}

impl AnnotationsView {
    /// The annotations of run `number`'s `jobs`, in `needs` order, errors of failed steps
    /// added for the failed steps that reported none.
    pub fn new(read: &RunAnnotations, jobs: &[Job], number: i64, urls: &ActionsUrls) -> Self {
        let mut all = Vec::new();
        for (_, job) in needs_order(jobs) {
            let job_url = urls.job(number, job.id.0);
            let own = read.annotations.iter().filter(|annotation| annotation.job == job.id);
            let mut erred_steps = Vec::new();
            for annotation in own {
                let text = annotation_text(&annotation.text);
                if annotation.level == AnnotationLevel::Error {
                    erred_steps.push(annotation.step.map(|(step, _)| step));
                }
                let url = match annotation.step {
                    Some((step, line)) => format!("{job_url}#step-{step}-line-{line}"),
                    None => job_url.clone(),
                };
                all.push(AnnotationView {
                    level: annotation.level.as_str(),
                    job: job.name.clone(),
                    url,
                    title: text.title.map(str::to_owned),
                    message: Some(text.message.to_owned()),
                    failed_step: None,
                    location: text.file.map(|file| text.line.map_or_else(|| file.to_owned(), |line| format!("{file}:{line}"))),
                });
            }
            let Some(task) = job.task_id.filter(|_| job.status == Status::Failure) else { continue };
            for step in read.steps.iter().filter(|step| step.task_id == task && step.status == Status::Failure) {
                if erred_steps.contains(&Some(step.number)) {
                    continue;
                }
                all.push(AnnotationView {
                    level: AnnotationLevel::Error.as_str(),
                    job: job.name.clone(),
                    url: format!("{job_url}#step-{}", step.number),
                    title: None,
                    message: None,
                    failed_step: Some(step.name.clone()),
                    location: None,
                });
            }
        }
        let count = |level: AnnotationLevel| u64::try_from(all.iter().filter(|item| item.level == level.as_str()).count()).unwrap_or(0);
        let (errors, warnings, notices) = (count(AnnotationLevel::Error), count(AnnotationLevel::Warning), count(AnnotationLevel::Notice));
        // Errors first, then warnings, then notices; each in job order.
        all.sort_by_key(|item| ["error", "warning", "notice"].iter().position(|level| *level == item.level));
        let more = u64::try_from(all.len().saturating_sub(MAX_LISTED)).unwrap_or(0);
        all.truncate(MAX_LISTED);
        Self { items: all, errors, warnings, notices, more }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{
        Timestamp,
        actions::{JobId, Step, TaskId},
    };
    use rbit_services::actions::annotations::Annotation;

    use super::*;
    use crate::views::actions::run_layout::tests::job;

    fn step(task: i64, number: i64, name: &str, status: Status) -> Step {
        Step {
            task_id: TaskId(task),
            number,
            name: name.to_owned(),
            status,
            started_at: Some(Timestamp(1)),
            stopped_at: Some(Timestamp(2)),
            log_index: 0,
            log_length: 0,
        }
    }

    #[test]
    fn annotations_link_to_their_line_and_failed_steps_without_errors_are_added() {
        let mut build = job(1, "build", &[], Status::Failure);
        build.task_id = Some(TaskId(10));
        let mut lint = job(2, "lint", &[], Status::Success);
        lint.task_id = Some(TaskId(11));
        let read = RunAnnotations {
            annotations: vec![
                Annotation {
                    job: JobId(1),
                    level: AnnotationLevel::Error,
                    text: "mismatched types (file=src/a.rs,line=3,title=E0308)".into(),
                    step: Some((1, 4)),
                },
                Annotation { job: JobId(2), level: AnnotationLevel::Warning, text: "unused".into(), step: None },
            ],
            steps: vec![
                step(10, 0, "Set up", Status::Success),
                step(10, 1, "Compile", Status::Failure),
                step(10, 2, "Test", Status::Failure),
            ],
        };
        let view = AnnotationsView::new(&read, &[build, lint], 7, &ActionsUrls::new("/a/b"));
        assert_eq!((view.errors, view.warnings, view.notices, view.more), (2, 1, 0, 0));
        let first = &view.items[0];
        assert_eq!(first.url, "/a/b/actions/runs/7/jobs/1#step-1-line-4");
        assert_eq!((first.message.as_deref(), first.title.as_deref()), (Some("mismatched types"), Some("E0308")));
        assert_eq!(first.location.as_deref(), Some("src/a.rs:3"));
        let failed = &view.items[1];
        assert_eq!((failed.failed_step.as_deref(), failed.url.as_str()), (Some("Test"), "/a/b/actions/runs/7/jobs/1#step-2"));
        assert_eq!(view.items[2].url, "/a/b/actions/runs/7/jobs/2", "outside every step: the job");
        assert_eq!(view.items.len(), 3, "the compile step reported its error");
    }

    #[test]
    fn long_lists_are_cut_and_counted() {
        let mut build = job(1, "build", &[], Status::Success);
        build.task_id = Some(TaskId(1));
        let annotations = (0..60)
            .map(|line| Annotation { job: JobId(1), level: AnnotationLevel::Notice, text: format!("n{line}"), step: Some((0, line)) })
            .collect();
        let view = AnnotationsView::new(&RunAnnotations { annotations, steps: Vec::new() }, &[build], 1, &ActionsUrls::new("/a/b"));
        assert_eq!((view.items.len(), view.notices, view.more), (MAX_LISTED, 60, 10));
        assert!(AnnotationsView::default().is_empty());
    }
}
