import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkActionsTriggersRetirement } from "../rewrite/actions-triggers-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkActionsTriggersRetirement([[path, source]]);

test("trigger decisions returning to an adapter fail", () => {
  const cases = [
    ["crates/rbit-services/src/actions/triggers.rs", "pub const MAX_REF_UPDATES: usize = 16;\n", /limit redefined/u],
    ["crates/rbit-web/src/x.rs", "const EVALUATION_BUDGET: Duration = Duration::from_secs(10);\n", /limit redefined/u],
    ["crates/rbit-services/src/actions/triggers.rs", "updates.iter().filter(f).take(MAX_REF_UPDATES).cloned()\n", /cap applied/u],
    ["crates/rbit-services/src/actions/triggers.rs", "DiffLimits { max_files: MAX_CHANGED_PATHS + 1, ..l }\n", /cap applied/u],
    ["crates/rbit-services/src/actions/triggers.rs", "if comparison.files.len() > MAX_CHANGED_PATHS {}\n", /cap applied/u],
    ["crates/rbit-services/src/actions/triggers.rs", "(None, Some(branch)) if branch != default_branch => Some(x),\n", /comparison base/u],
    ["crates/rbit-services/src/actions/triggers.rs", "IssueAction::Synchronized { source, .. } if source.starts_runs() => x,\n", /synchronize gating/u],
    ["crates/rbit-services/src/actions/triggers.rs", "matches!(run.status, Status::Waiting | Status::Blocked | Status::AwaitingApproval)\n", /close cancellation/u],
    ["crates/rbit-services/src/actions/schedules.rs", "pub const PAUSE_AFTER_FAILURES: i64 = 3;\n", /limit redefined/u],
    ["crates/rbit-services/src/actions/schedules.rs", "&& matches!(previous.status, Status::Waiting | Status::Blocked)\n", /schedule waiting rule/u],
    ["crates/rbit-services/src/actions/chains.rs", "if self.chain_depth(&run).await? >= MAX_WORKFLOW_RUN_CHAIN {}\nwhile depth < MAX_WORKFLOW_RUN_CHAIN {}\n", /dispatch or chain limit/u],
    ["crates/rbit-services/src/actions/chains.rs", "if event_type.chars().count() > MAX_DISPATCH_EVENT_TYPE_CHARS {}\n", /dispatch or chain limit/u],
    ["crates/rbit-web/src/routes/api/actions.rs", "if payload.len() > MAX_CLIENT_PAYLOAD_PROPERTIES {}\n", /dispatch or chain limit/u],
    ["crates/rbit-services/src/actions/dispatch.rs", "Err(ServiceError::Invalid(\"the ref must be a branch or tag of this repository\".into()))\n", /dispatch ref resolution/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source)[0] ?? "", message, source);
});

test("the decisions crate, projections, other limits and comments are allowed", () => {
  assert.deepEqual(checkActionsTriggersRetirement([
    ["crates/rbit-actions/src/triggers.rs", "pub const MAX_REF_UPDATES: usize = 16;\n(None, Some(branch)) if branch != default_branch => Some(d),\n"],
    ["crates/rbit-services/src/actions/trigger_inputs.rs", "Status::Waiting => RunStage::Waiting,\nStatus::Blocked => RunStage::Held,\n"],
    ["crates/rbit-services/src/actions/trigger_inputs.rs", "IssueAction::Synchronized { source, .. } => PullEvent::Synchronized { starts_runs: source.starts_runs(), pusher: p },\n"],
    ["crates/rbit-services/src/actions/scheduler.rs", "const MAX_RUNS: usize = 16;\n"],
    ["crates/rbit-services/src/actions/chains.rs", "pub use rbit_actions::triggers::events::{MAX_DISPATCH_EVENT_TYPE_CHARS, MAX_WORKFLOW_RUN_CHAIN};\n"],
    ["crates/rbit-web/src/routes/api/actions.rs", "(length == 0 || length > rbit_services::actions::chains::MAX_DISPATCH_EVENT_TYPE_CHARS).then_some(\"event_type\")\n"],
    ["crates/rbit-services/src/actions/triggers.rs", "// .take(MAX_REF_UPDATES) is decided in rbit-actions\n"],
  ]), []);
});

test("the current tree has no trigger decision in an adapter", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkActionsTriggersRetirement(files), []);
  assert.match(readFileSync(new URL("crates/rbit-services/src/actions/triggers.rs", root), "utf8"), /decide::considered_refs/u);
});
