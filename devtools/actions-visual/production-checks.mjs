// Actual production DOM/HTMX behavior against one fixed synthetic run.
import assert from "node:assert/strict";
import { productionOrigin, productionRun, allowedProductionRequest } from "./production-policy.mjs";
import { contain } from "./containment.mjs";
import { postRevocationPoll } from "./production-poll.mjs";

async function context(browser, fixture, { width, rtl = false, dark = false, javascript = true }) {
  // Chromium also derives Accept-Language from its emulated browser locale.
  // Keep that setting consistent with the requested diagnostic catalog.
  const result = await browser.newContext({ viewport: { width, height: 1000 }, locale: rtl ? "ar-XB" : "en-US", timezoneId: "UTC",
    reducedMotion: "reduce", deviceScaleFactor: 1, serviceWorkers: "block", javaScriptEnabled: javascript,
    colorScheme: dark ? "dark" : "light", extraHTTPHeaders: { "Accept-Language": rtl ? "ar-XB" : "en" } });
  result.setDefaultTimeout(10000); result.setDefaultNavigationTimeout(10000);
  try {
    await fixture.authenticate(result);
    await result.addCookies([{ name: "rbit-theme", value: dark ? "dna-dark" : "dna-day", url: productionOrigin }]);
    const faults = await contain(result, allowedProductionRequest);
    return { context: result, faults };
  } catch (error) {
    try { await result.close(); } catch { /* The outer browser owner still closes on this failed setup. */ }
    throw error;
  }
}
async function openRun(page) {
  const response = await page.goto(productionOrigin + productionRun, { waitUntil: "networkidle" });
  assert.equal(response.status(), 200);
  assert.ok((await response.allHeaders())["content-security-policy"]);
}
async function polled(page) {
  const old = await page.locator("#run").elementHandle();
  try { await page.waitForFunction(node => !node.isConnected, old, { timeout: 10000 }); }
  finally { await old.dispose(); }
}
async function retainDraft(page) {
  const checkbox = page.locator('form[data-run-review="approval"] input[name="trust_author"]');
  await checkbox.check(); await checkbox.focus();
  await polled(page); // Wait for real three-second HTMX replacement, not a mocked event.
  assert.equal(await checkbox.isChecked(), true);
  assert.equal(await checkbox.evaluate(node => node === document.activeElement), true);
}
async function checkPage(page, faults, javascript = true) {
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1), false);
  // Init scripts cannot observe events when document JavaScript is disabled.
  if (javascript) assert.equal(await page.evaluate(() => window.__visualCspViolations), 0);
  assert.deepEqual(faults, []);
}
async function session(browser, fixture, options, operation) {
  const owned = await context(browser, fixture, options);
  let failure;
  try { await operation(await owned.context.newPage(), owned.faults); }
  catch (error) { failure = error; }
  try { await owned.context.close(); } catch { failure ??= Error("PRODUCTION_CONTEXT_CLOSE_FAILED"); }
  if (failure) throw failure;
}

export async function productionChecks(browser, fixture, capture, stage) {
  stage("desktop-navigation");
  await session(browser, fixture, { width: 1440 }, async (page, faults) => {
    await openRun(page);
    assert.equal(await page.locator('form[data-run-review="approval"]').count(), 1);
    await checkPage(page, faults);
    await capture(page, "production-run-desktop-light");
    stage("desktop-htmx-draft"); await retainDraft(page);
    await checkPage(page, faults); fixture.healthy();
  });
  stage("nojs-native-controls");
  await session(browser, fixture, { width: 360, javascript: false }, async (page, faults) => {
    await openRun(page);
    const form = page.locator('form[data-run-review="approval"]');
    assert.equal(await form.getAttribute("method"), "post");
    assert.equal(await form.getAttribute("action"), `${productionRun}/approve`);
    assert.equal(await form.locator('input[name="trust_author"]').isVisible(), true);
    await checkPage(page, faults, false);
    await capture(page, "production-run-mobile-nojs");
  });
  stage("mobile-rtl-navigation");
  await session(browser, fixture, { width: 360, rtl: true, dark: true }, async (page, faults) => {
    await openRun(page);
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
