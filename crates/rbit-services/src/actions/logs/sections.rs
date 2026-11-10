//! The parts of a task log outside its steps, which the job page shows as "Set up job"
//! (before the first step: the runner, its labels, containers and actions) and "Complete
//! job" (after the last: cleanup and the result), as GitHub does.

use rbit_core::actions::Step;

/// Whether `step` has lines: it ran, or the runner explained why it was skipped.
pub fn has_log(step: &Step) -> bool {
    step.started_at.is_some() || step.log_length > 0
}

/// Task-log line ranges `start..end` (`None`: to the end of the log).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sections {
    /// Lines before the first step's; all of them while no step has any.
    pub setup: Option<(u64, Option<u64>)>,
    /// Lines after the last step's, once every step is done.
    pub complete: Option<u64>,
}

/// Where the set-up and completion lines of a task with `steps` are; `finished` when its
/// job is done, whatever steps the runner left waiting.
pub fn sections(steps: &[Step], finished: bool) -> Sections {
    let logged: Vec<&Step> = steps.iter().filter(|step| has_log(step)).collect();
    let line = |value: i64| u64::try_from(value).unwrap_or_default();
    let Some(first) = logged.iter().map(|step| line(step.log_index)).min() else {
        return Sections { setup: Some((0, None)), complete: None };
    };
    let done = finished || steps.iter().all(|step| step.status.is_done());
    let end = logged.iter().map(|step| line(step.log_index).saturating_add(line(step.log_length))).max().unwrap_or(first);
    Sections { setup: (first > 0).then_some((0, Some(first))), complete: done.then_some(end) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbit_core::actions::{Status, TaskId};

    fn step(number: i64, status: Status, started: bool, log_index: i64, log_length: i64) -> Step {
        Step {
            task_id: TaskId(1),
            number,
            name: String::new(),
            status,
            started_at: started.then_some(rbit_core::Timestamp(1)),
            stopped_at: None,
            log_index,
            log_length,
        }
    }

    #[test]
    fn set_up_lines_come_before_the_first_step_and_completion_lines_after_the_last() {
        assert_eq!(sections(&[step(0, Status::Waiting, false, 0, 0)], false), Sections { setup: Some((0, None)), complete: None });
        let running = [step(0, Status::Running, true, 7, 0), step(1, Status::Waiting, false, 0, 0)];
        assert_eq!(sections(&running, false), Sections { setup: Some((0, Some(7))), complete: None });
        let done = [step(0, Status::Success, true, 7, 3), step(1, Status::Skipped, false, 10, 1), step(2, Status::Skipped, false, 11, 0)];
        assert_eq!(sections(&done, false), Sections { setup: Some((0, Some(7))), complete: Some(11) });
        assert!(has_log(&done[1]) && !has_log(&done[2]), "a skipped step with a reason has a log");
        let left_waiting = [step(0, Status::Failure, true, 2, 1), step(1, Status::Waiting, false, 0, 0)];
        assert_eq!(sections(&left_waiting, false).complete, None);
        assert_eq!(sections(&left_waiting, true).complete, Some(3), "a finished job's last lines complete it");
        assert_eq!(sections(&[step(0, Status::Success, true, 0, 2)], false).setup, None);
    }
}
