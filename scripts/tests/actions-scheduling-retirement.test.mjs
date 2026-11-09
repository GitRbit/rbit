import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkActionsSchedulingRetirement } from "../rewrite/actions-scheduling-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkActionsSchedulingRetirement([[path, source]]);

test("a second admission cap, allowance, group decision or lease check fails in the adapters", () => {
  const cases = [
    ["crates/rbit-services/src/actions/runs.rs", "format!(\"{}: this event already started its share of jobs; at most {max}\", p)\n", /admission cap/u],
    ["crates/rbit-services/src/actions/runs.rs", "const MAX_JOB_LABEL_CHARS: usize = 255;\n", /admission cap/u],
    ["crates/rbit-web/src/x.rs", "format!(\"has {waiting} jobs waiting for a runner; at most {max} may wait\")\n", /admission cap/u],
    ["crates/rbit-services/src/actions/runs.rs", "allowance.runs -= 1;\n", /allowance/u],
    ["crates/rbit-services/src/actions/runs.rs", "let mut busy = false;\n", /concurrency group/u],
    ["crates/rbit-server/src/x.rs", "matches!(run.status, Status::Blocked | Status::AwaitingApproval)\n", /concurrency group/u],
    ["crates/rbit-services/src/actions/artifacts.rs", "async fn leased_task(&self) {}\n", /lease check/u],
    ["crates/rbit-services/src/actions/caches.rs", "RunnerError::conflict(\"the job has been rerun; this task no longer belongs to it\")\n", /task lease/u],
    ["crates/rbit-services/src/actions/tasks.rs", "RunnerError::forbidden(\"this task belongs to another runner\")\n", /task lease/u],
    ["crates/rbit-services/src/actions/logs/append.rs", "RunnerError::conflict(\"the task has ended; its log is closed\")\n", /task lease/u],
    ["crates/rbit-services/src/actions/conditions.rs", "let outcome = Condition::parse(condition);\n", /job decision/u],
    ["crates/rbit-services/src/actions/calls.rs", "const fn result_name(result: JobResult) -> &'static str { todo!() }\n", /job decision/u],
    ["crates/rbit-services/src/actions/conditions.rs", "if template.strategy.fail_fast && template.strategy.job_total > 1 {}\n", /job decision/u],
    ["crates/rbit-services/src/actions/runs.rs", "ServiceError::Invalid(\"only a finished run can be run again\".to_owned())\n", /request rule/u],
    ["crates/rbit-services/src/actions/environments.rs", "format!(\"no job of this run waits for {name}\")\n", /request rule/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source).join("\n"), message, source);
});

test("the owner, the lease adapter, other crates and comments are allowed", () => {
  assert.deepEqual(checkActionsSchedulingRetirement([
    ["crates/rbit-actions/src/scheduling.rs", "pub const MAX_JOB_LABEL_CHARS: usize = 255;\nlet mut busy = false;\n"],
    ["crates/rbit-services/src/actions/task_leases.rs", "pub(super) async fn leased_task(&self) {}\n"],
    ["crates/rbit-storage/src/sqlite/actions_runs.rs", "WHERE status IN (Status::Blocked | Status::AwaitingApproval)\n"],
    ["crates/rbit-services/src/actions/runs.rs", "// allowance.runs -= 1 moved to rbit_actions::scheduling\n"],
    ["crates/rbit-web/src/routes/api/environments.rs", "format!(\"no job of this run waits for environment {id}\")\n"],
    ["crates/rbit-services/src/actions/triggers.rs", "matches!(run.status, Status::Waiting | Status::Blocked | Status::AwaitingApproval)\n"],
    ["crates/rbit-services/src/actions/hooks.rs", "Status::Waiting | Status::Running | Status::Blocked | Status::AwaitingApproval => None,\n"],
  ]), []);
});

test("the current tree has no retired scheduling decision", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkActionsSchedulingRetirement(files), []);
});
