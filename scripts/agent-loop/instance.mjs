// Starts a throwaway rbit instance (the "candidate") from built binaries: an empty data
// directory, a loopback port, one account with a token, and an rbit-runner on the host
// backend registered against it. Everything lives under one work directory and is removed
// by `stop()`. Nothing here needs a network beyond loopback.
import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { createWriteStream, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { join, resolve } from "node:path";
import { Api, FormClient, attributeAfter } from "./api.mjs";

/** The label fixture workflows ask for; the candidate's runner is the only one with it. */
export const RUNNER_LABEL = "agent-loop";

/** A free loopback port, chosen by the kernel. */
export function freePort() {
  return new Promise((done, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => done(port));
    });
  });
}

/** The candidate's configuration: Actions on, no update checks, no rate limits. */
export function candidateConfig({ port, dataDir, setupToken }) {
  return [
    "[server]", `bind = "127.0.0.1:${port}"`, `public_url = "http://127.0.0.1:${port}"`, "open_registration = false", "",
    "[storage]", `data_dir = ${JSON.stringify(dataDir)}`, "",
    // The benchmark polls on purpose; throttles would measure themselves, not the loop.
    "[security]", `setup_token = ${JSON.stringify(setupToken)}`, "rate_limit = false", "hsts = false", "",
    "[updates]", "check_for_updates = false", "",
    "[actions]", "enabled = true", "enable_for_new_repositories = true", "",
  ].join("\n");
}

function startProcess(program, args, { cwd, env, log }) {
  const output = createWriteStream(log, { flags: "a" });
  const child = spawn(program, args, { cwd, env: { ...process.env, ...env }, stdio: ["ignore", "pipe", "pipe"] });
  child.stdout.pipe(output);
  child.stderr.pipe(output);
  return child;
}

async function runToEnd(program, args, { cwd, env, log }) {
  const child = startProcess(program, args, { cwd, env, log });
  const code = await new Promise((done) => child.once("exit", done));
  if (code !== 0) throw new Error(`${program} ${args[0]} exited with ${code}; see ${log}`);
}

async function waitForHealth(base, child, deadlineMs) {
  const until = Date.now() + deadlineMs;
  while (Date.now() < until) {
    if (child.exitCode !== null) throw new Error(`the candidate server exited with ${child.exitCode} during start-up`);
    try {
      if ((await fetch(`${base}/healthz`, { signal: AbortSignal.timeout(2_000) })).ok) return;
    } catch {
      // Not listening yet.
    }
    await new Promise((done) => setTimeout(done, 250));
  }
  throw new Error(`the candidate server did not answer /healthz within ${deadlineMs} ms`);
}

async function stopProcess(child, graceMs = 10_000) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  child.kill("SIGTERM");
  const exited = await Promise.race([
    new Promise((done) => child.once("exit", () => done(true))),
    new Promise((done) => setTimeout(() => done(false), graceMs)),
  ]);
  if (!exited) child.kill("SIGKILL");
}

/** Signs up the first account (an administrator, thanks to the setup token) and makes a token. */
async function provisionAccount(base, { username, password, setupToken }) {
  const browser = new FormClient(base);
  const signup = await browser.post("/signup", { username, email: `${username}@example.test`, password, setup_token: setupToken });
  if (signup.status !== 303) throw new Error(`sign-up answered HTTP ${signup.status}`);
  const tokens = {};
  // `write` is what an agent works with; `admin` only registers the runner.
  for (const scope of ["write", "admin"]) {
    const page = await browser.post("/settings/tokens", { name: `agent-loop-${scope}`, scope, expires: "never" });
    const token = attributeAfter(page.text, 'id="new-token"', "value");
    if (page.status !== 200 || !token?.startsWith("rbit_")) throw new Error(`token creation answered HTTP ${page.status} without a token`);
    tokens[scope] = token;
  }
  return tokens;
}

/**
 * Boots the candidate. `rbit` and `runner` are paths to the binaries under test; `workDir`
 * is created and later removed. Returns the base URL, the account, an API client, the
 * runner's work directory, and `stop()`.
 */
export async function startCandidate({ rbit, runner, workDir, startDeadlineMs = 60_000, runnerCapacity = 1 }) {
  for (const binary of [rbit, runner]) if (!existsSync(binary)) throw new Error(`missing binary: ${binary}`);
  rmSync(workDir, { recursive: true, force: true });
  const dirs = { data: join(workDir, "data"), runner: join(workDir, "runner"), logs: join(workDir, "logs") };
  for (const dir of Object.values(dirs)) mkdirSync(dir, { recursive: true });
  const port = await freePort();
  const base = `http://127.0.0.1:${port}`;
  const setupToken = randomBytes(16).toString("hex");
  const configPath = join(workDir, "rbit.toml");
  writeFileSync(configPath, candidateConfig({ port, dataDir: resolve(dirs.data), setupToken }), { mode: 0o600 });

  const processes = [];
  const stop = async ({ keep = false } = {}) => {
    for (const child of processes.reverse()) await stopProcess(child);
    if (!keep) rmSync(workDir, { recursive: true, force: true });
  };
  try {
    const server = startProcess(rbit, ["--config", configPath, "serve"], { cwd: workDir, env: { RUST_LOG: "warn" }, log: join(dirs.logs, "server.log") });
    processes.push(server);
    await waitForHealth(base, server, startDeadlineMs);

    const username = "agent";
    const tokens = await provisionAccount(base, { username, password: randomBytes(18).toString("hex"), setupToken });
    const api = new Api(base, tokens.write);
    const runnerProcess = await startRunner({ api: new Api(base, tokens.admin), base, runner, dir: dirs.runner, log: join(dirs.logs, "runner.log"), username, capacity: runnerCapacity });
    processes.push(runnerProcess);
    return { base, username, token: tokens.write, api, dirs, port, server, runner: runnerProcess, stop };
  } catch (error) {
    await stop({ keep: true });
    error.message += ` (candidate logs kept in ${dirs.logs})`;
    throw error;
  }
}

/** Registers a host-backend runner for the account's repositories and starts it. */
async function startRunner({ api, base, runner, dir, log, username, capacity }) {
  // A user-level registration token serves every repository the account owns.
  const { token } = await api.post("/user/actions/runners/registration-token", {}, { label: "setup" });
  const config = join(dir, "runner.toml");
  await runToEnd(runner, ["--config", config, "register", "--url", base, "--name", "agent-loop", "--labels", `${RUNNER_LABEL}:host`,
    "--capacity", String(capacity), "--insecure-http"], { cwd: dir, env: { RBIT_RUNNER_TOKEN: token }, log });
  const child = startProcess(runner, ["--config", config, "run"], { cwd: dir, env: {}, log });
  await new Promise((done) => setTimeout(done, 500));
  if (child.exitCode !== null) throw new Error(`rbit-runner exited with ${child.exitCode}; see ${log}`);
  return child;
}

/** The last `lines` lines of a candidate log, for a failure message. */
export function tail(path, lines = 40) {
  if (!existsSync(path)) return "";
  return readFileSync(path, "utf8").split("\n").slice(-lines).join("\n");
}
