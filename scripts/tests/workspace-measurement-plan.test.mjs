import assert from "node:assert/strict";
import test from "node:test";
import { sessionPlan, requireDeclaredProfiles, requireKnownStep } from "../workspace/retention/session-plan.mjs";
const step = (command = "check", args = ["-p", "model"]) => ({ command, args });
const namespace = "a".repeat(20);

test("two arms differ only in explicit profile choices while commands and arguments stay bounded", () => {
  const value = sessionPlan({ schema: 1, arms: [{ steps: [step()] },
    { steps: [step("check", ["-p", "model", "--profile", "compact"])], environment: { CARGO_PROFILE_COMPACT_DEBUG: "0" } }] });
  requireDeclaredProfiles(value, Buffer.from('[profile.compact]\ninherits="dev"\n'), {}, namespace);
  for (const arm of value.arms) assert.equal(requireKnownStep(arm.steps[0], arm.environment, namespace).confidence, "known");
  for (const input of [
    { schema: 1, arms: Array(3).fill({ steps: [step()] }) },
    { schema: 1, arms: [{ steps: Array(17).fill(step()) }] },
    { schema: 1, arms: [{ steps: [step()] }, { steps: [step("check", ["-p", "other"])] }] },
    { schema: 1, arms: [{ steps: [step()], environment: { RUSTFLAGS: "-Cincremental=/private" } }] },
    { schema: 1, arms: [{ steps: [step("clean")] }] },
    { schema: 1, arms: [{ steps: [step("fetch")] }] },
    { schema: 1, arms: [{ steps: [step("check", ["--target-dir", "/private"])] }] },
    { schema: 1, arms: [{ steps: [step()], execute: "private-command" }] },
  ]) assert.throws(() => sessionPlan(input));
  assert.throws(() => requireDeclaredProfiles(value, Buffer.from("[workspace]\n"), {}, namespace), /UNDECLARED/);
  assert.throws(() => requireKnownStep(step("check", ["--unknown-switch"]), {}, namespace), /UNOBSERVED/);
});

test("harness arguments cannot override Cargo output/config and nextest requires run", () => {
  const value = sessionPlan({ schema: 1, arms: [{ steps: [step("clippy", ["--", "-D", "warnings"]),
    step("test", ["--", "--nocapture"]), step("nextest", ["run", "--profile", "ci"])] }] });
  for (const candidate of value.arms[0].steps) requireKnownStep(candidate, {}, namespace);
  assert.throws(() => sessionPlan({ schema: 1, arms: [{ steps: [step("nextest", ["list"])] }] }));
  assert.throws(() => sessionPlan({ schema: 1, arms: [{ steps: [step("test", ["--", "--config", "private"])] }] }));
});
