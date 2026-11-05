// Explicit process ownership for managed developer runtimes, never a global PID scan.
import { spawn } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { UncertainProcessTreeError } from "./lease.mjs";

const pause = (ms) => new Promise((done) => setTimeout(done, ms));

export function launchTree(executable, args, options) {
  const { stopFile, proofFile, token, ...spawnOptions } = options;
  let child;
  if (process.platform === "win32") {
    const launcher = fileURLToPath(new URL("job-launcher.ps1", import.meta.url));
    child = spawn("powershell.exe", ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", launcher,
      "-Executable", executable, "-ArgumentsBase64", Buffer.from(JSON.stringify(args)).toString("base64"),
      "-Directory", spawnOptions.cwd, "-StopFile", stopFile, "-ProofFile", proofFile, "-Token", token],
    { ...spawnOptions, windowsHide: true });
  } else child = spawn(executable, args, { ...spawnOptions, detached: true });
  child.treeControl = { stopFile, proofFile, token };
  return child;
}

function groupExists(pid) {
  try { process.kill(-pid, 0); return true; } catch (error) { if (error.code === "ESRCH") return false; throw error; }
}

/** Windows acknowledgement comes only after job ActiveProcesses==0. */
export async function verifyTreeStopped(child) {
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
      await Promise.race([closed, new Promise((_, reject) => { timer = setTimeout(() => reject(new UncertainProcessTreeError("Owned runtime did not acknowledge stop")), 15_000); })]);
    } finally { clearTimeout(timer); }
  }
  if (awaitMarker && child.runtimeCompletion) await child.runtimeCompletion;
  else if (!await verifyTreeStopped(child)) throw new UncertainProcessTreeError("Runtime tree cleanup is unverified");
}
