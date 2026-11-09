// Failure paths of an upgrade: a binary meeting a database from a newer build, a backup
// taken by a newer binary, and a migration killed part-way. Each works on its own copy.
import { cp, rm } from "node:fs/promises";
import { join } from "node:path";
import { Instance } from "./instance.mjs";
import { countsPreserved, execute, inspectDatabase, treeChanges, treeDigest } from "./store.mjs";

/** A migration version no build has: the database was "migrated by a newer build". */
export const FUTURE_MIGRATION = 9999;

const ignoreRedb = (path) => path === "rbit.redb";

/**
 * Starts `binary` on `dataDir` and expects a refusal: a non-zero exit, a message naming
 * the problem, and no change to the database, key or repositories.
 */
export async function expectRefusal(step, { work, name, dataDir, binary, pattern }) {
  const instance = await Instance.create(work, name, dataDir);
  const before = await treeDigest(dataDir, ignoreRedb);
  let started = false;
  try {
    await instance.start(binary, { timeoutMs: 60_000 });
    started = true;
  } catch {}
  if (started) await instance.stop();
  const log = await instance.logTail(15);
  const message = log.split("\n").filter((line) => /error|refus|migration|newer/i.test(line)).slice(-3).join(" | ");
  const changed = treeChanges(before, await treeDigest(dataDir, ignoreRedb));
  step.notes.push(`${name}: ${started ? "STARTED" : "refused to start"}; message: ${message || "(none)"}`);
  step.notes.push(`${name}: files changed by the attempt: ${changed.length ? changed.join(", ") : "none"}`);
  if (started) throw new Error(`${name}: the binary started on a database it must refuse`);
  if (pattern && !pattern.test(log)) throw new Error(`${name}: the refusal does not explain itself:\n${log}`);
  const meaningful = changed.filter((path) => !/^rbit\.db-(wal|shm)$/.test(path));
  if (meaningful.length) throw new Error(`${name}: the refused start changed ${meaningful.join(", ")}`);
  return { changed };
}

/** Records a migration no binary embeds, as a newer build would leave it. */
export function markNewer(dataDir) {
  execute(join(dataDir, "rbit.db"), "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (?, 'from a newer build', 1, X'00', 0)", FUTURE_MIGRATION);
}

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Kill points spread evenly over a whole `rbit migrate` run that took `seconds` (start-up
 * included), so most land while migrations are being applied.
 */
export function killDelays(seconds) {
  const span = Math.max(80, Math.round(seconds * 1000));
  // About one kill per 5 ms, so a slow measured run does not thin out the migration window.
  const count = Math.min(60, Math.max(16, Math.round(span / 5)));
  return Array.from({ length: count }, (_, index) => Math.round(10 + (span * index) / (count - 1)));
}

/**
 * Kills `rbit migrate` after each delay in `delays` (each on a fresh copy of `source`),
 * inspects what the kill left, then runs `migrate` again and requires it to finish with
 * every row intact.
 */
export async function interruptedMigrations(step, { work, source, binary, delays, target }) {
  const before = inspectDatabase(join(source, "rbit.db"));
  const observed = [];
  for (const [index, delay] of delays.entries()) {
    const dataDir = join(work, `interrupt-${index}`);
    await rm(dataDir, { recursive: true, force: true });
    await cp(source, dataDir, { recursive: true });
    const instance = await Instance.create(work, `interrupt-${index}`, dataDir);
    let child;
    const run = instance.run(binary, ["migrate"], { onSpawn: (spawned) => { child = spawned; } });
    await pause(delay);
    if (child.exitCode === null) child.kill("SIGKILL");
    const first = await run;
    const after = inspectDatabase(join(dataDir, "rbit.db"));
    const killed = first.signal === "SIGKILL";
    const state = { delay, killed, latest: after.latest, failed: after.failed, integrity: after.integrity };
    if (after.failed.length || after.integrity.length) throw new Error(`a kill at ${delay} ms left a damaged database: ${JSON.stringify(state)}`);
    const again = await instance.run(binary, ["migrate"]);
    const done = inspectDatabase(join(dataDir, "rbit.db"));
    state.restart = again.code === 0 ? `completed at ${done.latest}` : `failed: ${again.output.split("\n").filter(Boolean).slice(-2).join(" ")}`;
    observed.push(state);
    if (again.code !== 0) throw new Error(`migrate after a kill at ${delay} ms failed:\n${again.output.slice(-2000)}`);
    if (done.latest !== target) throw new Error(`migrate after a kill stopped at ${done.latest}, expected ${target}`);
    const lost = countsPreserved(before.counts, done.counts);
    if (lost.length || done.foreignKeyViolations || done.integrity.length) throw new Error(`rows changed after a kill at ${delay} ms: ${lost.join("; ")}`);
    await rm(dataDir, { recursive: true, force: true });
  }
  for (const state of observed) {
    step.notes.push(`kill after ${state.delay} ms: ${state.killed ? "killed" : "had already exited"} at migration ${state.latest}; restart ${state.restart}`);
  }
  const partial = observed.filter((state) => state.killed && state.latest > before.latest && state.latest < target);
  step.notes.push(`${partial.length} kill(s) landed between migrations ${before.latest} and ${target}`);
  return observed;
}
