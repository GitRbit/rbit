// Real overview pages: native GET filters stay outside the naturally refreshed list.
import assert from "node:assert/strict";
import { productionOverview } from "./production-policy.mjs";
import { session, checkPage, openPage, replaced } from "./production-page.mjs";

async function counts(page, count) {
  assert.equal(await page.locator('#runs a[href*="/actions/runs/"]').count(), count);
}

export async function overviewChecks(browser, fixture, capture, stage) {
  stage("overview-desktop");
  await session(browser, fixture, { width: 1440 }, async (page, faults) => {
    await openPage(page, productionOverview);
    assert.equal(await page.locator("html").getAttribute("data-theme"), "dna-day");
    await counts(page, 25);
    const branch = page.locator("#runs-branch");
    await branch.fill("draft branch"); await branch.focus();
    await replaced(page, "#runs");
    assert.equal(await branch.inputValue(), "draft branch");
    assert.equal(await branch.evaluate(node => node === document.activeElement), true);
    await checkPage(page, faults);
    await capture(page, "production-overview-desktop-draft");
    const older = await page.locator('#runs a[rel="next"]').getAttribute("href");
    assert.equal(older, `${productionOverview}?before=2`);
    await openPage(page, older); await counts(page, 1);
    assert.equal(await page.locator('#runs a[rel="next"]').count(), 0);
    stage("overview-filter-empty");
    await openPage(page, `${productionOverview}?branch=missing`); await counts(page, 0);
    assert.equal(await page.locator('#runs-branch').inputValue(), "missing");
    await checkPage(page, faults);
  });
  stage("overview-mobile-rtl");
  await session(browser, fixture, { width: 360, rtl: true, dark: true }, async (page, faults) => {
    await openPage(page, productionOverview);
    assert.equal(await page.locator("#runs").getAttribute("lang"), "ar-XB");
    assert.equal(await page.locator("#runs").getAttribute("dir"), "rtl");
    assert.equal(await page.locator("html").getAttribute("data-theme"), "dna-dark");
    await counts(page, 25); await checkPage(page, faults);
    await capture(page, "production-overview-mobile-rtl-dark");
  });
  stage("overview-nojs-native-filter");
  await session(browser, fixture, { width: 360, javascript: false }, async (page, faults) => {
    await openPage(page, productionOverview);
    const form = page.locator('form[role="search"]');
    assert.equal(await form.getAttribute("method"), "get");
    assert.equal(await form.getAttribute("action"), productionOverview);
    await page.locator("#runs-branch").fill("main");
    await Promise.all([page.waitForNavigation({ waitUntil: "networkidle" }), form.locator('button[type="submit"]').click()]);
    await counts(page, 14);
    assert.equal(await page.locator("#runs-branch").inputValue(), "main");
    await checkPage(page, faults, false);
    await capture(page, "production-overview-mobile-nojs-filtered");
  });
}
