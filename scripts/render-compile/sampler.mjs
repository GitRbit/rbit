// Read-only observer. Hashes finalize only after child close AND drained pipes.
import { spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { OwnedMemory } from "./memory.mjs";

function drained(stream) {
  return new Promise((done) => {
    if (stream.readableEnded || stream.destroyed) return done();
    stream.once("end", done); stream.once("close", done);
  });
}

async function within(promise, milliseconds) {
  let timer;
  try { return await Promise.race([promise.then(() => true), new Promise((done) => { timer = setTimeout(() => done(false), milliseconds); })]); }
  finally { clearTimeout(timer); }
}

export class ObserverLifecycleError extends Error {
  constructor(reason, report, retainControl) {
    super(reason); this.name = "ObserverLifecycleError"; this.observerReport = report; this.retainControl = retainControl;
  }
}

export async function sampler(directory, config, { spawnObserver = spawn, startupMs = 15_000,
  stopMs = 5000 + config["sample-ms"], killMs = 5000, writeStop = writeFile, compilerIdentity } = {}) {
  if (config.memory === "none") return { attach() {}, async finish() { return new OwnedMemory({ pid: 0, beforeMs: 0 }).finish(["memory-not-requested"]); } };
  const stop = join(directory, `${randomUUID()}.stop`);
  const observer = spawnObserver("powershell.exe", ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File",
    fileURLToPath(new URL("sample-processes.ps1", import.meta.url)), "-ObserverParentPid", String(process.pid),
    "-IntervalMs", String(config["sample-ms"]), "-StopFile", stop, ...(compilerIdentity?.observerArguments() ?? [])], { windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  const errors = new Set(), stderr = createHash("sha256");
  observer.stderr.on("data", (chunk) => stderr.update(chunk));
  let memory, readyResolve, closedResolve, isReady = false, isClosed = false, shutdown;
  const ready = new Promise((done) => { readyResolve = done; });
  const closed = new Promise((done) => { closedResolve = done; });
  const stopped = Promise.all([closed, drained(observer.stdout), drained(observer.stderr)]);
  const lines = createInterface({ input: observer.stdout });
  lines.on("line", (line) => {
    try {
      const frame = JSON.parse(line);
      if (frame.kind === "ready") { isReady = true; readyResolve(); }
      else if (frame.kind === "census") memory?.observe(frame);
      else { errors.add("observer-error"); readyResolve(); }
    } catch { errors.add("invalid-observer-json"); readyResolve(); }
  });
  observer.on("error", () => { errors.add("observer-spawn-failed"); readyResolve(); });
  observer.on("close", (code, signal) => {
    isClosed = true;
    if (code !== 0 || signal) errors.add("observer-exited-abnormally");
    readyResolve(); closedResolve();
  });
  const report = (confirmed) => ({ compilerIdentity: compilerIdentity?.provenance ?? { policy: "actual-executable-v1", verifiedCandidate: false, reason: "compiler-memory-identity-unverified" }, ...(memory ?? new OwnedMemory({ pid: 0, beforeMs: 0 })).finish([...errors]),
    observerStopped: confirmed, observerStderrSha256: confirmed ? stderr.digest("hex") : null,
    fields: { workingSetBytes: "Win32_Process.WorkingSetSize", privateBytes: "Win32_Process.PrivatePageCount" } });
  const finish = () => shutdown ??= (async () => {
    try { await writeStop(stop, "stop", { flag: "wx" }); }
    catch { errors.add("observer-stop-file-failed"); }
    if (!await within(stopped, stopMs)) {
      errors.add("observer-stop-timeout");
      try { observer.kill(); } catch { errors.add("observer-kill-failed"); }
      if (!await within(stopped, killMs)) {
        errors.add("observer-closure-unproved");
        observer.unref?.(); observer.stdout.unref?.(); observer.stderr.unref?.();
        throw new ObserverLifecycleError("Observer closure could not be proved", report(false), true);
      }
    }
    return report(true);
  })();
  if (!await within(ready, startupMs)) errors.add("observer-startup-timeout");
  if (!isReady || isClosed || errors.size) {
    const evidence = await finish();
    throw new ObserverLifecycleError("Observer did not become ready", evidence, false);
  }
  return {
    attach(child, beforeMs, afterMs) { memory = new OwnedMemory({ pid: child.pid ?? 0, parentPid: process.pid, beforeMs, afterMs }); },
    finish,
  };
}
