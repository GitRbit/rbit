#!/usr/bin/env node
// Grades an agent's answers against a seeded candidate's truth and meter.
//
//   node scripts/agent-loop/eval-grade.mjs --work DIR --truth FILE --answers FILE [--label NAME]
//   node scripts/agent-loop/eval-grade.mjs --stdin   < {"truth":…,"answers":…,"meter":…}
//
// The truth file is the one eval-seed.mjs wrote, kept where the agent cannot read it.
// FILE holds `{ "<case id>": {"verdict": …, "path": …, "line": …}, … }`. Prints one JSON
// object: per case whether the answer was right, and the calls and bytes it cost (from the
// meter eval-api.mjs writes); then accuracy and cost for the train and test splits.
// With files, it clears the meter afterwards so the next trial starts from zero.
import { existsSync, readFileSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { gradeAnswer, splitCases } from "./eval-cases.mjs";

const MAX_AGE_MS = 20 * 60 * 60 * 1000;

function parse(argv) {
  const options = { work: "", truth: "", answers: "", label: "", stdin: false };
  for (let i = 0; i < argv.length; i += 1) {
    const key = argv[i].replace(/^--/, "");
    if (key === "stdin") options.stdin = true;
    else if (Object.hasOwn(options, key) && argv[i + 1] !== undefined) options[key] = argv[++i];
    else throw new Error(`unknown argument ${argv[i]}`);
  }
  if (!options.stdin && (!options.work || !options.truth || !options.answers)) throw new Error(USAGE);
  return options;
}

const USAGE = "usage: eval-grade.mjs --work DIR --truth FILE --answers FILE [--label NAME] | --stdin";

/** The calls and bytes recorded for one case. */
export function meterOf(work, id) {
  const file = join(work, "meter", `${id}.jsonl`);
  if (!existsSync(file)) return { calls: 0, bytes: 0 };
  const rows = readFileSync(file, "utf8").split("\n").filter(Boolean).map((line) => JSON.parse(line));
  return { calls: rows.length, bytes: rows.reduce((sum, row) => sum + row.bytes, 0) };
}

/** Accuracy and mean cost over `rows`. */
export function aggregate(rows) {
  if (!rows.length) return { cases: 0, correct: 0, accuracy: 0, meanBytes: 0, meanCalls: 0 };
  const correct = rows.filter((row) => row.correct).length;
  const mean = (key) => Math.round(rows.reduce((sum, row) => sum + row[key], 0) / rows.length);
  return { cases: rows.length, correct, accuracy: Math.round((correct / rows.length) * 1000) / 1000, meanBytes: mean("bytes"), meanCalls: mean("calls") };
}

/**
 * Grades every case. `truth` is what eval-seed.mjs wrote, `answers` maps case id to an
 * answer, `meter` maps case id to `{calls, bytes}`, `now` is the grading time.
 */
export function gradeAll({ truth, answers, meter, label = "", now = Date.now() }) {
  const { seededAt, cases: truths } = truth;
  // A queued run is cancelled after a day, which would change the truth under the agent.
  if (now - seededAt > MAX_AGE_MS) throw new Error("the seeded candidate is over 20 hours old: seed it again");
  const ids = Object.keys(truths);
  const rows = ids.map((id) => ({ id, kind: truths[id].kind, ...gradeAnswer(answers[id], truths[id]), ...(meter[id] ?? { calls: 0, bytes: 0 }) }));
  const { train, test } = splitCases(Object.fromEntries(ids.map((id) => [id, truths[id].kind])));
  return {
    schema: "rbit.agent-eval/1", label,
    train: aggregate(rows.filter((row) => train.includes(row.id))),
    test: aggregate(rows.filter((row) => test.includes(row.id))),
    cases: rows,
  };
}

async function main() {
  const options = parse(process.argv.slice(2));
  let result;
  if (options.stdin) {
    // One JSON bundle {truth, answers, meter, label}: the truth never has to be stored where
    // the grader runs.
    const chunks = [];
    for await (const chunk of process.stdin) chunks.push(chunk);
    result = gradeAll(JSON.parse(Buffer.concat(chunks).toString("utf8")));
  } else {
    const work = resolve(options.work);
    const truth = JSON.parse(readFileSync(resolve(options.truth), "utf8"));
    const answers = JSON.parse(readFileSync(resolve(options.answers), "utf8"));
    const meter = Object.fromEntries(Object.keys(truth.cases).map((id) => [id, meterOf(work, id)]));
    result = gradeAll({ truth, answers, meter, label: options.label });
    rmSync(join(work, "meter"), { recursive: true, force: true });
  }
  process.stdout.write(`${JSON.stringify(result)}
`);
}

if (process.argv[1]?.endsWith("eval-grade.mjs")) {
  main().catch((error) => {
    process.stderr.write(`${error.message}
`);
    process.exitCode = 2;
  });
}
