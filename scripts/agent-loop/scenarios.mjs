// The benchmark's scenarios: each changes the fixture in a known way, pushes, asks the
// recipe what happened, and grades the answer against what is known to be true.
//
// `gate: "correct"` scenarios must be answered right today; a wrong answer fails the run.
// `gate: "known_wrong"` scenarios record a trap that today's rbit falls into (the agent
// plan's section 3.2). They are expected to be wrong until the fix lands; when one turns
// right it is reported as fixed, so the step that fixes it can move it to "correct".
import { baselineRecipe, combinedStatus, findRun, newestRunOfRepository, waitForRun } from "./recipes.mjs";
import { lineOf, workflow } from "./fixtures.mjs";

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

/** Pushes the current commit of a fresh branch and returns its commit and the push time. */
function pushChange(ctx, branch, files, message) {
  ctx.fixture.branch(branch);
  ctx.fixture.write(files);
  const sha = ctx.fixture.commit(message);
  ctx.fixture.push(branch);
  return { sha, pushedAt: Date.now() };
}

function sameLocation(found, expected) {
  return Boolean(found) && found.path === expected.path && found.line === expected.line;
}

function graded(answer, expected, correct, extra = {}) {
  return { answer, expected, correct, ...extra };
}

async function failureScenario(ctx, branch, files, expected) {
  const { sha, pushedAt } = pushChange(ctx, branch, files, `Seed ${branch}`);
  const answer = await baselineRecipe(ctx.api, ctx.repo, { branch, sha, pollMs: ctx.pollMs });
  const correct = answer.verdict === "failure" && answer.sha === sha && sameLocation(answer.failure, expected);
  const firstFailureMs = answer.firstFailureAt ? answer.firstFailureAt - pushedAt : null;
  return graded(answer, { verdict: "failure", ...expected }, correct, { firstFailureMs });
}

