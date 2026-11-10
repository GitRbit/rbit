use rbit_core::{
    RepoId, Timestamp,
    actions::{Job, JobId, Run, RunEvent, RunId, Status},
};
use rbit_localization::Locale;

use super::*;

pub(crate) fn job(id: i64, key: &str, needs: &[&str], status: Status) -> Job {
    Job {
        id: JobId(id),
        run_id: RunId(1),
        repo_id: RepoId(1),
        key: key.to_owned(),
        name: key.to_owned(),
        matrix: None,
        needs: needs.iter().map(|need| (*need).to_owned()).collect(),
        runs_on: vec!["ubuntu-latest".to_owned()],
        permissions: None,
        status,
        attempt: 1,
        task_id: None,
        runner_id: None,
        created_at: Timestamp(0),
        queued_at: None,
        started_at: None,
        finished_at: None,
        environment: None,
        environment_url: None,
        awaiting_review: false,
    }
}

pub(crate) fn matrix(id: i64, key: &str, name: &str, needs: &[&str], status: Status) -> Job {
    Job { name: name.to_owned(), matrix: Some(format!(r#"{{"name":"{name}"}}"#)), ..job(id, key, needs, status) }
}

pub(crate) fn run(status: Status) -> Run {
    Run {
        id: RunId(1),
        repo_id: RepoId(1),
        number: 7,
        workflow_path: ".rbit/workflows/ci.yml".to_owned(),
        workflow_name: "CI".to_owned(),
        title: "Fix".to_owned(),
        event: RunEvent::Push,
        reference: "refs/heads/main".to_owned(),
        sha: "0123456789abcdef".to_owned(),
        actor: None,
        trigger_actor: None,
        pull_number: None,
        is_fork: false,
        trusted: true,
        status,
        attempt: 1,
        concurrency_group: None,
        cancel_in_progress: false,
        error: None,
        approved_by: None,
        approved_at: None,
        created_at: Timestamp(0),
        started_at: None,
        finished_at: None,
        logs_expired_at: None,
        awaiting_review: false,
    }
}

#[test]
fn jobs_come_in_needs_order_with_their_depth() {
    let jobs = [
        job(1, "deploy", &["test", "lint"], Status::Blocked),
        job(2, "test", &["build"], Status::Blocked),
        job(3, "build", &[], Status::Success),
        job(4, "lint", &[], Status::Failure),
        job(5, "cycle", &["cycle"], Status::Blocked),
    ];
    let order: Vec<(usize, &str)> = needs_order(&jobs).into_iter().map(|(depth, job)| (depth, job.key.as_str())).collect();
    assert_eq!(order[..4], [(0, "build"), (0, "lint"), (1, "test"), (2, "deploy")]);
    assert_eq!(order.len(), 5, "a job that needs itself is still listed, after the rest");
}

#[test]
fn several_states_show_what_needs_attention_first() {
    assert_eq!(combined_status([Status::Success, Status::Failure, Status::Running]), Status::Failure);
    assert_eq!(combined_status([Status::Success, Status::Running]), Status::Running);
    assert_eq!(combined_status([Status::Success, Status::Skipped]), Status::Success);
    assert_eq!(combined_status([]), Status::Skipped);
}

#[test]
fn the_sidebar_groups_a_matrix_under_its_key_and_marks_the_current_job() {
    let mut build = job(1, "build", &[], Status::Success);
    build.started_at = Some(Timestamp(100));
    build.finished_at = Some(Timestamp(160));
    let mut linux = matrix(2, "test", "test (linux)", &["build"], Status::Running);
    linux.started_at = Some(Timestamp(170));
    let windows = matrix(3, "test", "test (windows)", &["build"], Status::Failure);
    let jobs = [linux, build, windows];
    let urls = ActionsUrls::new("/a/b");
    let sidebar = RunSidebar::new(&run(Status::Running), &jobs, &urls, "/a/b/blob/x/ci.yml".to_owned(), Some(JobId(3)), Locale::English);
    assert_eq!(sidebar.groups.len(), 2);
    assert_eq!(sidebar.groups[0].name, None);
    assert_eq!(sidebar.groups[0].jobs[0].url, "/a/b/actions/runs/7/jobs/1");
    assert_eq!(sidebar.groups[1].name.as_deref(), Some("test"));
    assert_eq!(sidebar.groups[1].count, 2);
    assert_eq!(sidebar.groups[1].look.key, "failure", "a matrix shows its worst state");
    assert_eq!(sidebar.groups[1].jobs[0].since, Some(170), "a running job ticks from its start");
    assert_eq!(sidebar.groups[0].jobs[0].since, None);
    assert!(sidebar.groups[1].jobs[1].current && !sidebar.on_summary);
    assert_eq!(sidebar.current_name.as_deref(), Some("test (windows)"));
    assert_eq!(sidebar.summary_url, "/a/b/actions/runs/7");
    assert!(sidebar.usage.is_some_and(|usage| usage.0 >= 60));
    assert_eq!(sidebar.clone().polling(&run(Status::Running), "/poll".to_owned()).poll_url.as_deref(), Some("/poll"));
    assert_eq!(sidebar.polling(&run(Status::Success), "/poll".to_owned()).poll_url, None, "a finished run stays still");
}

#[test]
fn the_summary_has_no_current_job_and_no_usage_before_anything_ran() {
    let jobs = [job(1, "build", &[], Status::Waiting)];
    let sidebar = RunSidebar::new(&run(Status::Waiting), &jobs, &ActionsUrls::new("/a/b"), String::new(), None, Locale::English);
    assert!(sidebar.on_summary && sidebar.current_name.is_none() && sidebar.usage.is_none());
    assert!(!sidebar.groups[0].jobs[0].current);
}

#[test]
fn clock_patterns_carry_the_marks_the_browser_replaces() {
    let sidebar = RunSidebar::new(&run(Status::Running), &[], &ActionsUrls::new("/a/b"), String::new(), None, Locale::English);
    assert_eq!(sidebar.clock_patterns(), ["11h 22m".to_owned(), "33m 44s".to_owned(), "55s".to_owned()]);
    for locale in [Locale::German, Locale::Japanese, Locale::Russian] {
        let patterns = RunSidebar { locale, ..sidebar.clone() }.clock_patterns();
        assert!(patterns[0].contains("11") && patterns[0].contains("22") && patterns[2].contains("55"), "{patterns:?}");
    }
}
