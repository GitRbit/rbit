import assert from "node:assert/strict";
import { interactionStep as step } from "./interaction-evidence.mjs";
export async function keyboard(page) {
  await page.keyboard.press("Tab");
  const skip = page.locator(".skip-link");
  assert.equal(await skip.evaluate(node => node === document.activeElement), true);
  assert.equal(await skip.evaluate(node => getComputedStyle(node).outlineStyle !== "none"), true);
  await page.keyboard.press("Enter");
  assert.equal(await page.locator("#main").evaluate(node => node === document.activeElement), true);
}
export async function logControls(page) {
  const ids = () => page.locator(".log-line").evaluateAll(nodes => nodes.map(node => node.id));
  const before = await step("logs.initial-lines", ids);
  const simulate = page.locator("[data-simulate]");
  await step("logs.simulate-focus", () => simulate.focus());
  const scroll = await step("logs.scroll-setup", async () => {
    await page.locator("#log-window").evaluate(node => { node.scrollTop = 20; });
    return page.locator("#log-window").evaluate(node => node.scrollTop);
  });
  await step("logs.simulate-output", () => simulate.press("Enter"));
  await step("logs.paused-lines", async () => assert.deepEqual(await ids(), before));
  await step("logs.paused-focus", async () => assert.equal(await simulate.evaluate(node => node === document.activeElement), true));
  await step("logs.paused-scroll", async () => assert.equal(await page.locator("#log-window").evaluate(node => node.scrollTop), scroll));
  await step("logs.follow-click", () => page.locator("[data-follow]").click());
  await step("logs.follow-count", async () => assert.equal((await ids()).length, 80));
  await step("logs.follow-last-line", async () => assert.equal((await ids()).at(-1), "L50001"));
  await step("logs.copy-click", () => page.locator("[data-copy]").click());
  await step("logs.copy-feedback", () => page.locator("[data-log-feedback]").filter({ hasText: /Copied|Copy is unavailable/ }).waitFor());
  const summary = page.locator('[data-step="5"] > summary');
  await step("logs.disclosure-focus", () => summary.focus());
  await step("logs.disclosure-space", () => page.keyboard.press("Space"));
  await step("logs.disclosure-closed", async () => assert.equal(await page.locator('[data-step="5"]').evaluate(node => node.open), false));
  await step("logs.disclosure-enter", () => page.keyboard.press("Enter"));
  await step("logs.disclosure-open", async () => assert.equal(await page.locator('[data-step="5"]').evaluate(node => node.open), true));
}
export async function noJavascript(page) {
  await step("nojs.filter-navigation", () => page.goto("http://127.0.0.1:4187/overview?workflow=1"));
  await step("nojs.branch-fill", () => page.getByLabel("Branch", { exact: true }).fill("main"));
  await step("nojs.filter-submit", () => page.getByRole("button", { name: "Filter", exact: true }).click());
  await step("nojs.branch-query", () => assert.equal(new URL(page.url()).searchParams.get("branch"), "main"));
  await step("nojs.job-navigation", () => page.goto("http://127.0.0.1:4187/job"));
  await step("nojs.follow-hidden", async () => assert.equal(await page.locator("[data-follow]").isVisible(), false));
  await step("nojs.earlier-click", () => page.getByRole("link", { name: "Show earlier lines" }).click());
  await step("nojs.earlier-start", async () => assert.equal(await page.locator(".log-lines").getAttribute("start"), "49841"));
  const summary = page.locator('[data-step="5"] > summary');
  await step("nojs.disclosure-focus", () => summary.focus());
  await step("nojs.disclosure-space", () => page.keyboard.press("Space"));
  await step("nojs.disclosure-closed", async () => assert.equal(await page.locator('[data-step="5"]').evaluate(node => node.open), false));
}
