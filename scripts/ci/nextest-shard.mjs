#!/usr/bin/env node
// Runs one CI test shard from a prebuilt nextest archive, and records its failed tests.
//
//   node scripts/ci/nextest-shard.mjs run     run the shard, or only the tests that
//                                             failed in the previous attempt
//   node scripts/ci/nextest-shard.mjs record  write this attempt's failed-test list
//
// Inputs come from the environment (never from the shell command line):
//   TEST_OS, TEST_SHARD ("2/4"), RUN_ATTEMPT   which shard and attempt this is
//   NEXTEST_ARCHIVE                            the archive built by the build job
//   PREVIOUS_FAILURES                          directory holding earlier attempts' lists
//   SHARD_STATUS                               where `run` leaves nextest's exit code
//   TEST_OUTCOME, FAILURES_OUTPUT              for `record`
//
// Test names never pass through a shell: the filter is handed to nextest as a single
// argv entry, and every character outside [A-Za-z0-9_.:-] in it is a \u{…} escape.
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { failureRecord, parseAttempt, parseShard, resumePlan } from "./nextest-failures.mjs";

// The archive is extracted into the workspace root, so test binaries land in ./target
// at the same absolute paths they were built with (see docs/ci-test-shards.md).
const JUNIT = join("target", "nextest", "ci", "junit.xml");

function required(name) {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is not set`);
  return value;
}

function identity() {
  const os = required("TEST_OS");
  if (!/^[a-z0-9.-]+$/.test(os)) throw new Error("TEST_OS is not a runner label");
  const shard = required("TEST_SHARD");
  parseShard(shard);
  return { os, shard, attempt: parseAttempt(required("RUN_ATTEMPT")) };
}

function readRecords(directory) {
  if (!directory || !existsSync(directory)) return [];
  const records = [];
  for (const name of readdirSync(directory)) {
    if (!name.endsWith(".json")) continue;
    try { records.push(JSON.parse(readFileSync(join(directory, name), "utf8"))); }
    catch { console.log(`Ignoring unreadable failed-test list ${JSON.stringify(name)}.`); }
  }
  return records;
}

function archiveArgs(archive) {
  return ["--archive-file", archive, "--workspace-remap", ".", "--extract-to", ".", "--extract-overwrite"];
}

// Confirms the filter selects exactly the recorded tests before trusting it, so a
// renamed or mis-encoded test can never be skipped silently.
function filterSelectsExactly(archive, plan) {
  const listed = spawnSync("cargo", ["nextest", "list", ...archiveArgs(archive), "--profile", "ci",
    "--message-format", "json", "-E", plan.filter], { encoding: "utf8", maxBuffer: 256 * 1024 * 1024, stdio: ["ignore", "pipe", "inherit"] });
  if (listed.status !== 0) return false;
  let selected;
  try {
    selected = Object.values(JSON.parse(listed.stdout)["rust-suites"] ?? {}).flatMap((suite) =>
      Object.entries(suite.testcases ?? {}).filter(([, test]) => test["filter-match"]?.status === "matches")
        .map(([name]) => JSON.stringify([suite["binary-id"], name])));
  } catch { return false; }
  const expected = plan.tests.map(({ binaryId, name }) => JSON.stringify([binaryId, name]));
  return selected.length === expected.length && new Set(selected).size === selected.length && expected.every((key) => selected.includes(key));
}

function run() {
  const { os, shard, attempt } = identity();
  const archive = required("NEXTEST_ARCHIVE");
  const status = required("SHARD_STATUS");
  let plan = resumePlan(readRecords(process.env.PREVIOUS_FAILURES), { os, shard, attempt });
  if (plan.mode === "resume" && !filterSelectsExactly(archive, plan)) {
    plan = { mode: "full", reason: "the recorded failed tests could not be selected exactly" };
  }
  const { index, count } = parseShard(shard);
  const selection = plan.mode === "resume" ? ["-E", plan.filter] : ["--partition", `hash:${index}/${count}`];
  if (plan.mode === "resume") {
    console.log(`Attempt ${attempt}: re-running ${plan.reason} instead of the whole shard ${shard}.`);
    for (const test of plan.tests) console.log(`  - ${JSON.stringify(test.binaryId)} ${JSON.stringify(test.name)}`);
  } else {
    console.log(`Attempt ${attempt}: running shard ${shard} in full (${plan.reason}).`);
  }
  const result = spawnSync("cargo", ["nextest", "run", ...archiveArgs(archive), "--profile", "ci", ...selection], { stdio: "inherit" });
  const exitCode = result.status ?? null;
  writeFileSync(status, `${JSON.stringify({ exitCode, mode: plan.mode })}\n`);
  if (result.error) throw result.error;
  process.exitCode = exitCode ?? 1;
}

function record() {
  const { os, shard, attempt } = identity();
  const output = required("FAILURES_OUTPUT");
  let exitCode = null;
  try {
    const status = JSON.parse(readFileSync(required("SHARD_STATUS"), "utf8"));
    if (Number.isInteger(status.exitCode)) exitCode = status.exitCode;
  } catch { /* nextest never finished: the record stays untrusted */ }
  const junit = existsSync(JUNIT) ? readFileSync(JUNIT, "utf8") : null;
  const result = failureRecord({ os, shard, attempt, outcome: process.env.TEST_OUTCOME ?? "", exitCode, junit });
  // The upload step collects this whole directory, and no earlier step creates it.
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, `${JSON.stringify(result, null, 2)}\n`);
  console.log(`Shard ${shard}, attempt ${attempt}: ${result.trusted ? "" : "incomplete, "}${result.reason}.`);
}

try {
  const command = process.argv[2];
  if (command === "run" && process.argv.length === 3) run();
  else if (command === "record" && process.argv.length === 3) record();
  else throw new Error("Usage: nextest-shard.mjs run|record");
} catch (error) {
  console.error(`::error title=Test shard::${String(error.message).replace(/[\r\n%]/g, " ")}`);
  process.exitCode = 1;
}
