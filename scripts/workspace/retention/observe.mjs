// Trusted, default-off observation inside the helper's live lease. No deletion API.
import { randomUUID } from "node:crypto";
import { lstat, rename, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { guarded } from "../paths.mjs";
import { requireLease } from "../storage.mjs";
import { verifiedCargoCompletion } from "../cargo-tree.mjs";
import { boundedInput, policySnapshot } from "./policy.mjs";
import { namespaceUsage, freeBytes } from "./usage.mjs";
import { proveHandoff } from "./handoff.mjs";
import { planRetention } from "./planner.mjs";
const observations = new WeakMap();
const RECORD = "managed-observation.json";

export function managedRetentionMode(env) {
  const mode = env.RBIT_BUILD_MANAGED_RETENTION;
  if (mode === undefined || mode === "off") return "off";
  if (!["observe", "measure-candidate"].includes(mode)) throw new Error("RBIT_BUILD_MANAGED_RETENTION must be off, observe or measure-candidate");
  if (env.RBIT_WORKSPACE_CARGO_TREE !== "windows-job") throw new Error("Managed observation requires owned windows-job Cargo completion");
  return mode;
}
/** `read` is a filesystem seam for temporary tests, not observation/JSON authority. */
export async function beginRetention(ctx, request, lease, { read } = {}) {
  await requireLease(ctx, lease);
  const { namespace, command, args, env } = request;
  if (!/^[a-f0-9]{20}$/.test(namespace)) throw new Error("Invalid observation namespace");
  const trustedContext = Object.freeze({ root: ctx.root, checkout: ctx.checkout, cache: ctx.cache, state: ctx.state });
  const before = await policySnapshot(trustedContext, namespace, command, args, env, read);
  const handle = Object.freeze({});
  observations.set(handle, { ctx: trustedContext, namespace, command, args: [...args], env: { ...env }, before, token: lease.token, read });
  return handle;
}
async function persist(ctx, namespace, record, lease) {
  await requireLease(ctx, lease);
  const path = await guarded(ctx.root, join(ctx.cache, namespace, RECORD));
  try {
    const info = await lstat(path);
    if (!info.isFile() || info.size > 65536) throw new Error("Existing observation record is unrecognized");
    let previous;
    try { previous = JSON.parse((await boundedInput(path, false, 65536)).toString("utf8")); }
    catch { throw new Error("Existing observation record is unrecognized"); }
    if (previous.schema !== 1 || previous.source !== "managed-build-helper" || previous.namespace !== namespace) {
      throw new Error("Existing observation record is unrecognized");
    }
  } catch (error) { if (error.code !== "ENOENT") throw error; }
  const temporary = await guarded(ctx.root, join(ctx.cache, namespace, `.managed-observation-${randomUUID()}.tmp`));
  const serialized = JSON.stringify(record) + "\n";
  if (Buffer.byteLength(serialized) > 65536) throw new Error("Observation record exceeds its bound");
  await writeFile(temporary, serialized, { flag: "wx", mode: 0o600 });
  try {
    await requireLease(ctx, lease);
    await guarded(ctx.root, path);
    await rename(temporary, path);
  } finally { await rm(temporary, { force: true }); }
}
/** Only a fresh branded observation AND native completion in this lease may publish. */
export async function finishRetention(handle, completion, copies, budgetBytes, lease) {
  const observation = handle && observations.get(handle);
  if (!observation || observation.token !== lease?.token || !verifiedCargoCompletion(completion, lease)) {
    throw new Error("Managed observation/completion authority is unproved");
  }
  observations.delete(handle);
  const { ctx, namespace, command, args, env, before, read } = observation;
  await requireLease(ctx, lease);
  const path = await guarded(ctx.root, join(ctx.cache, namespace));
  const lane = await guarded(ctx.root, join(path, "shared"));
  const handoff = await proveHandoff(ctx, lane, copies);
  const after = await policySnapshot(ctx, namespace, command, args, env, read);
  const graph = before.inputId && before.inputId === after.inputId ? after.graph
    : { confidence: "conservative", reasons: ["POLICY_INPUTS_CHANGED_OR_UNOBSERVED"] };
  const usage = await namespaceUsage(path);
  const available = await freeBytes(ctx.cache);
  // A completed command cannot retroactively prove ownership/provenance of an
  // accumulated legacy shared lane. Never call it a successful managed generation.
  const decision = planRetention({
    lease: { state: "owned", tokenVerified: true, processTreeCertain: true },
    requested: graph,
    current: { id: "shared", bytes: usage.bytes, lastUsed: Date.now(), successful: false, graph },
    usage: { namespaceBytes: usage.bytes, freeBytes: available, complete: usage.complete },
    budgetBytes, rotationEnabled: false,
  });
  const record = {
    schema: 1, source: "managed-build-helper", namespace, recordedAt: Date.now(),
    commandGeneration: randomUUID(), completedCommand: ["build", "check", "clippy", "test", "nextest", "generate-lockfile"].includes(command) ? command : "unknown",
    completion: { method: completion.method, treeStopped: true },
    lane: { id: "shared", kind: "legacy-shared", eligibleForEviction: false },
    graph, inputId: graph.confidence === "known" ? after.inputId : null,
    handoff, usage: { ...usage, freeBytes: available, measuredBeforeRecordWrite: true }, decision,
  };
  await persist(ctx, namespace, record, lease);
  return { mode: "observe", recorded: true, graph, usage: record.usage, decision, eligibleForEviction: false };
}
