//! A pipe task must be joined after abort before its captured streams are released.

use tokio::task::JoinHandle;

#[derive(Debug)]
pub(super) struct Pump(Option<JoinHandle<()>>);

impl Pump {
    pub(super) fn new(task: JoinHandle<()>) -> Self {
        Self(Some(task))
    }

    pub(super) async fn stop(mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for Pump {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}
