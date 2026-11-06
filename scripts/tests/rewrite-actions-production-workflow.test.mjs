import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { relevance } from "../../devtools/actions-visual/relevance.mjs";
import { productionRelevantPath } from "../../devtools/actions-visual/production-relevance.mjs";

const before = "a".repeat(40), after = "b".repeat(40);
const event = { action: "synchronize", before, after, pull_request: { head: { sha: after } } };
const changed = paths => async args => args[0] === "diff" ? paths.map(path => `${path}\0`).join("") : "";

test("real production dependency changes run while irrelevant later PR edits skip", async () => {
  for (const path of [".github/workflows/ci.yml", "Cargo.lock", "crates/rbit-auth/src/lib.rs",
    "crates/rbit-web/templates/actions/run.html", "crates/rbit-web/assets/app.bundle.js",
    "devtools/actions-visual/production-run.mjs", ".cargo/config.toml",
    "scripts/tests/rewrite-actions-production-workflow.test.mjs"]) {
    assert.equal((await relevance("pull_request", event, changed([path]), productionRelevantPath)).run, true, path);
  }
  const result = await relevance("pull_request", event, changed(["README.md", "docs/design/rewrite.md"]), productionRelevantPath);
  assert.deepEqual(result, { run: false, reason: "NO_RELEVANT_EVENT_DELTA", changedPaths: 2 });
  assert.equal((await relevance("pull_request", event, async () => { throw Error("missing"); }, productionRelevantPath)).run, true);
  assert.equal((await relevance("workflow_dispatch", {}, changed([]), productionRelevantPath)).run, true);
  assert.equal((await relevance("push", {}, changed([]), productionRelevantPath)).run, true);
});

test("CI hands off only existing public inputs and contains the optional consuming job", async () => {
  const workflow = await readFile(new URL("../../.github/workflows/ci.yml", import.meta.url), "utf8");
  const testJob = workflow.split("\n  test:")[1].split("\n  actions-production-visual:")[0];
  assert.ok(testJob.indexOf("cargo nextest run --workspace --locked --profile ci")
    < testJob.indexOf("node devtools/actions-visual/export-production.mjs"));
  assert.match(testJob, /if: runner\.os == 'Linux' && steps\.production-visual\.outputs\.run != 'false'/);
  assert.match(testJob, /path: actions-production-fixture\/\s+if-no-files-found: error\s+retention-days: 1/);
  assert.doesNotMatch(testJob, /path:.*(?:target|\.worktree-state|\.sqlite)/);
  const job = workflow.split("\n  actions-production-visual:")[1].split("\n  assets:")[0];
  assert.match(job, /needs: test/);
  assert.match(job, /runs-on: ubuntu-24\.04/);
  assert.match(job, /permissions:\s+contents: read/);
  assert.match(job, /persist-credentials: false/);
  assert.match(job, /name: actions-production-fixture\s+path: actions-production-fixture/);
  assert.doesNotMatch(job, /run-id:|repository:|github-token:|secrets\.|cargo |npm run build/);
  assert.match(job, /if: \$\{\{ always\(\) \}\}\s+run: node devtools\/actions-visual\/cleanup\.mjs --production/);
  assert.match(job, /path: actions-production-artifacts\//);
  const imageUpload = job.split("- name: Retain production candidates and sanitized evidence")[1];
  assert.doesNotMatch(imageUpload, /path:.*(?:control|fixture|cookie|storageState)/);
  assert.match(job, /retention-days: 3/);
});

test("download validation precedes execute-mode restoration and fixed contained launch", async () => {
  const source = await readFile(new URL("../../devtools/actions-visual/container.mjs", import.meta.url), "utf8");
  assert.ok(source.indexOf("await verifyProductionArtifact") < source.indexOf("await chmod"));
  assert.match(source, /chmod\(fixture\.binary, 0o500\)/);
  const exporter = await readFile(new URL("../../devtools/actions-visual/export-production.mjs", import.meta.url), "utf8");
  assert.match(exporter, /"nextest", "list", "--workspace", "--locked", "--list-type", "binaries-only", "--message-format", "json"/);
  assert.doesNotMatch(exporter, /--release|--features|cargo.*build/);
});
