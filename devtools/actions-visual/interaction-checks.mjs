// Existing synthetic assertions, separated from browser setup and image capture.
import assert from "node:assert/strict";
import { origin } from "./policy.mjs";
import { logControls, noJavascript } from "./interactions.mjs";
import { interactionStep as step } from "./interaction-evidence.mjs";

export async function interactionChecks(context) {
  const owned = await step("javascript.context", () => context({ viewport: { width: 1440, height: 1000 } }));
  let failure;
  try {
    const page = await step("javascript.page", () => owned.context.newPage());
    await step("javascript.job-navigation", () => page.goto(origin + "/job?state=running"));
    await logControls(page);
    await step("javascript.search-navigation", () => page.goto(origin + "/job?q=error"));
    await step("javascript.match-next", () => page.locator('[data-match="next"]').click());
    await step("javascript.match-feedback", () => page.locator("[data-log-feedback]").filter({ hasText: /Match \d+ of/ }).waitFor());
    for (const [state, viewer, visible] of [
      ["approval", "writer", true], ["approval", "reader", false],
      ["deployment", "reviewer", true], ["deployment", "writer", false],
    ]) {
      await step(`${state}.${viewer}`, async () => {
        await page.goto(`${origin}/run?state=${state}&viewer=${viewer}`);
        assert.equal(await page.locator(`button[value="${state === "approval"
          ? "approve-run" : "approve-deployment"}"]`).count() > 0, visible);
      });
    }
    await step("javascript.containment", () => assert.deepEqual(owned.faults, []));
  } catch (error) { failure = error; }
  try { await step("javascript.close", () => owned.context.close()); }
  catch (error) { failure ??= error; }
  if (failure) throw failure;

  const plain = await step("nojs.context", () => context({ javaScriptEnabled: false }));
  try {
    await noJavascript(await step("nojs.page", () => plain.context.newPage()));
    await step("nojs.containment", () => assert.deepEqual(plain.faults, []));
  } catch (error) { failure = error; }
  try { await step("nojs.close", () => plain.context.close()); }
  catch (error) { failure ??= error; }
  if (failure) throw failure;
}
