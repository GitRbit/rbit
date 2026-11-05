// One cross-process lease protects Cargo output through handoff and test execution.
// Stale leases fail closed: a crashed owner may have left a compiler child running.
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { hostname } from "node:os";
import { join } from "node:path";
import { randomUUID } from "node:crypto";

const pause = (ms) => new Promise((done) => setTimeout(done, ms));

export function alive(pid) {
  if (!Number.isSafeInteger(pid) || pid < 1) return false;
  try { process.kill(pid, 0); return true; } catch (error) { return error.code !== "ESRCH"; }
}

export async function owner(path) {
  try { return JSON.parse(await readFile(join(path, "owner.json"), "utf8")); }
  catch (error) { if (error.code === "ENOENT") return null; throw error; }
}

/** A stale lock requires manual investigation; never steal output from orphaned rustc. */
export async function acquire(path, { timeout = 120_000, signal } = {}) {
  const deadline = Date.now() + timeout;
  const record = { pid: process.pid, host: hostname(), token: randomUUID(), created: new Date().toISOString() };
  for (;;) {
    signal?.throwIfAborted();
    try {
      await mkdir(path);
      try { await writeFile(join(path, "owner.json"), JSON.stringify(record), { flag: "wx" }); }
      catch (error) { await rm(path, { recursive: true }); throw error; }
      break;
    } catch (error) {
      if (error.code !== "EEXIST") throw error;
      const current = await owner(path);
      if (current?.host === hostname() && !alive(current.pid)) {
        throw new Error(`Stale lease ${path}; inspect orphaned Cargo/rustc processes, then remove this lease directory manually`);
      }
      if (Date.now() >= deadline) throw new Error(`Timed out waiting for lease ${path}`);
      await pause(100);
    }
  }
  let released = false;
  return async () => {
    if (released) return;
    const current = await owner(path);
    if (current?.token !== record.token) throw new Error(`Lease ownership changed: ${path}`);
    await rm(path, { recursive: true });
    released = true;
  };
}

export async function leased(path, task, options) {
  const release = await acquire(path, options);
  try { return await task(); } finally { await release(); }
}
