// Real runner-backed step windows and gzip downloads; only fixed private completion.
import assert from "node:assert/strict";
import { productionJob } from "./production-policy.mjs";
import { session, checkPage, openPage, replaced } from "./production-page.mjs";
import { rawLogEvidence } from "./production-log-evidence.mjs";

async function boundedPreview(page) {
  const step = page.locator('details[data-step="0"]');
  const rows = step.locator(".log-line");
  assert.equal(await rows.count(), 1000);
  assert.equal(await step.locator('.log-num[data-line="206"]').count(), 1);
  assert.equal(await step.locator('.log-num[data-line="1205"]').count(), 1);
  assert.equal(await step.locator('.log-num[data-line="1"]').count(), 0);
  assert.equal(await step.locator("synthetic").count(), 0);
  assert.ok((await step.locator(".log-text").allTextContents()).includes("line 0500: <synthetic> & output"));
}

export async function jobChecks(browser, fixture, capture, stage, record) {
  let live;
  stage("job-live-desktop");
  await session(browser, fixture, { width: 1440 }, async (page, faults) => {
    await openPage(page, productionJob);
    assert.equal(await page.locator("#job h1").textContent(), "Build / stable <synthetic> & matrix");
    await boundedPreview(page);
    const summary = page.locator('details[data-step="0"] > summary');
    await summary.focus(); await page.keyboard.press("Enter");
    assert.equal(await page.locator('details[data-step="0"]').getAttribute("open"), "");
    // Keep the image compact after proving the full preview and keyboard opening.
    await page.locator('details[data-step="0"] > summary').click();
    live = await rawLogEvidence(page, false);
    await checkPage(page, faults); await capture(page, "production-job-live-desktop");
    stage("step-longer-window");
    await openPage(page, `${productionJob}/logs?step=0`);
    assert.equal(await page.locator("#step .log-line").count(), 1205);
    assert.equal(await page.locator('#step .log-num[data-line="1"]').count(), 1);
    await checkPage(page, faults);
  });
  stage("job-mobile-rtl");
  await session(browser, fixture, { width: 360, rtl: true, dark: true }, async (page, faults) => {
    await openPage(page, productionJob);
    assert.equal(await page.locator("#job").getAttribute("lang"), "ar-XB");
    assert.equal(await page.locator("#job").getAttribute("dir"), "rtl");
    assert.equal(await page.locator("html").getAttribute("data-theme"), "dna-dark");
    await boundedPreview(page);
    const log = await page.locator('details[data-step="1"] .log-console').evaluate(grid => {
      const number = grid.querySelector(".log-num").getBoundingClientRect();
      const content = grid.querySelector(".log-text").getBoundingClientRect();
      return { direction: getComputedStyle(grid).direction, gutterBefore: number.right <= content.left,
        lines: [...grid.querySelectorAll(".log-text")].map(line => line.textContent) };
    });
    assert.equal(log.direction, "ltr", "machine logs retain LTR inside the localized RTL page");
    assert.equal(log.gutterBefore, true, "line-number gutters remain before their machine text");
    assert.deepEqual(log.lines, ["waiting for finish", "task token ***"]);
    await checkPage(page, faults);
    await capture(page, "production-job-live-mobile-rtl-dark");
  });
  stage("job-nojs-native-details");
  await session(browser, fixture, { width: 360, javascript: false }, async (page, faults) => {
    await openPage(page, productionJob);
    const step = page.locator('details[data-step="0"]');
    await step.locator("summary").click(); assert.equal(await step.getAttribute("open"), "");
    await step.locator("summary").click(); assert.equal(await step.getAttribute("open"), null);
    assert.equal(await page.locator(`a[href="${productionJob}/logs/raw"]`).getAttribute("hx-boost"), "false");
    await checkPage(page, faults, false); await capture(page, "production-job-live-mobile-nojs");
  });
  stage("job-private-finish");
  await session(browser, fixture, { width: 1440 }, async (page, faults) => {
    // A running job's page tails its log and reloads the job section only when a step
    // or the job changes state (assets/actions-log.js). Finishing the job is that reload:
    // a step the viewer closed stays closed, though a finished failure opens it by default.
    await openPage(page, productionJob);
    await page.locator('details[data-step="0"] > summary').focus();
    await page.keyboard.press("Enter"); await page.keyboard.press("Enter");
    assert.equal(await page.locator('details[data-step="0"]').getAttribute("open"), null);
    await fixture.finishLogJob();
    await replaced(page, "#job");
    assert.equal(await page.locator('details[data-step="0"]').getAttribute("open"), null, "the viewer's choice survives the reload");
    await openPage(page, productionJob);
    assert.equal(await page.locator("#job").getAttribute("hx-trigger"), null);
    const rerun = page.locator(`form[action="${productionJob}/rerun"]`);
    assert.equal(await rerun.getAttribute("method"), "post");
    await boundedPreview(page);
    // Finished failure opens the failed step by default; close for compact image.
    await page.locator('details[data-step="0"] > summary').click();
    const final = await rawLogEvidence(page, true);
    assert.equal(final.sha256, live.sha256); assert.equal(final.bytes, live.bytes);
    record({ live, final, previewLines: 1000, stepLines: 1205 });
    await checkPage(page, faults); await capture(page, "production-job-finished-desktop");
  });
}
