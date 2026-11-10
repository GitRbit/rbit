//! Log lines that belong to a step that never ran.

use rbit_workflow::protocol::StepStatus;

impl super::Reporter {
    /// Step `index` is skipped: `why` is its only log line, and it never starts, as on
    /// GitHub ("This step was skipped because …").
    pub fn skip_step(&self, index: usize, why: &str) {
        let line = self.log.len();
        {
            let mut progress = self.lock();
            let Some(step) = progress.steps.get_mut(index) else { return };
            step.log_index = line;
            step.log_length = 0;
            progress.current = Some(index);
        }
        self.log(why);
        let mut progress = self.lock();
        if progress.current == Some(index) {
            progress.current = None;
        }
        if let Some(step) = progress.steps.get_mut(index) {
            step.status = StepStatus::Skipped;
            step.started_at = None;
            step.stopped_at = None;
        }
        drop(progress);
        self.changed.notify_one();
    }
}