export const SCENARIOS = [
  {
    name: "green", gate: "correct", about: "A harmless change passes and is reported as a pass for that commit",
    async run(ctx) {
      const text = ctx.fixture.read("src/calc.sh");
      const { sha } = pushChange(ctx, "s/green", { "src/calc.sh": `${text}# harmless\n` }, "Harmless change");
      const answer = await baselineRecipe(ctx.api, ctx.repo, { branch: "s/green", sha, pollMs: ctx.pollMs });
      return graded(answer, { verdict: "success" }, answer.verdict === "success" && answer.sha === sha);
    },
  },
  {
    name: "compile_error", gate: "correct", about: "A lint error with a file, line and column",
    async run(ctx) {
      const text = ctx.fixture.read("src/calc.sh").replace("mul() {", "mul() { @@");
      return failureScenario(ctx, "s/compile-error", { "src/calc.sh": text }, { path: "src/calc.sh", line: lineOf(text, "@@") });
    },
  },
  {
    name: "test_failure", gate: "correct", about: "One failing test after thousands of lines of passing output",
    async run(ctx) {
      const text = ctx.fixture.read("tests/cases.txt").replace("cube 3 0 27", "cube 3 0 28");
      return failureScenario(ctx, "s/test-failure", { "tests/cases.txt": text }, { path: "tests/cases.txt", line: lineOf(text, "cube 3 0 28") });
    },
  },
  {
    name: "flaky", gate: "known_wrong", about: "A test fails once and passes on a re-run of the same commit; rbit does not label it",
    async run(ctx) {
      const first = await failureScenario(ctx, "s/flaky", { "tests/flaky": "1\n" }, { path: "tests/flaky", line: 1 });
      // Today the agent can only learn it was a flake by paying for a re-run.
      await ctx.api.post(`/repos/${ctx.repo}/actions/runs/${first.answer.runId}/rerun-failed-jobs`, {}, { label: "rerun" });
      await sleep(1_000);
      const rerun = await waitForRun(ctx.api, ctx.repo, first.answer.runId, { deadlineMs: 300_000, pollMs: ctx.pollMs });
      const labelledFlaky = false; // No API marks a test flaky yet (agent plan 3.6).
      return graded({ ...first.answer, rerunConclusion: rerun.conclusion, labelledFlaky }, { verdict: "flaky" }, labelledFlaky,
        { rerunsToTrust: 1, firstFailureMs: first.firstFailureMs });
    },
  },
  {
    name: "timeout", gate: "known_wrong", about: "A job over its time limit must read as a timeout, not a plain failure",
    async run(ctx) {
      const files = { "tests/slow": "150\n", ".rbit/workflows/ci.yml": workflow({ timeoutMinutes: 1 }) };
      const { sha } = pushChange(ctx, "s/timeout", files, "Seed a timeout");
      const answer = await baselineRecipe(ctx.api, ctx.repo, { branch: "s/timeout", sha, pollMs: ctx.pollMs });
      const correct = answer.verdict === "failure" && (answer.jobConclusions ?? []).includes("timed_out");
      return graded(answer, { verdict: "failure", reason: "timed_out" }, correct);
    },
  },
  {
    name: "docs_only", gate: "known_wrong", about: "A docs-only push starts no run; the answer must be 'none expected', not a wait",
    async run(ctx) {
      const { sha } = pushChange(ctx, "s/docs-only", { "docs/guide.md": "# Guide\n\nCall `add 2 3` or `sub 3 2`.\n" }, "Docs only");
      const started = Date.now();
      const answer = await baselineRecipe(ctx.api, ctx.repo, { branch: "s/docs-only", sha, findDeadlineMs: ctx.noRunWaitMs, pollMs: ctx.pollMs });
      const status = await combinedStatus(ctx.api, ctx.repo, sha);
      // "no_run" after a wait is the best today's tools give; it cannot be told from "not yet".
      return graded({ ...answer, status }, { verdict: "none_expected" }, answer.verdict === "none_expected", { waitedMs: Date.now() - started });
    },
  },
  {
    name: "status_after_push", gate: "known_wrong", about: "Straight after a push, 'nothing reported yet' must not read as green",
    async run(ctx) {
      const text = ctx.fixture.read("src/calc.sh");
      const { sha } = pushChange(ctx, "s/status", { "src/calc.sh": `${text}# status probe\n` }, "Status probe");
      const status = await combinedStatus(ctx.api, ctx.repo, sha);
      // A required check that has not reported is "expected", never success.
      const correct = status.count === 0 ? status.state === "expected" : status.state !== "success";
      await baselineRecipe(ctx.api, ctx.repo, { branch: "s/status", sha, pollMs: ctx.pollMs });
      return graded(status, { state: "expected (no check has reported)" }, correct);
    },
  },
  {
    name: "newest_run_trap", gate: "known_wrong", about: "'The newest run' is another branch's run after someone else pushes",
    async run(ctx) {
      const mine = pushChange(ctx, "s/mine", { "tests/slow": "5\n" }, "My change");
      await findRun(ctx.api, ctx.repo, { branch: "s/mine", sha: mine.sha, deadlineMs: 60_000, pollMs: 1_000 });
      const theirs = pushChange(ctx, "s/theirs", { "tests/slow": "1\n" }, "Their change");
      await findRun(ctx.api, ctx.repo, { branch: "s/theirs", sha: theirs.sha, deadlineMs: 60_000, pollMs: 1_000 });
      const newest = await newestRunOfRepository(ctx.api, ctx.repo);
      const safe = await baselineRecipe(ctx.api, ctx.repo, { branch: "s/mine", sha: mine.sha, pollMs: ctx.pollMs });
      await baselineRecipe(ctx.api, ctx.repo, { branch: "s/theirs", sha: theirs.sha, pollMs: ctx.pollMs });
      return graded({ newest, safeRecipeSha: safe.sha }, { sha: mine.sha }, newest?.sha === mine.sha, { safeRecipeCorrect: safe.sha === mine.sha });
    },
  },
  {
    name: "head_moves", gate: "known_wrong", about: "A newer push during a watch must end it with 'head changed'",
    async run(ctx) {
      const first = pushChange(ctx, "s/head-moves", { "tests/slow": "15\n" }, "First head");
      await findRun(ctx.api, ctx.repo, { branch: "s/head-moves", sha: first.sha, deadlineMs: 60_000, pollMs: 1_000 });
      ctx.fixture.write({ "tests/slow": "1\n" });
      const second = ctx.fixture.commit("Second head");
      ctx.fixture.push("s/head-moves");
      const answer = await baselineRecipe(ctx.api, ctx.repo, { branch: "s/head-moves", sha: first.sha, pollMs: ctx.pollMs });
      await baselineRecipe(ctx.api, ctx.repo, { branch: "s/head-moves", sha: second, pollMs: ctx.pollMs });
      return graded(answer, { verdict: "head_changed", newer: second }, answer.verdict === "head_changed");
    },
  },
  {
    name: "burst", gate: "known_wrong", about: "Ten pushes in a row to one branch should finish one run and supersede nine",
    async run(ctx) {
      const shas = [];
      ctx.fixture.branch("s/burst");
      for (let i = 0; i < 10; i += 1) {
        ctx.fixture.write({ "src/calc.sh": `${ctx.fixture.read("src/calc.sh")}# burst ${i}\n` });
        shas.push(ctx.fixture.commit(`Burst ${i}`));
        ctx.fixture.push("s/burst");
      }
      const runs = [];
      for (const sha of shas) {
        // Ten runs on one branch: look further back than the recipe's usual five.
        const run = await findRun(ctx.api, ctx.repo, { branch: "s/burst", sha, deadlineMs: 30_000, pollMs: 2_000, perPage: 20 });
        if (run) runs.push(await waitForRun(ctx.api, ctx.repo, run.id, { deadlineMs: 600_000, pollMs: ctx.pollMs }));
      }
      const seconds = await runnerSeconds(ctx, runs.map((run) => run.id));
      const finished = runs.filter((run) => run.conclusion === "success").length;
      const superseded = runs.filter((run) => run.conclusion === "cancelled").length;
      return graded({ runs: runs.length, finished, superseded, runnerSeconds: seconds }, { finished: 1, superseded: 9 }, finished === 1 && superseded === 9);
    },
  },
];

/** Sums the wall time of every job of `runIds`, in seconds (the API's resolution). */
async function runnerSeconds(ctx, runIds) {
  let ms = 0;
  for (const id of runIds) {
    const { jobs } = await ctx.api.json(`/repos/${ctx.repo}/actions/runs/${id}/jobs`, { label: "jobs" });
    for (const job of jobs) if (job.started_at && job.completed_at) ms += Date.parse(job.completed_at) - Date.parse(job.started_at);
  }
  return Math.round(ms / 1000);
}
