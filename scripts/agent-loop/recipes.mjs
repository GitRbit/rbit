// How an agent finds out what happened to its push, using only what rbit offers today
// (the safe recipe of the agent plan: find the run by commit, wait, read the failed job's
// log). Each recipe returns a verdict the benchmark grades, and the API client's meter
// records what it cost. Later steps add recipes for new endpoints beside this baseline.

/** Conclusions that mean a job did not pass. */
const FAILED = new Set(["failure", "timed_out", "cancelled", "startup_failure"]);

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

/**
 * Pulls the first useful failure line out of a job log: a workflow-command annotation
 * (`::error file=…,line=…`), then a `path:line:col: error` line, then a `FAIL … (path:line)`
 * test line, then an `##[error]` marker. Timestamps and colour codes are ignored.
 */
export function firstFailure(log) {
  const lines = log.split(/\r?\n/).map((line) => line.replace(/\x1b\[[0-9;]*m/g, "").replace(/^\S*Z\s/, ""));
  const annotation = /::error file=([^,]+),line=(\d+)(?:,col=(\d+))?::(.*)$/;
  const compiler = /^([\w./-]+):(\d+):(\d+): error: (.*)$/;
  const test = /^FAIL (\S+) \(([\w./-]+):(\d+)\): (.*)$/;
  for (const [pattern, map] of [
    [annotation, (m) => ({ path: m[1], line: Number(m[2]), message: m[4] })],
    [compiler, (m) => ({ path: m[1], line: Number(m[2]), message: m[4] })],
    [test, (m) => ({ path: m[2], line: Number(m[3]), message: `${m[1]}: ${m[4]}` })],
  ]) {
    for (const line of lines) {
      const match = line.match(pattern);
      if (match) return map(match);
    }
  }
  const marker = lines.find((line) => line.includes("##[error]"));
  return marker ? { path: null, line: null, message: marker.slice(marker.indexOf("##[error]") + 9).trim() } : null;
}

/**
 * Finds the run for exactly `sha` on `branch`: lists the branch's newest runs and picks the
 * one whose `head_sha` matches. Returns `null` when none appears before `deadlineMs`, which
 * the caller must report as "no run", never as a pass.
 */
export async function findRun(api, repo, { branch, sha, deadlineMs, pollMs = 3_000, perPage = 5 }) {
  const until = Date.now() + deadlineMs;
  for (;;) {
    const page = await api.json(`/repos/${repo}/actions/runs?branch=${encodeURIComponent(branch)}&per_page=${perPage}`, { label: "runs" });
    const run = page.workflow_runs.find((candidate) => candidate.head_sha === sha);
    if (run) return run;
    if (Date.now() >= until) return null;
    await sleep(pollMs);
  }
}

/** Polls one run until it completes (what `rbit run watch <id>` does) or the deadline passes. */
export async function waitForRun(api, repo, runId, { deadlineMs, pollMs = 3_000 }) {
  const until = Date.now() + deadlineMs;
  for (;;) {
    const run = await api.json(`/repos/${repo}/actions/runs/${runId}`, { label: "run" });
    if (run.status === "completed") return run;
    if (Date.now() >= until) return run;
    await sleep(pollMs);
  }
}

/**
 * Today's safe recipe, end to end. Returns
 * `{ verdict, sha, runId, conclusion, failure, firstFailureAt }` where `verdict` is
 * `success`, `failure`, `no_run` (nothing appeared in time) or `pending` (still running at
 * the deadline), and `failure` is the first failure line found in a failed job's log.
 */
export async function baselineRecipe(api, repo, { branch, sha, findDeadlineMs = 60_000, runDeadlineMs = 300_000, pollMs = 3_000 }) {
  const run = await findRun(api, repo, { branch, sha, deadlineMs: findDeadlineMs, pollMs });
  if (!run) return { verdict: "no_run", sha, runId: null };
  const done = await waitForRun(api, repo, run.id, { deadlineMs: runDeadlineMs, pollMs });
  if (done.status !== "completed") return { verdict: "pending", sha, runId: run.id };
  if (done.conclusion === "success") return { verdict: "success", sha, runId: run.id, conclusion: done.conclusion };
  const { jobs } = await api.json(`/repos/${repo}/actions/runs/${run.id}/jobs`, { label: "jobs" });
  const failed = jobs.filter((job) => FAILED.has(job.conclusion));
  let failure = null;
  for (const job of failed) {
    const log = await api.text(`/repos/${repo}/actions/jobs/${job.id}/logs`, { label: "log" });
    failure = log === null ? null : firstFailure(log);
    if (failure) {
      failure = { ...failure, job: job.name, jobConclusion: job.conclusion };
      break;
    }
  }
  const reasons = [...new Set(failed.map((job) => job.conclusion))];
  return { verdict: "failure", sha, runId: run.id, conclusion: done.conclusion, jobConclusions: reasons, failure, firstFailureAt: Date.now() };
}

/**
 * The trap of section 3.2 item 1: "the newest run of the repository", which is what
 * `rbit run watch` without an id follows. Returns that run's head commit.
 */
export async function newestRunOfRepository(api, repo) {
  const page = await api.json(`/repos/${repo}/actions/runs?per_page=1`, { label: "runs" });
  const [run] = page.workflow_runs;
  return run ? { runId: run.id, sha: run.head_sha, branch: run.head_branch } : null;
}

/** The combined commit status straight after a push: what `rbit pr checks` reads. */
export async function combinedStatus(api, repo, sha) {
  const status = await api.json(`/repos/${repo}/commits/${sha}/status`, { label: "status" });
  return { state: status.state, count: status.total_count ?? status.statuses?.length ?? 0 };
}
