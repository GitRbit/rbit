//! Immediately-ready scripted ports; no executor/runtime dependency.
use rbit_actions::claims::{ClaimQueue, Claimed, LeaseReplay, Specification};
use std::{
    collections::VecDeque,
    future::{Future, ready},
    pin::pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
#[derive(Debug, Clone)]
pub struct Task {
    pub owned: bool,
    pub running: bool,
}
#[derive(Debug)]
pub struct State {
    pub calls: Vec<&'static str>,
    pub jobs: VecDeque<usize>,
    pub specs: VecDeque<Result<usize, String>>,
    pub failure: Option<&'static str>,
    pub task: Option<Task>,
    pub replaced: bool,
    pub job_exists: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            jobs: [1].into(),
            specs: [Ok(7)].into(),
            failure: None,
            task: Some(Task { owned: true, running: true }),
            replaced: true,
            job_exists: true,
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct Port(pub Arc<Mutex<State>>);
impl Port {
    pub fn step(&self, name: &'static str) -> Result<(), &'static str> {
        let mut state = self.0.lock().unwrap();
        state.calls.push(name);
        if state.failure == Some(name) { Err(name) } else { Ok(()) }
    }
    pub fn calls(&self) -> Vec<&'static str> {
        self.0.lock().unwrap().calls.clone()
    }
}
impl Specification for Port {
    type Error = &'static str;
    type Job = usize;
    type Task = Task;
    type Token = usize;
    type Spec = usize;
    fn new_token(&self) -> usize {
        self.step("token").unwrap();
        42
    }
    fn specification(&self, _: &usize, _: &Task, _: &usize) -> impl Future<Output = Result<Result<usize, String>, Self::Error>> + Send {
        ready(self.step("spec").map(|()| self.0.lock().unwrap().specs.pop_front().unwrap()))
    }
    fn remember_masks(&self, _: &usize) {
        self.step("masks").unwrap();
    }
    fn fail_unrunnable(&self, _: &Task, _: &str) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(self.step("fail"))
    }
}
impl ClaimQueue for Port {
    fn claim(&self, _: &usize, _: Option<&str>) -> impl Future<Output = Result<Option<Claimed<usize, Task>>, Self::Error>> + Send {
        ready(
            self.step("claim")
                .map(|()| self.0.lock().unwrap().jobs.pop_front().map(|job| Claimed { job, task: Task { owned: true, running: true } })),
        )
    }
    fn remember_key(&self, _: &str, _: &Task) {
        self.step("key").unwrap();
    }
    fn claimed(&self, _: &Claimed<usize, Task>) {
        self.step("claimed").unwrap();
    }
    fn create_steps(&self, _: &Task, _: &usize) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(self.step("steps"))
    }
    fn report_statuses(&self, _: &usize) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(self.step("statuses"))
    }
}
impl LeaseReplay for Port {
    fn lookup(&self, _: &str) -> impl Future<Output = Result<Option<Task>, Self::Error>> + Send {
        ready(self.step("lookup").map(|()| self.0.lock().unwrap().task.clone()))
    }
    fn belongs_to_runner(&self, task: &Task) -> bool {
        task.owned
    }
    fn is_running(&self, task: &Task) -> bool {
        task.running
    }
    fn replace_token(&self, _: &Task, _: &usize) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        ready(self.step("replace").map(|()| self.0.lock().unwrap().replaced))
    }
    fn job(&self, _: &Task) -> impl Future<Output = Result<Option<usize>, Self::Error>> + Send {
        ready(self.step("job").map(|()| self.0.lock().unwrap().job_exists.then_some(1)))
    }
    fn resumed(&self, _: &Task) {
        self.step("resumed").unwrap();
    }
}
pub fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected suspension"),
    }
}
