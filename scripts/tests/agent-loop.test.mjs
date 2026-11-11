// Unit tests for the agent-loop benchmark's pure parts: log reading, grading, scenario
// selection and the candidate configuration. The benchmark itself needs built binaries and
// runs in CI's candidate job.
import assert from "node:assert/strict";
import test from "node:test";
import { Meter, attributeAfter } from "../agent-loop/api.mjs";
import { QUICK, parseArgs, selectScenarios, summarize } from "../agent-loop/bench.mjs";
import { baseFiles, lineOf, workflow } from "../agent-loop/fixtures.mjs";
import { RUNNER_LABEL, candidateConfig } from "../agent-loop/instance.mjs";
import { firstFailure } from "../agent-loop/recipes.mjs";
import { SCENARIOS } from "../agent-loop/scenarios.mjs";

test("an annotation wins over the plain lines around it", () => {
  const log = [
    "2026-11-11T10:00:00.000Z \x1b[32mstarting\x1b[0m",
    "2026-11-11T10:00:01.000Z src/calc.sh:5:10: error: unknown token '@@'",
    "2026-11-11T10:00:01.000Z ::error file=src/calc.sh,line=5,col=10::unknown token '@@'",
    "2026-11-11T10:00:02.000Z ##[error]Process completed with exit code 1.",
  ].join("\n");
  assert.deepEqual(firstFailure(log), { path: "src/calc.sh", line: 5, message: "unknown token '@@'" });
});

test("a failing test line is found after thousands of passing ones", () => {
  const noise = Array.from({ length: 3000 }, (_, i) => `test noise::case_${i} ... ok`);
  const log = [...noise, "FAIL cubes_numbers (tests/cases.txt:6): expected 28, got 27", "##[error]exit 1"].join("\r\n");
  assert.deepEqual(firstFailure(log), { path: "tests/cases.txt", line: 6, message: "cubes_numbers: expected 28, got 27" });
});

test("a log with only an error marker gives a message without a location", () => {
  assert.deepEqual(firstFailure("plain\n##[error]The job exceeded its time limit"), { path: null, line: null, message: "The job exceeded its time limit" });
  assert.equal(firstFailure("all good\n"), null);
});

test("the meter counts calls and bytes per label", () => {
  const meter = new Meter();
  meter.record("runs", 100);
  meter.record("runs", 50);
  meter.record("log", 4000);
  assert.deepEqual(meter.snapshot(), { calls: 3, bytes: 4150, byLabel: { runs: { calls: 2, bytes: 150 }, log: { calls: 1, bytes: 4000 } } });
});

test("the created token is read from its input", () => {
  const html = '<p>Copy it now</p><input class="x" id="new-token" readonly value="rbit_abc123">';
  assert.equal(attributeAfter(html, 'id="new-token"', "value"), "rbit_abc123");
  assert.equal(attributeAfter("<p>none</p>", 'id="new-token"', "value"), null);
});

test("the fixture's workflow asks only for the candidate runner's label", () => {
  assert.match(workflow(), new RegExp(`runs-on: ${RUNNER_LABEL}\\n`));
  assert.match(workflow({ timeoutMinutes: 1 }), /timeout-minutes: 1\n/);
  assert.doesNotMatch(workflow(), /ubuntu-latest|self-hosted/);
  const files = baseFiles();
  assert.ok(files[".rbit/workflows/ci.yml"] && files["tests/run.sh"] && files["tools/lint.sh"]);
  assert.equal(lineOf(files["tests/cases.txt"], "cube 3 0 27"), 6);
  assert.throws(() => lineOf("a\nb", "zzz"));
});

test("the candidate configuration binds loopback and turns Actions on", () => {
  const config = candidateConfig({ port: 4321, dataDir: "/tmp/x", setupToken: "t" });
  assert.match(config, /bind = "127\.0\.0\.1:4321"/);
  assert.match(config, /\[actions\]\nenabled = true/);
  assert.match(config, /check_for_updates = false/);
  assert.match(config, /rate_limit = false/);
});

