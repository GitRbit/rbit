#!/usr/bin/env node
// The agent-loop benchmark and candidate check (docs/design/agent-step0-ci-and-candidate.md).
// Boots a throwaway rbit from the binaries under test, pushes a generated dummy project,
// runs the scenarios, and writes one JSON report. It exits 1 when a "correct" scenario is
// answered wrong or the candidate cannot be started, so a broken build fails here.
//
//   node scripts/agent-loop/bench.mjs --rbit target/release/rbit --runner target/release/rbit-runner \
//     [--work target/agent-loop] [--report target/agent-loop-report.json] [--only a,b] [--quick]
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { Meter } from "./api.mjs";
import { FixtureRepo } from "./fixtures.mjs";
import { startCandidate, tail } from "./instance.mjs";
import { SCENARIOS } from "./scenarios.mjs";

/** Scenarios that finish in about a minute together: the candidate check of every commit. */
export const QUICK = ["green", "compile_error", "test_failure", "status_after_push"];

export function parseArgs(argv) {
  const options = { rbit: "", runner: "", work: "target/agent-loop", report: "target/agent-loop-report.json", only: "", quick: false };
  for (let i = 0; i < argv.length; i += 1) {
    const key = argv[i].replace(/^--/, "");
    if (key === "quick") options.quick = true;
    else if (Object.hasOwn(options, key) && argv[i + 1] !== undefined) options[key] = argv[++i];
    else throw new Error(`unknown argument ${argv[i]}`);
  }
  if (!options.rbit || !options.runner) throw new Error("usage: bench.mjs --rbit PATH --runner PATH [--work DIR] [--report FILE] [--only a,b] [--quick]");
  return options;
}

/** The scenarios to run, in their defined order. */
export function selectScenarios({ only, quick }) {
  const names = only ? only.split(",") : quick ? QUICK : SCENARIOS.map((scenario) => scenario.name);
  const unknown = names.filter((name) => !SCENARIOS.some((scenario) => scenario.name === name));
  if (unknown.length) throw new Error(`unknown scenarios: ${unknown.join(", ")}`);
  return SCENARIOS.filter((scenario) => names.includes(scenario.name));
}

/** Folds scenario results into the report's summary and the process's exit decision. */
export function summarize(results) {
  const wrong = results.filter((result) => result.gate === "correct" && !result.correct).map((result) => result.name);
  const fixed = results.filter((result) => result.gate === "known_wrong" && result.correct).map((result) => result.name);
  const knownWrong = results.filter((result) => result.gate === "known_wrong" && !result.correct).map((result) => result.name);
  const errors = results.filter((result) => result.error).map((result) => result.name);
  const failures = results.filter((result) => result.firstFailureMs != null);
  return {
    pass: wrong.length === 0 && errors.length === 0,
    wrongAnswers: wrong, errors, knownWrong, fixed,
    maxBytesToFailureLine: Math.max(0, ...failures.map((result) => result.cost.bytes)),
    maxCallsToFailureLine: Math.max(0, ...failures.map((result) => result.cost.calls)),
  };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scenarios = selectScenarios(options);
  const work = resolve(options.work);
  const started = Date.now();
  const candidate = await startCandidate({ rbit: resolve(options.rbit), runner: resolve(options.runner), workDir: join(work, "agent-loop-candidate") });
  const results = [];
  try {
    const repo = `${candidate.username}/calc`;
    await candidate.api.post("/user/repos", { name: "calc", private: true }, { label: "setup" });
    const fixture = new FixtureRepo(join(work, "fixture"), { remote: `${candidate.base}/${repo}.git`, token: candidate.token });
    fixture.init();
    fixture.push("main");
    const ctx = { api: candidate.api, repo, fixture, pollMs: 3_000, noRunWaitMs: 30_000 };
    for (const scenario of scenarios) {
      candidate.api.meter = new Meter();
      const began = Date.now();
      process.stderr.write(`scenario ${scenario.name} ... `);
      let result;
      try {
        result = await scenario.run(ctx);
      } catch (error) {
        result = { correct: false, error: error.message };
      }
      const entry = { name: scenario.name, gate: scenario.gate, about: scenario.about, ...result, cost: candidate.api.meter.snapshot(), wallMs: Date.now() - began };
      results.push(entry);
      process.stderr.write(`${entry.error ? `error: ${entry.error}` : entry.correct ? "right" : "wrong"} (${entry.cost.calls} calls, ${entry.cost.bytes} bytes)\n`);
    }
  } catch (error) {
    process.stderr.write(`candidate setup failed: ${error.message}\n${tail(join(work, "agent-loop-candidate", "logs", "server.log"))}\n`);
    process.exitCode = 1;
  } finally {
    await candidate.stop({ keep: process.exitCode === 1 });
  }
  const report = { schema: "rbit.agent-loop-report/1", startedAt: new Date(started).toISOString(), wallMs: Date.now() - started, summary: summarize(results), results };
  mkdirSync(dirname(resolve(options.report)), { recursive: true });
  writeFileSync(resolve(options.report), `${JSON.stringify(report, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify(report.summary)}\n`);
  if (!report.summary.pass || results.length === 0) process.exitCode = 1;
}

if (import.meta.url === `file://${process.argv[1]}` || process.argv[1]?.endsWith("bench.mjs")) {
  main().catch((error) => {
    process.stderr.write(`${error.stack ?? error}\n`);
    process.exitCode = 1;
  });
}
