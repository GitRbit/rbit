// Real shared picker labels, search and Colorblind filter; no theme mutation.
import assert from "node:assert/strict";
import { productionLanguage } from "./production-policy.mjs";
import { session, openPage, checkPage } from "./production-page.mjs";

export async function checkSpanishPicker(page, capture = async () => {}) {
  const opener = page.locator('.site-header [data-theme-dialog-open]:visible').first();
  assert.equal(await opener.getAttribute("href"), "/settings/appearance");
  await opener.click();
  const dialog = page.locator("[data-theme-dialog]");
  await dialog.waitFor({ state: "visible" });
  assert.equal(await dialog.getAttribute("lang"), "es");
  assert.equal(await dialog.getAttribute("aria-label"), "Elegir un tema");
  const search = dialog.locator("[data-tpk-search]");
  assert.equal(await search.getAttribute("aria-label"), "Buscar temas");
  const filter = dialog.locator('[data-tpk-variant="colorblind"]');
  assert.equal(await filter.innerText(), "Daltonismo");
  await filter.click();
  assert.equal(await filter.getAttribute("aria-pressed"), "true");
  assert.equal(await dialog.locator("[data-tpk-count]").innerText(), "2 temas");
  assert.equal(await dialog.locator("[data-tpk-option]:visible").count(), 2);
  await search.fill("no-synthetic-theme-match");
  assert.equal(await dialog.locator("[data-tpk-count]").innerText(), "0 temas");
  assert.equal(await dialog.locator("[data-tpk-empty]").innerText(), "Ningún tema coincide con tu búsqueda.");
  await search.fill("");
  assert.equal(await dialog.locator("[data-tpk-count]").innerText(), "2 temas");
  await capture(page, "production-theme-picker-spanish-colorblind");
  await search.press("Escape");
  assert.equal(await dialog.isVisible(), false);
}

export async function themeChecks(browser, fixture, capture, stage) {
  stage("theme-picker-spanish-colorblind");
  await session(browser, fixture, { width: 1440, spanish: true }, async (page, faults) => {
    await openPage(page, productionLanguage);
    await checkSpanishPicker(page, capture);
    await checkPage(page, faults); fixture.healthy();
  });
}
