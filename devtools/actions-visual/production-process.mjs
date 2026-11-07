// Own one fixed in-container test process; no cookie or child output is public.
import { spawn } from "node:child_process";
import { createConnection } from "node:net";
import { productionBinary, fixtureArguments, productionSocket, productionOrigin } from "./production-policy.mjs";

export function parseFixtureReady(line) {
  const prefix = "RBIT_ACTIONS_FIXTURE_READY ";
  if (!line.startsWith(prefix)) return null;
  let value;
  try { value = JSON.parse(line.slice(prefix.length)); } catch { throw Error("FIXTURE_READY_REFUSED"); }
  if (Object.keys(value ?? {}).sort().join(",") !== "cookie,version" || value.version !== 1
    || !/^[A-Za-z0-9_-]{43}$/.test(value.cookie ?? "")) throw Error("FIXTURE_READY_REFUSED");
  return value.cookie;
}
async function within(promise, ms, code) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Error(code)), ms); })]); }
  finally { clearTimeout(timer); }
}
export function fixtureCommand(command) {
  const expected = { "revoke-writer\n": "REVOKED\n", "finish-log-job\n": "LOG-FINISHED\n", "stop\n": "STOPPED\n" }[command];
  if (!expected) return Promise.reject(Error("FIXTURE_COMMAND_REFUSED"));
  return new Promise((resolve, reject) => {
    const socket = createConnection({ path: productionSocket });
    let response = "", failed = false;
    const fail = () => { failed = true; socket.destroy(); reject(Error("FIXTURE_CONTROL_REFUSED")); };
    socket.setTimeout(12000, fail);
    socket.on("error", fail);
    socket.on("connect", () => socket.end(command));
    socket.on("data", chunk => { response += chunk.toString("ascii"); if (response.length > 32) fail(); });
    socket.on("close", () => {
      if (!failed && response === expected) resolve();
      else reject(Error("FIXTURE_CONTROL_REFUSED"));
    });
  });
}

export class ProductionFixture {
  #child; #closed; #exit; #ready; #token; #failure; #stop; #command;
  constructor(spawnProcess = spawn, command = fixtureCommand) {
    this.#command = command;
    this.#child = spawnProcess(productionBinary, [...fixtureArguments], { cwd: "/tmp", windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"], env: { PATH: "/usr/bin:/bin", HOME: "/tmp/visual-home", TMPDIR: "/tmp",
        LANG: "C.UTF-8", TZ: "UTC", RBIT_ACTIONS_VISUAL_FIXTURE: "isolated-linux-v1" } });
    let resolveReady, rejectReady, buffer = "", bytes = 0;
    this.#ready = new Promise((resolve, reject) => { resolveReady = resolve; rejectReady = reject; });
    const fail = code => {
      this.#failure ??= code;
      rejectReady(Error(code));
      this.#child.kill("SIGTERM");
    };
    this.#closed = new Promise(resolve => this.#child.once("close", (code, signal) => {
      this.#exit = { code: Number.isInteger(code) && code >= 0 && code <= 255 ? code : null,
        signal: [null, "SIGTERM", "SIGKILL", "SIGABRT", "SIGSEGV", "SIGSYS"].includes(signal) ? signal : "unknown" };
      resolve();
      if (!this.#token) rejectReady(Error("FIXTURE_EXITED_BEFORE_READY"));
    }));
    this.#child.once("error", () => fail("FIXTURE_SPAWN_FAILED"));
    this.#child.stderr.on("data", () => {}); // Drain without retaining panic messages, paths or driver errors.
    this.#child.stdout.on("data", chunk => {
      bytes += chunk.length;
      if (bytes > 8192) { fail("FIXTURE_OUTPUT_LIMIT"); return; }
      buffer += chunk.toString("utf8");
      for (let index; (index = buffer.indexOf("\n")) >= 0;) {
        const line = buffer.slice(0, index).replace(/\r$/, ""); buffer = buffer.slice(index + 1);
        try {
          const token = parseFixtureReady(line);
          if (token) {
            if (this.#token) { fail("FIXTURE_READY_REFUSED"); return; }
            this.#token = token; resolveReady();
          }
        } catch { fail("FIXTURE_READY_REFUSED"); }
      }
    });
  }
  async ready() { await within(this.#ready, 30000, "FIXTURE_READY_TIMEOUT"); this.healthy(); }
  healthy() { if (this.#failure || this.#exit) throw Error(this.#failure ?? "FIXTURE_EXITED_EARLY"); }
  async authenticate(context) {
    this.healthy();
    if (!this.#token) throw Error("FIXTURE_AUTH_NOT_READY");
    await context.addCookies([{ name: "rbit_session", value: this.#token, url: productionOrigin, httpOnly: true, sameSite: "Lax" }]);
  }
  async revokeWriter() { this.healthy(); await this.#command("revoke-writer\n"); this.healthy(); }
  async finishLogJob() { this.healthy(); await this.#command("finish-log-job\n"); this.healthy(); }
  status() { return { closed: !!this.#exit, exit: this.#exit ?? null }; }
  stop() {
    return this.#stop ??= (async () => {
      let failure;
      try { this.healthy(); await this.#command("stop\n"); }
      catch { failure = "FIXTURE_STOP_REFUSED"; this.#child.kill("SIGTERM"); }
      try { await within(this.#closed, 5000, "FIXTURE_CLOSE_UNPROVED"); }
      catch {
        this.#child.kill("SIGKILL");
        await within(this.#closed, 5000, "FIXTURE_CLOSE_UNPROVED");
        failure ??= "FIXTURE_FORCED_STOP";
      }
      if (failure || this.#failure || this.#exit.code !== 0 || this.#exit.signal !== null) {
        throw Error(failure ?? this.#failure ?? "FIXTURE_EXIT_REFUSED");
      }
      this.#token = undefined;
    })();
  }
}
