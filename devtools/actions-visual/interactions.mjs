import assert from "node:assert/strict";
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
  const before = await ids();
  const simulate = page.locator("[data-simulate]");
  await simulate.focus();
  await page.locator("#log-window").evaluate(node => { node.scrollTop = 20; });
  const scroll = await page.locator("#log-window").evaluate(node => node.scrollTop);
  await simulate.press("Enter");
  assert.deepEqual(await ids(), before);
  assert.equal(await simulate.evaluate(node => node === document.activeElement), true);
  assert.equal(await page.locator("#log-window").evaluate(node => node.scrollTop), scroll);
  await page.locator("[data-follow]").click();
  assert.equal((await ids()).length, 80);
  assert.equal((await ids()).at(-1), "L50001");
  await page.locator("[data-copy]").click();
  await page.locator("[data-log-feedback]").filter({ hasText: /Copied|Copy is unavailable/ }).waitFor();
  const summary = page.locator('[data-step="5"] > summary');
  await summary.focus();
  await page.keyboard.press("Space");
  assert.equal(await page.locator('[data-step="5"]').evaluate(node => node.open), false);
  await page.keyboard.press("Enter");
  assert.equal(await page.locator('[data-step="5"]').evaluate(node => node.open), true);
}
export async function noJavascript(page) {
  await page.goto("http://127.0.0.1:4187/overview?workflow=1");
  await page.getByLabel("Branch", { exact: true }).fill("main");
  await page.getByRole("button", { name: "Filter", exact: true }).click();
  assert.equal(new URL(page.url()).searchParams.get("branch"), "main");
  await page.goto("http://127.0.0.1:4187/job");
  assert.equal(await page.locator("[data-follow]").isVisible(), false);
  await page.getByRole("link", { name: "Show earlier lines" }).click();
  assert.equal(await page.locator(".log-lines").getAttribute("start"), "49841");
  const summary = page.locator('[data-step="5"] > summary');
  await summary.focus(); await page.keyboard.press("Space");
  assert.equal(await page.locator('[data-step="5"]').evaluate(node => node.open), false);
}
