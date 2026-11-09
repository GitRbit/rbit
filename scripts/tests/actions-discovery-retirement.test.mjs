import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkActionsDiscoveryRetirement } from "../rewrite/actions-discovery-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkActionsDiscoveryRetirement([[path, source]]);

test("a second discovery precedence, filter or cap fails in any adapter", () => {
  const cases = [
    ["crates/rbit-services/src/actions/triggers.rs", "pub const WORKFLOW_DIRS: [&str; 3] = [\".rbit/workflows\", \".forgejo/workflows\", \".github/workflows\"];\n", /directory list/u],
    ["crates/rbit-web/src/x.rs", "for dir in [\".github/workflows\", \".rbit/workflows\"] {}\n", /directory list/u],
    ["crates/rbit-server/src/x.rs", "pub const MAX_WORKFLOW_FILES: usize = 64;\n", /cap redefined/u],
    ["crates/rbit-services/src/actions/dispatch.rs", "ext.is_some_and(|extension| extension == \"yml\" || extension == \"yaml\")\n", /file filter/u],
    ["crates/rbit-services/src/actions/calls.rs", "rbit_workflow::parse(&data, limits, deadline)\n", /direct workflow parse/u],
    ["crates/rbit-actions/src/x.rs", "let jobs = rbit_workflow::plan_with(w, &c, l, d, loader)?;\n", /direct run planning/u],
    ["crates/rbit-services/src/actions/x.rs", "use rbit_workflow::{concurrency_group, Workflow};\n", /direct run planning/u],
    ["crates/rbit-services/src/actions/tokens.rs", "log.args(WORKFLOW_DIRECTORIES);\n", /directory alias/u],
    ["crates/rbit-services/src/actions/triggers.rs", "pub(super) fn read_workflow_file() {}\n", /function redefined/u],
    ["crates/rbit-services/src/actions/runs.rs", "pub struct Plan { jobs: Vec<JobTemplate> }\n", /type redefined/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source)[0] ?? "", message, source);
});

test("fixtures, stored fields, comments and other crates are allowed", () => {
  assert.deepEqual(checkActionsDiscoveryRetirement([
    ["crates/rbit-web/src/routes/api/actions.rs", "std::fs::create_dir_all(work.path().join(\".forgejo/workflows\")).unwrap();\n"],
    ["crates/rbit-services/src/actions/runs.rs", "NewRun { concurrency_group: None, ..run }\n"],
    ["crates/rbit-web/src/routes/repository_documents.rs", "async fn discover(reader: &RepositoryReader) {}\n"],
    ["crates/rbit-services/src/actions/triggers.rs", "// fn read_workflow_file was retired\n/* [\".rbit/workflows\", \".github/workflows\"] */\n"],
    ["crates/rbit-workflow/src/discovery.rs", "pub const WORKFLOW_DIRS: [&str; 3] = [\".rbit/workflows\", \".forgejo/workflows\", \".github/workflows\"];\n"],
    ["crates/rbit-runner/src/exec.rs", "let workflow = rbit_workflow::parse(&source, &limits, &deadline);\n"],
    ["crates/rbit-services/tests/x.rs", "rbit_workflow::parse(b\"on: push\", &l, &d)\n"],
  ]), []);
});

test("the current tree has no retired discovery path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkActionsDiscoveryRetirement(files), []);
  assert.match(readFileSync(new URL("crates/rbit-services/src/actions/workflow_files.rs", root), "utf8"), /impl WorkflowSource for GitWorkflows/u);
});
