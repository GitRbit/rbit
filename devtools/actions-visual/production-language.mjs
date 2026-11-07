// Native synthetic account language saves plus actual HTMX/history boundaries.
import assert from "node:assert/strict";
import { languageProbe } from "./production-language-evidence.mjs";
import { productionOrigin, productionLanguage, productionExplore } from "./production-policy.mjs";
import { session, openPage, checkPage } from "./production-page.mjs";

const formSelector = 'form[action="/settings/language"]';
async function language(page, tag, selected) {
  assert.equal(await page.locator("html").getAttribute("lang"), tag);
  assert.equal(await page.locator("html").getAttribute("data-catalog-key"), `1.${tag}`);
  assert.equal(await page.locator("#language").getAttribute("lang"), tag);
  assert.equal(await page.locator("#language").getAttribute("dir"), "ltr");
  assert.equal(await page.locator("#preferred-language").inputValue(), selected);
}
async function nativeSave(page, selected, expected) {
  assert.ok(["", "en", "es"].includes(selected));
  const form = page.locator(formSelector);
  assert.equal(await form.getAttribute("method"), "post");
  assert.equal(await form.getAttribute("hx-boost"), "false");
  await page.locator("#preferred-language").selectOption(selected);
  const navigation = page.waitForNavigation({ waitUntil: "networkidle" });
  await form.locator('button[type="submit"]').click();
  const response = await navigation;
  assert.equal(response.status(), 200);
  assert.equal(page.url(), productionOrigin + productionLanguage + "?saved=1");
  assert.equal(await response.headerValue("content-language"), expected);
  assert.ok(await response.headerValue("content-security-policy"));
  assert.equal(await response.headerValue("clear-site-data"), null);
  await language(page, expected, selected);
}
function navigationResponse(page) {
  return page.waitForResponse(async response => {
    const url = new URL(response.url());
    if (url.origin !== productionOrigin || url.pathname !== productionExplore) return false;
    const request = response.request();
    return request.method() === "GET" && await request.headerValue("hx-request") === "true"
      && await request.headerValue("hx-preloaded") === null;
  });
}
async function explore(page, expected, phase, stage, record) {
  const observe = languageProbe(phase, stage, record);
  const response = navigationResponse(page);
  await page.locator('.site-header a[href="/explore"]').first().click();
  const completed = await response;
  const status = completed.status();
  observe("status", status, 200);
  assert.equal(status, 200);
  const contentLanguage = await completed.headerValue("content-language");
  observe("content-language", contentLanguage, expected);
  assert.equal(contentLanguage, expected);
  observe("url", false, true);
  await page.waitForURL(productionOrigin + productionExplore);
  observe("url", true, true);
  const documentLanguage = await page.locator("html").getAttribute("lang");
  observe("document-language", documentLanguage, expected);
  assert.equal(documentLanguage, expected);
  const selectedLanguage = await page.locator("#explore-results").getAttribute("lang");
  observe("selected-language", selectedLanguage, expected);
  assert.equal(selectedLanguage, expected);
  const marker = await page.evaluate(() => window.__visualLocaleDocument === true);
  observe("document-marker", marker, true);
  assert.equal(marker, true, "ordinary same-catalog navigation stays in-document");
}

export async function languageChecks(browser, fixture, capture, stage, record = () => {}) {
  stage("language-native-spanish-save");
  await session(browser, fixture, { width: 1440 }, async (page, faults) => {
    await openPage(page, productionLanguage);
    await language(page, "en", "");
    await nativeSave(page, "es", "es");
    await checkPage(page, faults); fixture.healthy();
    await capture(page, "production-language-desktop-spanish");
    await page.evaluate(() => { window.__visualLocaleDocument = true; });
    stage("language-spanish-htmx-navigation");
    await explore(page, "es", "spanish", stage, record);
    await checkPage(page, faults);
    await capture(page, "production-explore-desktop-spanish");
    stage("language-spanish-history-back");
    await page.goBack();
    await page.locator("#language").waitFor();
    await language(page, "es", "es");

    stage("language-mobile-native-browser-reset");
    await session(browser, fixture, { width: 360, spanish: true, javascript: false }, async (mobile, mobileFaults) => {
      await openPage(mobile, productionLanguage);
      await language(mobile, "es", "es");
      await nativeSave(mobile, "", "es");
      await checkPage(mobile, mobileFaults, false);
      await capture(mobile, "production-language-mobile-spanish-nojs-reset");
    });

    // The old Spanish document has an English browser preference. A real save in
    // the other context reset the account preference; its next natural navigation
    // must refresh the document once instead of swapping English into Spanish.
    stage("language-stale-document-refresh");
    const staleResponse = navigationResponse(page);
    const refreshed = page.waitForNavigation({ waitUntil: "networkidle" });
    await page.locator('.site-header a[href="/explore"]').first().click();
    assert.equal(await (await staleResponse).headerValue("hx-refresh"), "true");
    assert.equal((await refreshed).status(), 200);
    await language(page, "en", "");
    assert.equal(await page.evaluate(() => window.__visualLocaleDocument === true), false, "cross-catalog navigation creates a new document");
    await page.evaluate(() => { window.__visualLocaleDocument = true; });
    stage("language-refreshed-navigation-no-loop");
    await explore(page, "en", "refreshed", stage, record);
    await checkPage(page, faults); fixture.healthy();
  });
}