test("scenarios are selected by name, by the quick set, or all", () => {
  assert.deepEqual(selectScenarios({ only: "", quick: true }).map((s) => s.name), QUICK);
  assert.equal(selectScenarios({ only: "", quick: false }).length, SCENARIOS.length);
  assert.deepEqual(selectScenarios({ only: "green,docs_only", quick: false }).map((s) => s.name), ["green", "docs_only"]);
  assert.throws(() => selectScenarios({ only: "nope", quick: false }), /unknown scenarios: nope/);
  assert.ok(SCENARIOS.every((s) => ["correct", "known_wrong"].includes(s.gate) && s.about));
});

test("arguments need both binaries", () => {
  assert.throws(() => parseArgs(["--rbit", "a"]), /usage/);
  assert.deepEqual(parseArgs(["--rbit", "a", "--runner", "b", "--quick"]).quick, true);
  assert.throws(() => parseArgs(["--rbit", "a", "--runner", "b", "--bogus", "1"]), /unknown argument/);
});

test("only a wrong 'correct' scenario or an error fails the run; fixed traps are reported", () => {
  const cost = { calls: 3, bytes: 900 };
  const ok = summarize([
    { name: "green", gate: "correct", correct: true, cost },
    { name: "test_failure", gate: "correct", correct: true, cost, firstFailureMs: 5000 },
    { name: "docs_only", gate: "known_wrong", correct: false, cost },
    { name: "timeout", gate: "known_wrong", correct: true, cost },
  ]);
  assert.equal(ok.pass, true);
  assert.deepEqual(ok.knownWrong, ["docs_only"]);
  assert.deepEqual(ok.fixed, ["timeout"]);
  assert.equal(ok.maxBytesToFailureLine, 900);
  const bad = summarize([{ name: "green", gate: "correct", correct: false, cost }, { name: "x", gate: "known_wrong", correct: false, error: "boom", cost }]);
  assert.equal(bad.pass, false);
  assert.deepEqual(bad.wrongAnswers, ["green"]);
  assert.deepEqual(bad.errors, ["x"]);
});

test("eval cases are generated from a seed, opaque and complete", async () => {
  const { MIX, generateCases } = await import("../agent-loop/eval-cases.mjs");
  const a = generateCases("seed-1");
  const b = generateCases("seed-1");
  const total = Object.values(MIX).reduce((sum, n) => sum + n, 0);
  assert.equal(a.length, total);
  assert.deepEqual(a.map((c) => c.kind), b.map((c) => c.kind));
  assert.notDeepEqual(a.map((c) => c.kind), generateCases("seed-2").map((c) => c.kind));
  assert.ok(a.every((c, i) => c.id === `c${String(i + 1).padStart(2, "0")}`));
  const files = baseFiles();
  for (const item of a) {
    const changed = item.files((path) => files[path]);
    const truth = item.truth(changed);
    if (truth.verdict === "failure") assert.ok(Number.isInteger(truth.line) && truth.line > 0, `${item.kind} has a line`);
  }
});

test("eval answers are normalised before grading", async () => {
  const { gradeAnswer } = await import("../agent-loop/eval-cases.mjs");
  const truth = { verdict: "failure", path: "src/calc.sh", line: 4, also: [{ path: "tests/cases.txt", line: 2 }] };
  assert.equal(gradeAnswer({ verdict: "failure", path: "./src/calc.sh", line: "4" }, truth).correct, true);
  assert.equal(gradeAnswer({ verdict: "Failure", path: "src/calc.sh:4:9" }, truth).correct, true);
  assert.equal(gradeAnswer({ verdict: "failure", path: "tests/cases.txt", line: 2 }, truth).correct, true);
  assert.equal(gradeAnswer({ verdict: "failure", path: "src/calc.sh", line: 5 }, truth).correct, false);
  assert.equal(gradeAnswer({ verdict: "success" }, truth).correct, false);
  assert.equal(gradeAnswer(undefined, truth).correct, false);
  assert.equal(gradeAnswer({ verdict: "pending" }, { verdict: "pending" }).correct, true);
});

test("the eval split holds out about a third of every kind", async () => {
  const { splitCases } = await import("../agent-loop/eval-cases.mjs");
  const kinds = { c01: "green", c02: "green", c03: "green", c04: "lint", c05: "lint", c06: "lint", c07: "lint", c08: "pending" };
  const { train, test } = splitCases(kinds);
  assert.deepEqual(test, ["c03", "c06"]);
  assert.equal(train.length + test.length, 8);
});
