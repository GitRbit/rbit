use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_notifications::webhooks::{
    policy::NextStep,
    queue::{HookQueue, HookScope, QueuedDelivery, queue, redeliver, select},
    worker::{AttemptReport, DeliveryQueue, deliver_due, settle},
};
use serde_json::json;
use std::{
    future::{Future, ready},
    sync::Mutex,
};

/// Both ports; `trace` records calls in order.
#[derive(Default)]
struct World {
    trace: Mutex<Vec<String>>,
    subscribed: Vec<i64>,
    /// Batches `claim_due` hands out, in order.
    batches: Mutex<Vec<Vec<i64>>>,
}
impl World {
    fn push(&self, entry: String) {
        self.trace.lock().unwrap().push(entry);
    }
    fn trace(&self) -> Vec<String> {
        self.trace.lock().unwrap().clone()
    }
}

impl HookQueue for World {
    type Error = &'static str;
    type Hook = i64;
    type Event = &'static str;
    type Delivery = usize;
    fn event_name(event: &'static str) -> &'static str {
        event
    }
    fn subscribed(&self, scope: &HookScope, event: &'static str) -> impl Future<Output = Result<Vec<i64>, &'static str>> + Send {
        self.push(format!(
            "subscribed:{:?}:{:?}:{event}",
            scope.repo.map(|repo| repo.0),
            scope.orgs.iter().map(|org| org.0).collect::<Vec<_>>()
        ));
        ready(Ok(self.subscribed.clone()))
    }
    fn enqueue(&self, deliveries: Vec<QueuedDelivery<i64, &'static str>>) -> impl Future<Output = Result<Vec<usize>, &'static str>> + Send {
        for delivery in &deliveries {
            self.push(format!("enqueue:{}:{}:{}:{:?}:{}", delivery.hook, delivery.guid, delivery.event, delivery.action, delivery.payload));
        }
        ready(Ok((0..deliveries.len()).collect()))
    }
    fn new_guid(&self) -> String {
        format!("g{}", self.trace().len())
    }
    fn wake(&self) {
        self.push("wake".into());
    }
}

#[tokio::test]
async fn only_hooks_that_existed_when_the_event_was_published_are_selected() {
    let world = World { subscribed: vec![3, 5, 9], ..World::default() };
    let scope = HookScope::transferred(RepoId(7), Some(AccountId(2)), Some(AccountId(4)));
    assert_eq!(select(&world, 5, &scope, "repository").await.unwrap(), [3, 5]);
    assert_eq!(world.trace(), ["subscribed:Some(7):[2, 4]:repository"]);
    assert_eq!(HookScope::deleted(AccountId(2)), HookScope { repo: None, orgs: vec![AccountId(2)] });
    assert_eq!(HookScope::repository(RepoId(7), None).orgs, []);
}

#[tokio::test]
async fn queueing_gives_each_hook_its_own_guid_records_actions_and_wakes_once() {
    let world = World::default();
    assert_eq!(queue(&world, Vec::new(), "push", &json!({})).await.unwrap(), Vec::<usize>::new());
    assert_eq!(world.trace(), Vec::<String>::new(), "no hooks: no store call and no wake-up");
    let ids = queue(&world, vec![3, 5], "issues", &json!({ "action": "opened" })).await.unwrap();
    assert_eq!(ids, [0, 1]);
    assert_eq!(
        world.trace(),
        [r#"enqueue:3:g0:issues:Some("opened"):{"action":"opened"}"#, r#"enqueue:5:g0:issues:Some("opened"):{"action":"opened"}"#, "wake",]
    );
    let world = World::default();
    queue(&world, vec![3], "push", &json!({ "action": "x" })).await.unwrap();
    assert_eq!(world.trace()[0], r#"enqueue:3:g0:push:None:{"action":"x"}"#, "push deliveries have no action");
    let world = World::default();
    redeliver(&world, 3, "release", Some("published".into()), "{}".into()).await.unwrap();
    assert_eq!(world.trace(), [r#"enqueue:3:g0:release:Some("published"):{}"#, "wake"]);
}

impl DeliveryQueue for World {
    type Error = &'static str;
    type Claimed = i64;
    type Record = u16;
    fn claim_due(&self, now: Timestamp, limit: u32) -> impl Future<Output = Result<Vec<i64>, &'static str>> + Send {
        self.push(format!("claim:{}:{limit}", now.0));
        let mut batches = self.batches.lock().unwrap();
        ready(Ok(if batches.is_empty() { Vec::new() } else { batches.remove(0) }))
    }
    fn run_all(&self, claimed: Vec<i64>) -> impl Future<Output = ()> + Send {
        self.push(format!("run:{claimed:?}"));
        ready(())
    }
    fn record(&self, claimed: &i64, record: u16, step: NextStep) -> impl Future<Output = ()> + Send {
        self.push(format!("record:{claimed}:{record}:{step:?}"));
        ready(())
    }
    fn observe(&self, claimed: &i64, record: &u16, step: NextStep) {
        self.push(format!("observe:{claimed}:{record}:{}", step.label()));
    }
}

#[tokio::test]
async fn the_worker_drains_bounded_batches_and_counts_attempts() {
    let world = World { batches: Mutex::new(vec![vec![1, 2], vec![3]]), ..World::default() };
    assert_eq!(deliver_due(&world, Timestamp(9), 0).await, Ok(3));
    assert_eq!(world.trace(), ["claim:9:1", "run:[1, 2]", "claim:9:1", "run:[3]", "claim:9:1"]);
}

#[tokio::test]
async fn attempts_are_observed_then_recorded_with_their_next_step() {
    let world = World::default();
    let report = AttemptReport { succeeded: false, permanent: false, record: 500 };
    let step = settle(&world, &4, 0, 5, report, || Timestamp(100), || 0.5).await;
    assert_eq!(step, NextStep::Retry(Timestamp(110)));
    assert_eq!(world.trace(), ["observe:4:500:retrying", "record:4:500:Retry(Timestamp(110))"]);
    let report = AttemptReport { succeeded: true, permanent: false, record: 204 };
    let never = || -> Timestamp { panic!("no retry, no clock") };
    assert_eq!(settle(&world, &4, 4, 5, report, never, || 0.5).await, NextStep::Succeeded);
}
