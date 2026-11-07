import assert from "node:assert/strict";
import test from "node:test";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { REQUIRED_CHECKS, evaluateResults, summaryMarkdown, annotations } from "../ci/required-results.mjs";

const success = () => Object.fromEntries(REQUIRED_CHECKS.map(({ id }) => [id, { result: "success", outputs: {} }]));
const metadata = { GITHUB_SHA: "a".repeat(40), GITHUB_REPOSITORY: "GitRbit/rbit", GITHUB_SERVER_URL: "https://github.com", GITHUB_RUN_ID: "123", GITHUB_RUN_ATTEMPT: "2" };

test("only complete successful dependency evidence passes the required gate", () => {
  const all = success(); assert.equal(evaluateResults(JSON.stringify(all)).passed, true);
  for (const { id } of REQUIRED_CHECKS) for (const result of ["failure", "cancelled", "skipped", "neutral", "", null]) {
    const report = evaluateResults(JSON.stringify({ ...all, [id]: { result } }));
    assert.equal(report.passed, false); assert.equal(annotations(report).length, 1);
    assert.ok(annotations(report)[0].includes(id));
  }
  for (const { id } of REQUIRED_CHECKS) { const missing = success(); delete missing[id]; assert.equal(evaluateResults(JSON.stringify(missing)).passed, false); }
});

test("asset failure plus skipped release retains both blocking reasons", () => {
  const report = evaluateResults(JSON.stringify({ ...success(), assets: { result: "failure" }, release: { result: "skipped" } }));
  const summary = summaryMarkdown(report, metadata);
  assert.equal(report.passed, false); assert.equal(annotations(report).length, 2);
  assert.match(summary, /Front-end build and tests.*Failed/); assert.match(summary, /Release binaries and version smoke.*Skipped/);
  assert.match(summary, /5\/7/); assert.match(summary, /Blocked/);
});

test("malformed, missing and unexpected dependency metadata fails without reflecting payloads", () => {
  for (const value of [undefined, "not json", "null", "[]", "42", '"text"', JSON.stringify({ ...success(), unexpected: { result: "success", outputs: { secret: "::warning::injection" } } })]) {
    const report = evaluateResults(value); assert.equal(report.passed, false);
    assert.ok(report.inputIssue); assert.ok(!summaryMarkdown(report, metadata).includes("injection"));
  }
});

test("summary shows validated revision/run/artifact context and discards untrusted fields", () => {
  const all = success(); all.test.outputs = { markup: "<script>secret</script>\n::error::injection" };
  const report = evaluateResults(JSON.stringify(all)); const summary = summaryMarkdown(report, metadata);
  assert.match(summary, /Ready/); assert.match(summary, /7\/7/); assert.match(summary, /commit\/a{40}/); assert.match(summary, /actions\/runs\/123/);
  assert.match(summary, /attempt 2/); assert.match(summary, /app-css/); assert.match(summary, /rbit-linux-x86_64/);
  assert.match(summary, /combined job result/); assert.match(summary, /does not infer unreported measurements/);
  assert.ok(!summary.includes("secret")); assert.deepEqual(annotations(report), []);
  const untrusted = Object.fromEntries(Object.keys(metadata).map((key) => [key, "bad\n::error::injection"]));
  assert.ok(!summaryMarkdown(report, untrusted).includes("injection"));
});

test("actual CLI publishes summaries and returns failure for skipped, invalid or unwritable evidence", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "rbit-ci-summary-")); t.after(() => rm(root, { recursive: true, force: true }));
  const output = join(root, "summary.md"); await writeFile(output, "");
  const cli = fileURLToPath(new URL("../ci/required-check.mjs", import.meta.url));
  const env = { ...process.env, ...metadata, CI_NEEDS: JSON.stringify(success()), GITHUB_STEP_SUMMARY: output };
  const passed = execFileSync(process.execPath, [cli], { env, encoding: "utf8", windowsHide: true }); assert.match(passed, /passed \(7\/7\)/);
  assert.match(await readFile(output, "utf8"), /CI validation/);
  env.CI_NEEDS = JSON.stringify({ ...success(), "runner-image": { result: "skipped" } });
  const skipped = spawnSync(process.execPath, [cli], { env, encoding: "utf8", windowsHide: true });
  assert.equal(skipped.status, 1); assert.match(skipped.stdout, /::error title=CI required - runner-image/);
  env.CI_NEEDS = "invalid"; assert.equal(spawnSync(process.execPath, [cli], { env, windowsHide: true }).status, 1);
  env.CI_NEEDS = JSON.stringify(success()); env.GITHUB_STEP_SUMMARY = root;
  const unwritable = spawnSync(process.execPath, [cli], { env, encoding: "utf8", windowsHide: true });
  assert.equal(unwritable.status, 1); assert.match(unwritable.stderr, /Unable to publish/);
});

test("workflow gate covers every validation, preserves platform/smoke checks and stays pinned/read-only", async () => {
  const workflow = await readFile(new URL("../../.github/workflows/ci.yml", import.meta.url), "utf8");
  // This workflow deliberately uses direct, two-space job definitions and inline
  // needs. Refuse structural drift instead of pretending to be a general YAML parser.
  const jobs = [...workflow.slice(workflow.indexOf("\njobs:")).matchAll(/^  ([a-z][a-z0-9-]*):\r?$/gm)].map((match) => match[1]);
  assert.deepEqual(jobs.filter((id) => id !== "required").sort(),
    REQUIRED_CHECKS.map((check) => check.id).sort());
  assert.equal(jobs.includes("actions-production-visual"), false);
  const gate = workflow.slice(workflow.indexOf("\n  required:"));
  const needs = gate.match(/^    needs: \[([^\]]+)\]/m)[1].split(",").map((id) => id.trim());
  assert.deepEqual(needs.sort(), REQUIRED_CHECKS.map((check) => check.id).sort());
  assert.match(gate, /name: CI required/); assert.ok(gate.includes("if: ${{ always() }}"));
  assert.ok(gate.includes("CI_NEEDS: ${{ toJSON(needs) }}"));
  assert.match(gate, /permissions:\r?\n      contents: read/); assert.match(gate, /persist-credentials: false/);
  assert.ok(!workflow.includes("continue-on-error:")); assert.ok(!workflow.includes("pull_request_target:"));
  assert.ok(!gate.includes("GH_TOKEN")); assert.doesNotMatch(gate, /:\s*(write|write-all)\s*$/m);
  for (const match of workflow.matchAll(/uses: ([^\s#]+)/g)) if (!match[1].startsWith("./")) assert.match(match[1], /@[a-f0-9]{40}$/);
  assert.equal([...workflow.matchAll(/os: \[ubuntu-latest, windows-latest\]/g)].length, 2);
  for (const command of ["docker build --file Dockerfile.runner", "docker run --rm rbit-runner:ci --version", "--entrypoint git rbit-runner:ci --version", "--entrypoint docker rbit-runner:ci --version"]) assert.ok(workflow.includes(command));
});
