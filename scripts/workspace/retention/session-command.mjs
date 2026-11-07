// One command's native receipt, current source admission and exact handoff stay indivisible.
import { performance } from "node:perf_hooks";
import { join } from "node:path";
import { copyExecutables } from "../artifacts.mjs";
import { verifiedCargoCompletion, failedCargoCompletion } from "../cargo-tree.mjs";
import { UncertainProcessTreeError } from "../lease.mjs";
import { requireLease } from "../storage.mjs";
import { policySnapshot } from "./policy.mjs";
import { proveHandoff } from "./handoff.mjs";
import { freeBytes, namespaceUsage } from "./usage.mjs";
import { requireCandidateHeadroom } from "./candidate.mjs";
import { digest } from "./session-plan.mjs";

export async function sessionCommand(value, arm, index, step, env, lease, dependencies) {
  const { ctx, namespace, initialSource, requiredFreeBytes } = value;
  const lane = join(value.allocation.path, `arm-${arm}`);
  const active = value.active = { arm, step: index, command: step.command, phase: "preflight", status: "failed",
    commandStarted: false, treeStopped: false };
  await requireLease(ctx, lease);
  requireCandidateHeadroom(await freeBytes(ctx.cache), requiredFreeBytes);
  if (digest(await dependencies.seal(ctx)) !== digest(initialSource)) throw Error("SESSION_SOURCE_CHANGED");
  const common = await policySnapshot(ctx, namespace, step.command, step.args, value.baseEnv, dependencies.read);
  if (common.inputId !== value.commonInputId) throw Error("SESSION_COMMON_POLICY_CHANGED");
  const before = await policySnapshot(ctx, namespace, step.command, step.args, env, dependencies.read);
  if (!before.inputId || before.graph.confidence !== "known") throw Error("SESSION_POLICY_UNPROVED");
  const json = ["build", "check", "clippy", "test"].includes(step.command);
  const args = [step.command, ...(json ? ["--message-format=json-render-diagnostics"] : []), ...step.args];
  active.graph = before.graph;
  active.invocationSha256 = digest({ args, env: Object.entries(env).sort(([a], [b]) => a.localeCompare(b)) });
  const artifacts = [];
  active.cargoArtifacts = { messages: 0, fresh: 0, rebuilt: 0 };
  let proof, receipt, stopped = false, exitSeen = false;
  let began;
  try {
    proof = await dependencies.provenance.begin(ctx, { lane, namespace, command: step.command, args: step.args, env }, lease, dependencies.proofInputs);
    active.phase = "execution";
    active.commandStarted = true;
    began = performance.now();
    receipt = await dependencies.execute(ctx, args, env, artifact => {
      if (artifacts.length >= 1024) throw Error("SESSION_ARTIFACT_BOUND");
      artifacts.push(artifact);
    }, { lease, invocation: proof.invocation, output: process.stderr, diagnostics: process.stderr,
      onExit: (code, signal) => {
        if (exitSeen || !(code === null || Number.isInteger(code) && code >= 0 && code <= 0xffff_ffff)
          || !(signal === null || typeof signal === "string" && /^SIG[A-Z0-9]{1,16}$/.test(signal))) throw Error("SESSION_EXIT_STATUS_REFUSED");
        exitSeen = true;
        active.exit = { code, signal };
      },
      onMessage: message => {
        proof.observe(message);
        if (message.reason !== "compiler-artifact") return;
        if (++active.cargoArtifacts.messages > 1_000_000) throw Error("SESSION_ARTIFACT_BOUND");
        if (message.fresh === true) active.cargoArtifacts.fresh++;
        if (message.fresh === false) active.cargoArtifacts.rebuilt++;
      } });
    active.commandWallMs = Math.max(0, performance.now() - began);
    stopped = verifiedCargoCompletion(receipt, lease, proof.invocation) && !value.receipts.has(receipt);
    if (!stopped) throw new UncertainProcessTreeError("SESSION_NATIVE_COMPLETION_UNPROVED");
    value.receipts.add(receipt);
    active.treeStopped = true;
    if (!exitSeen || active.exit.code !== 0 || active.exit.signal !== null) throw Error("SESSION_EXIT_STATUS_REFUSED");
    active.phase = "provenance-and-handoff";
    const sourceProof = await dependencies.provenance.finish(proof.handle, receipt, lease);
    if (!sourceProof.trusted) throw Error("SESSION_SOURCE_PROOF_UNTRUSTED");
    if (digest(await dependencies.seal(ctx)) !== digest(initialSource)) throw Error("SESSION_SOURCE_CHANGED");
    const after = await policySnapshot(ctx, namespace, step.command, step.args, env, dependencies.read);
    const afterCommon = await policySnapshot(ctx, namespace, step.command, step.args, value.baseEnv, dependencies.read);
    if (before.inputId !== after.inputId) throw Error("SESSION_POLICY_CHANGED");
    if (afterCommon.inputId !== value.commonInputId) throw Error("SESSION_COMMON_POLICY_CHANGED");
    const output = step.command === "build" ? await copyExecutables(ctx, lane, namespace, artifacts) : { copies: [] };
    const handoff = await proveHandoff(ctx, lane, output.copies);
    const usage = await namespaceUsage(lane);
    if (!usage.complete) throw Error("SESSION_USAGE_UNPROVED");
    return { ...active, status: "succeeded", phase: "complete", sourceProof, graph: after.graph, inputId: after.inputId,
      handoff, usage, outputState: index === 0 ? "fresh-arm-directories" : "reused-arm-directories",
      completion: { method: receipt.method, treeStopped: true }, nativeLauncherIncluded: true };
  } catch (error) {
    if (!stopped && proof) {
      const failed = failedCargoCompletion(error, lease);
      stopped = !!failed && verifiedCargoCompletion(failed, lease, proof.invocation) && !value.receipts.has(failed);
      if (stopped) value.receipts.add(failed);
    }
    if (began !== undefined) active.commandWallMs ??= Math.max(0, performance.now() - began);
    active.treeStopped = stopped;
    // Establish uncertainty BEFORE best-effort ledger cleanup. Secondary errors
    // must never let the enclosing lease release after unproved execution.
    const failure = error instanceof UncertainProcessTreeError ? error : active.commandStarted && !stopped
      ? new UncertainProcessTreeError("SESSION_NATIVE_COMPLETION_UNPROVED", { cause: error }) : error;
    if (proof) { try { await dependencies.provenance.fail(proof.handle, lease); } catch {} }
    throw failure;
  }
}
