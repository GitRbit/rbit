// Read-only Windows observer. Unrelated census identities are discarded in memory.
import { spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { OwnedMemory } from "./memory.mjs";

export async function sampler(directory, config) {
  if (config.memory === "none") return { attach() {}, async finish() { return new OwnedMemory({ pid: 0, beforeMs: 0 }).finish(["memory-not-requested"]); } };
  const stop = join(directory, `${randomUUID()}.stop`);
  const script = fileURLToPath(new URL("sample-processes.ps1", import.meta.url));
  const observer = spawn("powershell.exe", ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script,
    "-ObserverParentPid", String(process.pid), "-IntervalMs", String(config["sample-ms"]), "-StopFile", stop],
  { windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  const errors = new Set();
  const stderr = createHash("sha256");
  observer.stderr.on("data", (chunk) => stderr.update(chunk));
  let memory, readyResolve, closedResolve, startupTimer;
  const ready = new Promise((done) => { readyResolve = done; });
  const closed = new Promise((done) => { closedResolve = done; });
  const lines = createInterface({ input: observer.stdout });
  lines.on("line", (line) => {
    try {
      const frame = JSON.parse(line);
      if (frame.kind === "ready") readyResolve();
      else if (frame.kind === "census") memory?.observe(frame);
      else errors.add("observer-error");
    } catch { errors.add("invalid-observer-json"); }
  });
  observer.on("error", () => { errors.add("observer-spawn-failed"); readyResolve(); });
  observer.on("close", (code, signal) => {
    if (code !== 0 || signal) errors.add("observer-exited-abnormally");
    readyResolve(); closedResolve();
  });
  await Promise.race([ready, new Promise((done) => { startupTimer = setTimeout(() => { errors.add("observer-startup-timeout"); done(); }, 15_000); })]);
  clearTimeout(startupTimer);
  return {
    attach(child, beforeMs, afterMs) { memory = new OwnedMemory({ pid: child.pid ?? 0, parentPid: process.pid, beforeMs, afterMs }); },
    async finish() {
      await writeFile(stop, "stop", { flag: "wx" });
      let timeout;
      await Promise.race([closed, new Promise((done) => { timeout = setTimeout(() => {
        errors.add("observer-stop-timeout"); observer.kill(); done();
      }, 5000 + config["sample-ms"]); })]);
      clearTimeout(timeout);
      const report = (memory ?? new OwnedMemory({ pid: 0, beforeMs: 0 })).finish([...errors]);
      return { ...report, observerStderrSha256: stderr.digest("hex"), fields: { workingSetBytes: "Win32_Process.WorkingSetSize", privateBytes: "Win32_Process.PrivatePageCount" } };
    },
  };
}
