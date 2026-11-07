// Opt-in bounded measurement; no JSON resume, promotion, rotation or deletion API.
import { execFileSync } from "node:child_process";
import { join, resolve } from "node:path";
import { cargoProcess, compatibility } from "../build.mjs";
import { cargoTreeMode } from "../cargo-tree.mjs";
import { leased, UncertainProcessTreeError } from "../lease.mjs";
import { guarded, managedDirectory } from "../paths.mjs";
import { requireLease } from "../storage.mjs";
import { sourceProof } from "../source-proof/guard.mjs";
import { allocateMeasurementSession, candidateHeadroom } from "./candidate.mjs";
import { boundedInput, policySnapshot } from "./policy.mjs";
import { sessionPlan, requireKnownStep, requireDeclaredProfiles, digest } from "./session-plan.mjs";
import { sourceSeal, requireSourceSeal } from "./session-source.mjs";
import { sessionCommand } from "./session-command.mjs";
const sessions = new WeakMap();
const failureCodes = new Set(["SESSION_SOURCE_CHANGED", "SESSION_POLICY_UNPROVED", "SESSION_POLICY_CHANGED",
  "SESSION_COMMON_POLICY_CHANGED", "SESSION_USAGE_UNPROVED", "SESSION_REPORT_BOUND", "CANDIDATE_HEADROOM_UNPROVED",
  "SESSION_EXIT_STATUS_REFUSED", "SESSION_SOURCE_PROOF_UNTRUSTED"]);

export async function loadSessionPlan(ctx, path) {
  try {
    const input = JSON.parse((await boundedInput(await guarded(ctx.checkout, resolve(ctx.checkout, path)), false, 32768)).toString("utf8"));
    sessionPlan(input);
    return input;
  } catch { throw Error("SESSION_PLAN_UNREADABLE_OR_REFUSED"); }
}

/** Fixture seams are programmatic only, never selected by plan or evidence JSON. */
export async function measureSession(ctx, input, { env = process.env, key, execute = cargoProcess, seal = sourceSeal,
  read, provenance = sourceProof, proofInputs, platform = process.platform } = {}) {
  ctx = Object.freeze({ ...ctx });
  if (env.RBIT_BUILD_MANAGED_RETENTION !== "measure-session") throw Error("SESSION_EXPLICIT_OPT_IN_REQUIRED");
  if (cargoTreeMode(env, platform) !== "windows-job") throw Error("SESSION_NATIVE_OWNERSHIP_REQUIRED");
  const plan = sessionPlan(input);
  const requiredFreeBytes = candidateHeadroom(env);
  const baseEnv = Object.freeze({ ...env, LIBSQLITE3_FLAGS: [env.LIBSQLITE3_FLAGS ?? "", "-DSQLITE_ENABLE_QPSG"].join(" ").trim() });
  const namespace = key ?? compatibility(baseEnv, execFileSync("rustc", ["-vV"], {
    cwd: ctx.checkout, env: baseEnv, encoding: "utf8", windowsHide: true,
  }));
  if (!/^[a-f0-9]{20}$/.test(namespace)) throw Error("SESSION_NAMESPACE_REFUSED");
  for (const arm of plan.arms) for (const step of arm.steps) requireKnownStep(step, { ...baseEnv, ...arm.environment }, namespace);
  const cache = await managedDirectory(ctx, ctx.cache);
  return leased(join(cache, ".lease"), async lease => {
    // Match the ordinary source guard: our first control-directory creation must
    // precede absence/ancestor stability observations.
    await managedDirectory(ctx, ctx.state);
    const checkedSeal = async context => requireSourceSeal(await seal(context));
    const initialSource = await checkedSeal(ctx);
    requireDeclaredProfiles(plan, await (read ?? boundedInput)(join(ctx.checkout, "Cargo.toml")), baseEnv, namespace);
    const first = plan.arms[0].steps[0];
    const common = await policySnapshot(ctx, namespace, first.command, first.args, baseEnv, read);
    if (!common.inputId) throw Error("SESSION_COMMON_POLICY_UNPROVED");
    const allocation = await allocateMeasurementSession(ctx, namespace, requiredFreeBytes, lease);
    const handle = Object.freeze({});
    sessions.set(handle, { ctx, allocation, namespace, initialSource, requiredFreeBytes, baseEnv,
      commonInputId: common.inputId, samples: [], receipts: new WeakSet() });
    const owned = sessions.get(handle);
    const record = () => ({ ...allocation.record, kind: "measurement-session", planSha256: plan.id,
      sourceSeal: initialSource, samples: owned.samples, commonPolicyInputSha256: common.inputId,
      armCount: plan.arms.length, plannedCommands: plan.arms.reduce((sum, arm) => sum + arm.steps.length, 0),
      schedule: "serial-alternating-arm-order-by-step", eligibleForPromotion: false, eligibleForEviction: false,
      automaticRotationApproved: false, peakMemoryMeasured: false, compilerCacheState: "unproved",
      externalCompilerOutputs: "unproved", osPageCacheState: "unmeasured", coldCostGateEligible: false });
    try {
      await allocation.publish({ ...record(), status: "measuring" });
      const environments = [];
      for (const [armIndex, arm] of plan.arms.entries()) {
        const lane = await managedDirectory(ctx, join(allocation.path, `arm-${armIndex}`));
        environments.push(Object.freeze({ ...baseEnv, ...arm.environment, CARGO_TARGET_DIR: join(lane, "target"),
          CARGO_BUILD_TARGET_DIR: join(lane, "target"), CARGO_BUILD_BUILD_DIR: join(lane, "intermediates") }));
      }
      for (let index = 0; index < plan.arms[0].steps.length; index++) {
        const order = plan.arms.map((_, arm) => arm);
        if (index % 2) order.reverse();
        for (const arm of order) {
          const sample = await sessionCommand(owned, arm, index, plan.arms[arm].steps[index], environments[arm], lease,
            { execute, seal: checkedSeal, read, provenance, proofInputs });
          sample.executionOrdinal = owned.samples.length;
          if (Buffer.byteLength(JSON.stringify([...owned.samples, sample])) > 48 * 1024) throw Error("SESSION_REPORT_BOUND");
          owned.samples.push(sample);
          owned.active = undefined;
          await allocation.publish({ ...record(), status: "measuring" });
        }
      }
      await requireLease(ctx, lease);
      if (digest(await checkedSeal(ctx)) !== digest(initialSource)) throw Error("SESSION_SOURCE_CHANGED");
      if ((await policySnapshot(ctx, namespace, first.command, first.args, baseEnv, read)).inputId !== common.inputId) {
        throw Error("SESSION_COMMON_POLICY_CHANGED");
      }
      const completed = { ...record(), status: "succeeded" };
      await allocation.publish(completed);
      return completed;
    } catch (error) {
      const uncertain = error instanceof UncertainProcessTreeError;
      try {
        await allocation.publish({ ...record(), status: uncertain ? "uncertain" : "failed",
          ...(owned.active ? { failedSample: owned.active } : {}),
          failure: { code: uncertain ? "EXECUTION_OR_COMPLETION_UNPROVED"
            : failureCodes.has(error?.message) ? error.message : "SESSION_COMMAND_SOURCE_OR_PUBLICATION_FAILED" } });
      } catch { /* Record failure cannot replace process uncertainty or release the lease. */ }
      if (uncertain) throw error;
      throw Error("MEASUREMENT_SESSION_FAILED", { cause: error });
    } finally { sessions.delete(handle); }
  });
}
