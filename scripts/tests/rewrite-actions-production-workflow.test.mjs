import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { relevance } from "../../devtools/actions-visual/relevance.mjs";
import { productionRelevantPath } from "../../devtools/actions-visual/production-relevance.mjs";
import { productionBuildArgs } from "../../devtools/actions-visual/export-production.mjs";

const before = "a".repeat(40), after = "b".repeat(40);
const event = { action: "synchronize", before, after, pull_request: { head: { sha: after } } };
const changed = paths => async args => args[0] === "diff" ? paths.map(path => `${path}\0`).join("") : "";
const read = path => readFile(new URL(`../../${path}`, import.meta.url), "utf8");
const workflowPath = ".github/workflows/actions-production-visual.yml";

// These fixtures execute the real event decision, including unknown comparisons.
test("production changes run; a later docs-only event skips without entering the capture group", async () => {
  for (const path of [workflowPath, ".github/workflows/ci.yml", "Cargo.lock", "crates/rbit-auth/src/lib.rs",
    "crates/rbit-web/templates/actions/run.html", "crates/rbit-web/assets/app.bundle.js",
    "devtools/actions-visual/production-run.mjs", ".cargo/config.toml",
    "scripts/tests/rewrite-actions-production-workflow.test.mjs"]) {
    assert.equal((await relevance("pull_request", event, changed([path]), productionRelevantPath)).run, true, path);
  }
  const result = await relevance("pull_request", event, changed(["README.md", "docs/design/rewrite.md"]), productionRelevantPath);
  assert.deepEqual(result, { run: false, reason: "NO_RELEVANT_EVENT_DELTA", changedPaths: 2 });
  assert.equal((await relevance("pull_request", event, async () => { throw Error("missing"); }, productionRelevantPath)).run, true);
  for (const initial of [{ action: "opened" }, { action: "reopened" }, { action: "synchronize", before: "bad" }]) {
    assert.equal((await relevance("pull_request", initial, changed([]), productionRelevantPath)).run, true);
  }
  for (const name of ["workflow_dispatch", "push"]) {
    assert.equal((await relevance(name, {}, changed([]), productionRelevantPath)).run, true);
  }
});

test("only admitted Linux captures have concurrency; unrelated required CI cancellation is isolated", async () => {
  const workflow = await read(workflowPath), ci = await read(".github/workflows/ci.yml");
  const [preamble, jobs] = workflow.split("\njobs:\n");
  assert.doesNotMatch(preamble, /^concurrency:/m);
  assert.match(preamble, /pull_request:\s+paths:/);
  assert.match(preamble, /push:\s+branches: \[main\]\s+paths:/);
  assert.equal(preamble.split(`- '${workflowPath}'`).length - 1, 2, "new workflow addition triggers both source event families");
  assert.match(preamble, /workflow_dispatch:/);
  const [relevanceJob, capture] = jobs.split("\n  production:");
  assert.doesNotMatch(relevanceJob, /concurrency:|docker |cargo |npm ci|container\.mjs/);
  assert.match(relevanceJob, /run: node devtools\/actions-visual\/production-relevance\.mjs/);
  assert.match(capture, /needs: relevance/);
  assert.ok(capture.includes("if: ${{ always() && !cancelled() && (needs.relevance.result != 'success' || needs.relevance.outputs.run != 'false') }}"));
  assert.match(capture, /concurrency:\s+group: actions-production-capture-\$\{\{ github.ref \}\}\s+cancel-in-progress: false/);
  assert.match(capture, /runs-on: ubuntu-24\.04/);
  assert.doesNotMatch(capture, /needs: test|^    strategy:|windows-latest|continue-on-error:/m);
  assert.doesNotMatch(ci, /actions-production-visual:|export-production\.mjs|actions-production-fixture/);
  assert.match(ci, /group: ci-\$\{\{ github.workflow \}\}-\$\{\{ github.ref \}\}/);
});

test("same-job focused build and assets precede the original fail-closed sandbox/evidence pipeline", async () => {
  const workflow = await read(workflowPath);
  const job = workflow.split("\n  production:")[1];
  const ordered = ["npm run build", "git diff --exit-code -- crates/rbit-web/assets", "node devtools/actions-visual/export-production.mjs",
    "npm ci --ignore-scripts --prefix devtools/actions-visual", "node devtools/actions-visual/container.mjs --production",
    "node devtools/actions-visual/cleanup.mjs --production", "node devtools/actions-visual/artifacts.mjs actions-production-artifacts",
    "name: actions-production-linux-"];
  for (let i = 1; i < ordered.length; i++) assert.ok(job.indexOf(ordered[i]) > job.indexOf(ordered[i - 1]), ordered[i]);
  assert.match(job, /if: \$\{\{ always\(\) \}\}\s+run: node devtools\/actions-visual\/cleanup\.mjs --production/);
  assert.match(job, /if: \$\{\{ always\(\) && !cancelled\(\) && steps.artifacts.outcome == 'success' \}\}/);
  assert.match(job, /path: actions-production-artifacts\/\s+if-no-files-found: error\s+retention-days: 3/);
  assert.doesNotMatch(job, /download-artifact|name: actions-production-fixture|path:.*(?:fixture|control|cookie|storageState)/);
  assert.match(job, /uses: \.\/.github\/actions\/prepare-build/);
  assert.match(workflow, /CARGO_INCREMENTAL: '0'/);
  for (const name of ["CARGO_BUILD_JOBS", "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG"]) {
    assert.match(workflow, new RegExp(`${name}: '${name === "CARGO_BUILD_JOBS" ? "1" : "0"}'`));
  }
  assert.deepEqual(productionBuildArgs, ["nextest", "list", "--package", "rbit-web", "--test", "integration", "--locked",
    "--list-type", "binaries-only", "--message-format", "json"]);
  assert.ok(Object.isFrozen(productionBuildArgs));
  const exporter = await read("devtools/actions-visual/export-production.mjs");
  assert.match(exporter, /await run\("cargo", productionBuildArgs, 2400000\)/);
  assert.doesNotMatch(exporter, /--workspace|--release|--features/);
});

test("candidate workflow is low-trust pinned read-only and cannot consume cross-run artifacts", async () => {
  const workflow = await read(workflowPath);
  assert.match(workflow, /permissions:\s+contents: read/);
  assert.doesNotMatch(workflow, /workflow_run:|pull_request_target:|secrets\.|:\s*(?:write|write-all)\s*$|github-token:|run-id:|repository:/m);
  for (const match of workflow.matchAll(/uses: ([^\s#]+)/g)) {
    if (!match[1].startsWith("./")) assert.match(match[1], /@[a-f0-9]{40}$/);
  }
  assert.equal([...workflow.matchAll(/uses: actions\/checkout@/g)].length, 2);
  assert.equal([...workflow.matchAll(/persist-credentials: false/g)].length, 2);
  for (const field of ["cache-workspace-crates", "cache-all-crates", "cache-bin", "cache-on-failure"]) {
    assert.match(workflow, new RegExp(`${field}: false`));
  }
  assert.match(workflow, /github.event_name == 'pull_request'.*startsWith\(github.ref, 'refs\/pull\/'\)/);
  assert.doesNotMatch(workflow, /--no-sandbox|disable-setuid|seccomp=unconfined|--privileged/);
});

test("artifact validation still precedes execute-mode restoration and fixed contained launch", async () => {
  const source = await read("devtools/actions-visual/container.mjs");
  assert.ok(source.indexOf("await verifyProductionArtifact") < source.indexOf("await chmod"));
  assert.match(source, /chmod\(fixture\.binary, 0o500\)/);
});
