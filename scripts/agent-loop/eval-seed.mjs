#!/usr/bin/env node
// Seeds a long-lived candidate for agent evals: boots it (see instance.mjs), pushes one
// branch per eval case, waits until every case has reached its seeded state, writes
// `cases.json` (the questions) and `truth.json` (the answers, for the grader only) into the
// work directory, and keeps the instance running until it is stopped (SIGTERM or Ctrl-C).
//
//   node scripts/agent-loop/eval-seed.mjs --rbit PATH --runner PATH --work DIR [--seed S]
//
// The agent under test gets `cases.json` (base URL and questions) and the write token (in
// `<work>/token`, mode 0600). `truth.json` holds the seed and the answers: move it away from
// anything the agent can read before an eval starts.
import { randomBytes } from "node:crypto";
import { writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { generateCases } from "./eval-cases.mjs";
import { FixtureRepo } from "./fixtures.mjs";
import { startCandidate } from "./instance.mjs";
import { findRun, waitForRun } from "./recipes.mjs";

function parse(argv) {
  const options = { rbit: "", runner: "", work: "", seed: "" };
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i].replace(/^--/, "");
    if (!Object.hasOwn(options, key) || argv[i + 1] === undefined) throw new Error(`unknown argument ${argv[i]}`);
    options[key] = argv[i + 1];
  }
  if (!options.rbit || !options.runner || !options.work) throw new Error("usage: eval-seed.mjs --rbit PATH --runner PATH --work DIR");
  return options;
}

async function seed(candidate, work, seedValue) {
  const repo = `${candidate.username}/calc`;
  await candidate.api.post("/user/repos", { name: "calc", private: true });
  const fixture = new FixtureRepo(join(work, "fixture"), { remote: `${candidate.base}/${repo}.git`, token: candidate.token });
  fixture.init();
  fixture.push("main");
  const opaque = () => randomBytes(4).toString("hex");
  const questions = [];
  const truths = {};
  for (const item of generateCases(seedValue)) {
    const branch = `work/${opaque()}`;
    fixture.branch(branch);
    const changed = item.files((path) => fixture.read(path));
    fixture.write(changed);
    const sha = fixture.commit("Update");
    fixture.push(branch);
    truths[item.id] = { ...item.truth(changed), kind: item.kind, sha, branch };
    questions.push({ id: item.id, repo, branch, sha });
    const run = await findRun(candidate.api, repo, { branch, sha, deadlineMs: 60_000, pollMs: 1_000 });
    if (!run) throw new Error(`no run appeared for ${item.id}`);
    if (truths[item.id].verdict !== "pending") {
      const done = await waitForRun(candidate.api, repo, run.id, { deadlineMs: 300_000, pollMs: 2_000 });
      if (done.conclusion !== truths[item.id].verdict) throw new Error(`${item.id} concluded ${done.conclusion}, seeded as ${truths[item.id].verdict}`);
    }
    if (item.chaser) {
      // A newer, green run on another branch: the repository's newest run is not this one.
      const other = `work/${opaque()}`;
      fixture.branch(other);
      fixture.write({ "src/calc.sh": `${fixture.read("src/calc.sh")}# ${opaque()}\n` });
      const otherSha = fixture.commit("Update");
      fixture.push(other);
      const otherRun = await findRun(candidate.api, repo, { branch: other, sha: otherSha, deadlineMs: 60_000, pollMs: 1_000 });
      if (otherRun) await waitForRun(candidate.api, repo, otherRun.id, { deadlineMs: 300_000, pollMs: 2_000 });
    }
  }
  return { questions, truths };
}

async function main() {
  const options = parse(process.argv.slice(2));
  const work = resolve(options.work);
  const candidate = await startCandidate({ rbit: resolve(options.rbit), runner: resolve(options.runner), workDir: join(work, "instance") });
  try {
    const seedValue = options.seed || randomBytes(8).toString("hex");
    const { questions, truths } = await seed(candidate, work, seedValue);
    writeFileSync(join(work, "cases.json"), `${JSON.stringify({ base: candidate.base, cases: questions }, null, 2)}\n`);
    writeFileSync(join(work, "truth.json"), `${JSON.stringify({ seed: seedValue, seededAt: Date.now(), cases: truths }, null, 2)}\n`, { mode: 0o600 });
    writeFileSync(join(work, "token"), `${candidate.token}\n`, { mode: 0o600 });
    process.stderr.write(`seeded ${questions.length} cases at ${candidate.base}; running until stopped\n`);
  } catch (error) {
    await candidate.stop({ keep: true });
    throw error;
  }
  const shutdown = async () => {
    await candidate.stop({ keep: true });
    process.exit(0);
  };
  process.on("SIGTERM", shutdown);
  process.on("SIGINT", shutdown);
  await new Promise(() => {});
}

main().catch((error) => {
  process.stderr.write(`${error.stack ?? error}\n`);
  process.exitCode = 1;
});
