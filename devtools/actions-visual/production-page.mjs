// Shared contained production page ownership and fixed-route navigation.
import assert from "node:assert/strict";
import { productionOrigin, allowedProductionRequest } from "./production-policy.mjs";
import { contain } from "./containment.mjs";

async function context(browser, fixture, { width, rtl = false, dark = false, javascript = true }) {
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
export async function checkPage(page, faults, javascript = true) {
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1), false);
  // Init scripts cannot observe events when document JavaScript is disabled.
  if (javascript) assert.equal(await page.evaluate(() => window.__visualCspViolations), 0);
  assert.deepEqual(faults, []);
}
export async function session(browser, fixture, options, operation) {
  const owned = await context(browser, fixture, options);
  let failure;
  try { await operation(await owned.context.newPage(), owned.faults); }
  catch (error) { failure = error; }
  try { await owned.context.close(); } catch { failure ??= Error("PRODUCTION_CONTEXT_CLOSE_FAILED"); }
  if (failure) throw failure;
}

export async function openPage(page, path) {
  const url = productionOrigin + path;
  assert.equal(allowedProductionRequest(url, "GET"), true, "FIXTURE_READ_ROUTE_REFUSED");
  const response = await page.goto(url, { waitUntil: "networkidle" });
  assert.equal(response.status(), 200);
  assert.ok((await response.allHeaders())["content-security-policy"]);
}

export async function replaced(page, selector) {
  assert.ok(["#run", "#runs", "#job"].includes(selector), "FIXTURE_REFRESH_ROOT_REFUSED");
  const old = await page.locator(selector).elementHandle();
  try { await page.waitForFunction(node => !node.isConnected, old, { timeout: 10000 }); }
  finally { await old.dispose(); }
}
