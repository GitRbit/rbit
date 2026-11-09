import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkActionsPollSweepRetirement } from "../rewrite/actions-poll-sweep-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkActionsPollSweepRetirement([[path, source]]);

test("poll and stale-task decisions outside rbit-actions fail", () => {
  const cases = [
    ["crates/rbit-services/src/actions/scheduler.rs", "struct PollSlot<'a> { polls: &'a Polls }\n", /poll registry/u],
    ["crates/rbit-services/src/actions/scheduler.rs", "pub struct Polls { by_runner: Mutex<HashMap<RunnerId, u64>> }\n", /poll registry/u],
    ["crates/rbit-web/src/x.rs", "\"the server holds as many polls as it may; try again shortly\"\n", /refusal text/u],
    ["crates/rbit-services/src/actions/scheduler.rs", "if version != runner.idle_version.load(Ordering::SeqCst) {}\n", /claim-or-wait/u],
    ["crates/rbit-services/src/actions/sweeper.rs", "let silent = store.silent_tasks(cutoff).await?;\n", /stale-task selection/u],
    ["crates/rbit-services/src/actions/sweeper.rs", "for settled in store.cancel_abandoned(a, b).await? {}\n", /stale-task selection/u],
    ["crates/rbit-services/src/actions/sweeper.rs", "async fn time_out(&self, task: TaskId) {}\n", /pass redefined/u],
    ["crates/rbit-services/src/actions/sweeper.rs", "format!(\"the task ran longer than {} seconds, so it failed\", s)\n", /pass redefined/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source)[0] ?? "", message, source);
});

test("the owner, the adapters, the runner cache, tests and comments are allowed", () => {
  assert.deepEqual(checkActionsPollSweepRetirement([
    ["crates/rbit-actions/src/polling.rs", "pub struct PollSlot<'a, R, E> { by_runner: Mutex<()> }\n"],
    ["crates/rbit-services/src/actions/stale_task_port.rs", "store.silent_tasks(Timestamp(before)).await?\n"],
    ["crates/rbit-services/src/actions/scheduler/poll_queue.rs", "self.runner.idle_version.load(Ordering::SeqCst)\n"],
    ["crates/rbit-services/src/actions/runners.rs", "fresh.idle_version.store(self.idle_version.load(Ordering::SeqCst), Ordering::SeqCst);\n"],
    ["crates/rbit-services/src/actions/sweeper.rs", "// fn time_out retired\nfn x() {}\n#[cfg(test)]\nmod tests {\n  fn time_out() {}\n}\n"],
    ["crates/rbit-storage/src/sqlite/actions_runs.rs", "pub async fn silent_tasks(&self) {}\n"],
  ]), []);
});

test("the current tree has no retired poll or stale-task path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkActionsPollSweepRetirement(files), []);
});
