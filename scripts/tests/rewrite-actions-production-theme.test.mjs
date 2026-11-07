import assert from "node:assert/strict";
import test from "node:test";
import { checkSpanishPicker } from "../../devtools/actions-visual/production-theme.mjs";
import { allowedProductionRequest, productionOrigin } from "../../devtools/actions-visual/production-policy.mjs";

function pageFixture({ english = false, wrongCount = false } = {}) {
  let filtered = false, query = "", visible = false;
  const locator = selector => ({
    first() { return this; }, locator: child => locator(child),
    async getAttribute(name) {
      if (name === "href") return "/settings/appearance";
      if (name === "lang") return english ? "en" : "es";
      if (name === "aria-label") return selector.includes("search") ? "Buscar temas" : "Elegir un tema";
      if (name === "aria-pressed") return String(filtered);
      throw Error("Unexpected fixture attribute");
    },
    async click() { if (selector.includes("colorblind")) filtered = true; else visible = true; },
    async waitFor() { assert.equal(visible, true); },
    async innerText() {
      if (selector.includes("variant")) return "Daltonismo";
      if (selector.includes("empty")) return "Ningún tema coincide con tu búsqueda.";
      return query ? "0 temas" : wrongCount ? "3 temas" : "2 temas";
    },
    async count() { return 2; }, async fill(value) { query = value; },
    async press(key) { assert.equal(key, "Escape"); visible = false; },
    async isVisible() { return visible; },
  });
  return { locator };
}
test("actual picker candidate exercises translated search and unchanged Colorblind filtering", async () => {
  await checkSpanishPicker(pageFixture());
  for (const options of [{ english: true }, { wrongCount: true }]) {
    await assert.rejects(checkSpanishPicker(pageFixture(options)), assert.AssertionError);
  }
});
test("picker acceptance adds only its exact read endpoint, never theme writes", () => {
  assert.equal(allowedProductionRequest(productionOrigin + "/themes/picker", "GET"), true);
  for (const path of ["/themes", "/themes/picker?locale=es", "/themes/picker#other"]) {
    assert.equal(allowedProductionRequest(productionOrigin + path, "GET"), false);
    assert.equal(allowedProductionRequest(productionOrigin + path, "POST"), false);
  }
});
