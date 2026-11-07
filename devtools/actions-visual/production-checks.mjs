// Actual production DOM/HTMX behavior against one fixed synthetic run.
import assert from "node:assert/strict";
import { productionRun } from "./production-policy.mjs";
import { session, checkPage, openPage, replaced } from "./production-page.mjs";
import { postRevocationPoll } from "./production-poll.mjs";

async function retainDraft(page) {
  const checkbox = page.locator('form[data-run-review="approval"] input[name="trust_author"]');
  await checkbox.check(); await checkbox.focus();
  await replaced(page, "#run"); // Wait for real three-second HTMX replacement, not a mocked event.
  assert.equal(await checkbox.isChecked(), true);
  assert.equal(await checkbox.evaluate(node => node === document.activeElement), true);
}

export async function productionChecks(browser, fixture, capture, stage) {
  stage("desktop-navigation");
  await session(browser, fixture, { width: 1440 }, async (page, faults) => {
    await openPage(page, productionRun);
    assert.equal(await page.locator('form[data-run-review="approval"]').count(), 1);
    await checkPage(page, faults);
    await capture(page, "production-run-desktop-light");
    stage("desktop-htmx-draft"); await retainDraft(page);
    await checkPage(page, faults); fixture.healthy();
  });
  stage("nojs-native-controls");
  await session(browser, fixture, { width: 360, javascript: false }, async (page, faults) => {
    await openPage(page, productionRun);
    const form = page.locator('form[data-run-review="approval"]');
    assert.equal(await form.getAttribute("method"), "post");
    assert.equal(await form.getAttribute("action"), `${productionRun}/approve`);
    assert.equal(await form.locator('input[name="trust_author"]').isVisible(), true);
    await checkPage(page, faults, false);
    await capture(page, "production-run-mobile-nojs");
  });
  stage("mobile-rtl-navigation");
  await session(browser, fixture, { width: 360, rtl: true, dark: true }, async (page, faults) => {
    await openPage(page, productionRun);
    stage("mobile-rtl-language");
    assert.equal(await page.locator("#run").getAttribute("lang"), "ar-XB");
    stage("mobile-rtl-direction");
    assert.equal(await page.locator("#run").getAttribute("dir"), "rtl");
    stage("mobile-htmx-draft"); await retainDraft(page);
    await checkPage(page, faults);
    await capture(page, "production-run-mobile-rtl-dark");
    stage("permission-removal"); await fixture.revokeWriter();
    await postRevocationPoll(page);
    assert.equal(await page.locator('form[data-run-review="approval"]').count(), 0);
    assert.equal(await page.locator(`form[action="${productionRun}/approve"]`).count(), 0);
    assert.equal(await page.locator("#run h1").count(), 1, "public reader retains the real run document");
    await checkPage(page, faults); fixture.healthy();
    await capture(page, "production-run-mobile-revoked");
  });
}
