// Explicit bounded data, not a resumable authority or a general command runner.
import { createHash } from "node:crypto";
import { validateCargoArgs } from "../build.mjs";
import { normalizeGraph } from "./graph.mjs";
export const digest = value => createHash("sha256").update(JSON.stringify(value)).digest("hex");
const commands = new Set(["build", "check", "clippy", "test", "nextest"]);
const object = value => value && typeof value === "object" && !Array.isArray(value);
function keys(value, allowed) {
  if (!object(value) || Object.keys(value).some(key => !allowed.includes(key))) throw new Error("SESSION_PLAN_REFUSED");
}
function withoutCargoProfile(step) {
  const result = [];
  let child = false;
  const profile = step.command === "nextest" ? "--cargo-profile" : "--profile";
  for (let index = 0; index < step.args.length; index++) {
    const arg = step.args[index];
    if (arg === "--") child = true;
    if (!child && ["--release", "-r"].includes(arg)) continue;
    if (!child && arg === profile) { index++; continue; }
    if (!child && arg.startsWith(`${profile}=`)) continue;
    result.push(arg);
  }
  return { command: step.command, args: result };
}
export function sessionPlan(input) {
  keys(input, ["schema", "arms"]);
  if (input.schema !== 1 || !Array.isArray(input.arms) || input.arms.length < 1 || input.arms.length > 2) throw new Error("SESSION_ARM_BOUND");
  const arms = input.arms.map(arm => {
    keys(arm, ["environment", "steps"]);
    const environment = { ...(arm.environment ?? {}) };
    if (!object(arm.environment ?? {}) || Object.keys(environment).length > 32
      || Object.entries(environment).some(([key, value]) => !/^(CARGO_PROFILE_[A-Z0-9_]{1,80}|CARGO_INCREMENTAL)$/.test(key)
        || typeof value !== "string" || value.length > 256 || /[^\x20-\x7e]/.test(value))) throw new Error("SESSION_PROFILE_ENV_REFUSED");
    if (!Array.isArray(arm.steps) || arm.steps.length < 1 || arm.steps.length > 16) throw new Error("SESSION_STEP_BOUND");
    const steps = arm.steps.map(step => {
      keys(step, ["command", "args"]);
      if (!commands.has(step.command) || !Array.isArray(step.args) || step.args.length > 128
        || step.args.some(arg => typeof arg !== "string" || arg.length > 4096)) throw new Error("SESSION_COMMAND_REFUSED");
      validateCargoArgs(step.args);
      if (step.command === "nextest" && step.args[0] !== "run") throw new Error("SESSION_NEXTEST_RUN_REQUIRED");
      // A destructive/config/output switch is never accepted as an unknown graph.
      return Object.freeze({ command: step.command, args: Object.freeze([...step.args]) });
    });
    return Object.freeze({ environment: Object.freeze(environment), steps: Object.freeze(steps) });
  });
  const normalized = { schema: 1, arms };
  if (arms.length === 2 && digest(arms[0].steps.map(withoutCargoProfile)) !== digest(arms[1].steps.map(withoutCargoProfile))) {
    throw new Error("SESSION_ARMS_DIFFER_BEYOND_PROFILE");
  }
  if (Buffer.byteLength(JSON.stringify(normalized)) > 32768) throw new Error("SESSION_PLAN_BOUND");
  return Object.freeze({ ...normalized, arms: Object.freeze(arms), id: digest(normalized) });
}

export function requireKnownStep(step, env, namespace) {
  const graph = normalizeGraph({ ...step, env, policy: { compatibility: namespace,
    manifestDigest: "0".repeat(64), configurationDigest: "0".repeat(64) } });
  if (graph.confidence !== "known") throw new Error("SESSION_SELECTION_UNOBSERVED");
  return graph;
}

export function requireDeclaredProfiles(plan, manifest, env, namespace) {
  const source = new TextDecoder("utf-8", { fatal: true }).decode(manifest);
  const profiles = new Set(["dev", "release", "test", "bench"]);
  for (const match of source.matchAll(/^\s*\[profile\.([A-Za-z0-9_-]+)\]\s*(?:#[^\r\n]*)?$/gm)) profiles.add(match[1]);
  const fields = "(?:BUILD_OVERRIDE_)?(?:OPT_LEVEL|DEBUG|SPLIT_DEBUGINFO|STRIP|DEBUG_ASSERTIONS|OVERFLOW_CHECKS|LTO|PANIC|INCREMENTAL|CODEGEN_UNITS|RPATH)";
  for (const arm of plan.arms) {
    for (const step of arm.steps) {
      if (!profiles.has(requireKnownStep(step, { ...env, ...arm.environment }, namespace).selection.profile)) throw new Error("SESSION_PROFILE_UNDECLARED");
    }
    for (const [key, value] of Object.entries(arm.environment)) {
      if (key === "CARGO_INCREMENTAL") {
        if (!["0", "1"].includes(value)) throw new Error("SESSION_PROFILE_OVERRIDE_UNOBSERVED");
      } else if (![...profiles].some(profile => new RegExp(`^CARGO_PROFILE_${profile.toUpperCase().replaceAll("-", "_")}_${fields}$`).test(key))) {
        throw new Error("SESSION_PROFILE_OVERRIDE_UNOBSERVED");
      }
    }
  }
}
