// Running one rbit binary against one data directory: its configuration file, the
// server process, one-shot commands (`migrate`, `backup`, `restore`, `user create`) and
// the stop/kill that an operator or a crash would cause.
import { spawn, execFileSync } from "node:child_process";
import { createWriteStream } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { join } from "node:path";

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export function freePort() {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

export function gitBinary() {
  if (process.platform === "win32") return "git";
  return execFileSync("sh", ["-c", "command -v git"], { encoding: "utf8" }).trim();
}

const tomlPath = (path) => JSON.stringify(path.replaceAll("\\", "/"));

/**
 * A configuration both the baseline and the candidate accept: only keys the baseline
 * already knew, so the rehearsal also proves an unchanged configuration file upgrades.
 */
export function configText({ dataDir, port }) {
  return `# Written by scripts/release-rehearsal.mjs.
[server]
bind = "127.0.0.1:${port}"
public_url = "http://127.0.0.1:${port}"
open_registration = false

[storage]
data_dir = ${tomlPath(dataDir)}

[security]
rate_limit = false
hsts = false

[repository]
git_binary = ${tomlPath(gitBinary())}

[mail]
transport = "log"

[webhooks]
allow_private_addresses = true

[actions]
enabled = true
`;
}

/** One configured instance; `binary` can be swapped to model an upgrade or rollback. */
export class Instance {
  constructor({ work, name, dataDir, port }) {
    this.work = work;
    this.name = name;
    this.dataDir = dataDir;
    this.port = port;
    this.configPath = join(work, `${name}.toml`);
    this.logPath = join(work, `${name}.log`);
    this.base = `http://127.0.0.1:${port}`;
    this.child = null;
  }

  static async create(work, name, dataDir) {
    const instance = new Instance({ work, name, dataDir, port: await freePort() });
    await mkdir(work, { recursive: true });
    await writeFile(instance.configPath, configText(instance));
    return instance;
  }

  env() {
    // Saved administrator settings never redirect the rehearsal's paths; a stray
    // RBIT_* variable from the caller's shell must not either.
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^(RBIT|RUSTIT)_/i.test(key)));
    return { ...env, RUST_LOG: "info", NO_COLOR: "1" };
  }

  /** Runs a one-shot command to completion. */
  run(binary, args, { input, timeoutMs = 600_000, onSpawn } = {}) {
    return new Promise((resolve, reject) => {
      const child = spawn(binary, ["--config", this.configPath, ...args], { env: this.env(), stdio: ["pipe", "pipe", "pipe"] });
      onSpawn?.(child);
      let stdout = "";
      let stderr = "";
      child.stdout.on("data", (chunk) => { stdout += chunk; });
      child.stderr.on("data", (chunk) => { stderr += chunk; });
      const timer = setTimeout(() => child.kill("SIGKILL"), timeoutMs);
      child.on("error", reject);
      child.on("close", (code, signal) => {
        clearTimeout(timer);
        resolve({ code, signal, stdout, stderr, output: `${stdout}${stderr}` });
      });
      child.stdin.end(input ?? "");
    });
  }

  /** Starts `serve` and resolves once `/healthz` answers, or rejects with the log. */
  async start(binary, { timeoutMs = 180_000 } = {}) {
    if (this.child) throw new Error(`${this.name} is already running`);
    const log = createWriteStream(this.logPath, { flags: "a" });
    log.write(`\n=== ${new Date().toISOString()} start ${binary}\n`);
    const child = spawn(binary, ["--config", this.configPath, "serve"], { env: this.env(), stdio: ["ignore", "pipe", "pipe"] });
    child.stdout.pipe(log, { end: false });
    child.stderr.pipe(log, { end: false });
    const exited = new Promise((resolve) => child.on("exit", (code, signal) => resolve({ code, signal })));
    this.child = child;
    this.exited = exited;
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const done = await Promise.race([exited, pause(200).then(() => null)]);
      if (done) {
        this.child = null;
        throw new Error(`${this.name} exited during start-up (${done.code ?? done.signal}):\n${await this.logTail()}`);
      }
      try {
        const response = await fetch(`${this.base}/healthz`);
        if (response.ok) return;
      } catch {}
      if (Date.now() > deadline) {
        await this.kill();
        throw new Error(`${this.name} did not become healthy:\n${await this.logTail()}`);
      }
    }
  }

  /** What the process printed, for failure messages and refusal checks. */
  async logTail(lines = 40) {
    try { return (await readFile(this.logPath, "utf8")).split("\n").slice(-lines).join("\n"); }
    catch { return ""; }
  }

  /** A graceful stop (SIGTERM, as systemd or `docker stop` sends), then a hard one. */
  async stop() {
    if (!this.child) return;
    this.child.kill("SIGTERM");
    const done = await Promise.race([this.exited, pause(30_000).then(() => null)]);
    if (!done) await this.kill();
    this.child = null;
  }

  /** Kills the process without warning, as a crash or power loss of the process would. */
  async kill() {
    if (!this.child) return;
    this.child.kill("SIGKILL");
    await this.exited;
    this.child = null;
  }
}
