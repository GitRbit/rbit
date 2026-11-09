// Actions history for the rehearsal: a real baseline `rbit-runner` (host backend)
// registers, runs a pushed workflow that logs a marker and uploads an artifact, and
// stops before the upgrade, as an operator drains runners first.
import { spawn, execFileSync } from "node:child_process";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { attributeAfter, expect } from "./http.mjs";

export const LOG_MARKER = "rehearsal-log-marker-7f3a";

const WORKFLOW = `name: Rehearsal
on:
  push:
    branches: [main]
jobs:
  build:
    runs-on: self-hosted
    steps:
      - name: Write
        shell: bash
        env:
          GREETING: \${{ secrets.GREETING }}
        run: |
          echo "${LOG_MARKER}"
          echo "greeting is $GREETING"
          mkdir -p dist && echo "artifact contents" > dist/app.txt
      - uses: actions/upload-artifact@v4
        with:
          name: dist
          path: dist
`;

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export async function seedActions({ client, token, git, app, work, base, runner }) {
  const settings = await client.form("/alice/app/settings/actions", { enabled: "on", token_permission: "read" });
  expect([200, 303].includes(settings.status), "Actions are enabled for alice/app", settings);
  const secret = await client.form("/alice/app/settings/actions/secrets", { kind: "secret", name: "GREETING", value: "rehearsal-secret" });
  expect([200, 303].includes(secret.status), "an Actions secret is stored", secret);
  const page = await client.form("/alice/app/settings/actions/runners", { action: "create" });
  const registration = attributeAfter(page.text, 'id="issued-token"', "value");
  expect(registration, "a runner registration token is issued", page);

  const home = join(work, "runner");
  await mkdir(home, { recursive: true });
  const config = join(home, "runner.toml");
  execFileSync(runner, ["--config", config, "register", "--url", base, "--insecure-http", "--name", "rehearsal", "--labels", "self-hosted:host"], {
    env: { ...process.env, RBIT_RUNNER_TOKEN: registration }, stdio: "pipe",
  });
  const runnerProcess = spawn(runner, ["--config", config, "run"], { stdio: "ignore", cwd: home });
  try {
    await git.commit(app.dir, { ".rbit/workflows/ci.yml": WORKFLOW }, "Add the rehearsal workflow", 6);
    git.run(app.dir, ["push", "--quiet", app.url, "main"]);
    const deadline = Date.now() + 240_000;
    let run;
    for (;;) {
      const runs = await client.json("GET", "/api/v1/repos/alice/app/actions/runs", undefined, token);
      run = runs.body?.workflow_runs?.find((candidate) => candidate.status === "completed");
      if (run) break;
      if (Date.now() > deadline) throw new Error(`the seeded workflow did not finish: ${runs.text.slice(0, 400)}`);
      await pause(500);
    }
    expect(run.conclusion === "success", `the seeded workflow succeeded (${run.conclusion})`);
    return { repo: "alice/app", ...(await actionsFacts(client, token, run.id)), secrets: ["GREETING"] };
  } finally {
    runnerProcess.kill("SIGTERM");
    await new Promise((resolve) => (runnerProcess.exitCode !== null ? resolve() : runnerProcess.on("exit", resolve)));
  }
}

/** What a run looks like through the API and its raw log, for comparison across binaries. */
export async function actionsFacts(client, token, runId) {
  const run = await client.json("GET", `/api/v1/repos/alice/app/actions/runs/${runId}`, undefined, token);
  expect(run.status === 200, `run ${runId} is listed`, run);
  const jobs = await client.json("GET", `/api/v1/repos/alice/app/actions/runs/${runId}/jobs`, undefined, token);
  expect(jobs.status === 200, `the jobs of run ${runId} are listed`, jobs);
  const job = jobs.body.jobs[0];
  // A request the server breaks off (a panic while streaming) is recorded, not thrown.
  const attempt = (request) => request.catch((error) => ({ status: `connection failed (${error.cause?.code ?? error.message})`, text: "", bytes: Buffer.alloc(0) }));
  const log = await attempt(client.get(`/alice/app/actions/runs/${run.body.run_number}/jobs/${job.id}/logs/raw`));
  const artifacts = await client.json("GET", `/api/v1/repos/alice/app/actions/runs/${runId}/artifacts`, undefined, token);
  expect(artifacts.status === 200, `the artifacts of run ${runId} are listed`, artifacts);
  const [artifact] = artifacts.body.artifacts;
  const download = artifact
    ? await attempt(client.request("GET", `/api/v1/repos/alice/app/actions/artifacts/${artifact.id}/zip`, { headers: { authorization: `token ${token}` } }))
    : null;
  return {
    runId,
    runNumber: run.body.run_number,
    conclusion: run.body.conclusion,
    jobId: job.id,
    logStatus: log.status,
    logHasMarker: log.text.includes(LOG_MARKER),
    secretMasked: !log.text.includes("rehearsal-secret"),
    artifact: artifact ? { id: artifact.id, name: artifact.name, downloadStatus: download.status, downloadBytes: download.bytes.length } : null,
  };
}
