// Explicit process ownership for managed developer runtimes, never a global PID scan.
import { spawn } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { rmSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { UncertainProcessTreeError } from "./lease.mjs";

const pause = (ms) => new Promise((done) => setTimeout(done, ms));

export function launchTree(executable, args, options) {
  if (!Array.isArray(args) || args.some((arg) => typeof arg !== "string" || arg.includes("\0"))) throw new TypeError("Runtime arguments must be non-NUL strings");
  const { stopFile, proofFile, token, suspendedReady, resumeGate, ...spawnOptions } = options;
  let child;
  if (process.platform === "win32") {
    const launcher = fileURLToPath(new URL("job-launcher.ps1", import.meta.url));
    const argumentsFile = `${stopFile}.args.json`;
    writeFileSync(argumentsFile, JSON.stringify(args), { flag: "wx", mode: 0o600 });
    child = spawn("powershell.exe", ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", launcher,
      "-Executable", executable, "-ArgumentsFile", argumentsFile,
      "-Directory", spawnOptions.cwd, "-StopFile", stopFile, "-ProofFile", proofFile, "-Token", token,
      ...(suspendedReady ? ["-SuspendedReady", suspendedReady, "-ResumeGate", resumeGate] : [])],
    { ...spawnOptions, windowsHide: true });
    child.once("close", () => { rmSync(argumentsFile, { force: true }); });
  } else child = spawn(executable, args, { ...spawnOptions, detached: true });
  child.treeControl = { stopFile, proofFile, token };
  return child;
}

function groupExists(pid) {
  try { process.kill(-pid, 0); return true; } catch (error) { if (error.code === "ESRCH") return false; throw error; }
}

/** Windows acknowledgement comes only after job ActiveProcesses==0. */
export function verifyTreeStopped(child) {
  child.treeVerification ??= verifyOwnedTree(child);
  return child.treeVerification;
}

async function verifyOwnedTree(child) {
  if (!child.pid) return true; // native spawn failed before a target could exist
  if (process.platform === "win32") {
    try { return await readFile(child.treeControl.proofFile, "utf8") === child.treeControl.token; }
    catch (error) { if (error.code === "ENOENT") return false; throw error; }
  }
  if (!groupExists(child.pid)) return true;
  for (const signal of ["SIGTERM", "SIGKILL"]) {
    try { process.kill(-child.pid, signal); } catch (error) { if (error.code !== "ESRCH") throw error; }
    for (let attempt = 0; attempt < 30; attempt++) {
      if (!groupExists(child.pid)) return true;
      await pause(100);
    }
  }
  return false;
}

export async function stopTree(child, { awaitMarker = true } = {}) {
  if (!child.treeControl) throw new Error("Runtime is not owned by this launcher");
  if (child.exitCode === null && child.signalCode === null) {
    const closed = new Promise((done) => child.once("close", done));
    if (process.platform === "win32") await writeFile(child.treeControl.stopFile, child.treeControl.token, { flag: "wx" }).catch((error) => { if (error.code !== "EEXIST") throw error; });
    else { try { process.kill(-child.pid, "SIGTERM"); } catch (error) { if (error.code !== "ESRCH") throw error; } }
    let timer;
    try {
      // Escalate the owned Unix group while waiting: an ignoring root/pipe worker can
      // otherwise prevent close forever and keep the escalation unreachable.
      const cleanup = process.platform === "win32" ? Promise.resolve(true) : verifyTreeStopped(child);
      await Promise.race([Promise.all([closed, cleanup.then((verified) => {
        if (!verified) throw new UncertainProcessTreeError("Owned runtime group did not stop");
      })]), new Promise((_, reject) => { timer = setTimeout(() => reject(new UncertainProcessTreeError("Owned runtime did not acknowledge stop")), 15_000); })]);
    } finally { clearTimeout(timer); }
  }
  if (awaitMarker && child.runtimeCompletion) await child.runtimeCompletion;
  else if (!await verifyTreeStopped(child)) throw new UncertainProcessTreeError("Runtime tree cleanup is unverified");
}
