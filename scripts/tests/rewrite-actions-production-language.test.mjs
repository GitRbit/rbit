import assert from "node:assert/strict";
import test from "node:test";
import { allowedProductionRequest, productionOrigin, productionLanguage, productionExplore } from "../../devtools/actions-visual/production-policy.mjs";
import { contain } from "../../devtools/actions-visual/containment.mjs";
import { languageChecks } from "../../devtools/actions-visual/production-language.mjs";

const validForm = { body: "locale=es", contentType: "application/x-www-form-urlencoded", origin: productionOrigin };
test("browser mutation authority is limited to the exact synthetic language form", () => {
  for (const body of ["locale=", "locale=en", "locale=es"]) {
    assert.equal(allowedProductionRequest(productionOrigin + productionLanguage, "POST", { ...validForm, body }), true);
  }
  for (const contentType of ["application/x-www-form-urlencoded;charset=utf-8", "APPLICATION/X-WWW-FORM-URLENCODED; CHARSET=UTF-8"]) {
    assert.equal(allowedProductionRequest(productionOrigin + productionLanguage, "POST", { ...validForm, contentType }), true);
  }
  for (const field of [{ body: "locale=\n" }, { body: "locale=\r" }, { body: "locale=\r\n" }, { body: "locale=en\n" },
    { body: "locale=es\r\n" }, { body: "locale=\0" }, { body: "locale=\t" }, { body: "locale=\nen" }, { body: "locale=ar-XB" }, { body: "locale=es&next=/private" }, { body: "locale=es&locale=en" },
    { body: "locale=ES" }, { body: "locale=%65s" }, { body: "x".repeat(10000) }, { body: null },
    { contentType: "application/x-www-form-urlencoded\n" }, { contentType: "application/x-www-form-urlencoded\r\n" },
    { contentType: "application/x-www-form-urlencoded;\ncharset=utf-8" }, { contentType: "application/x-www-form-urlencoded;\tcharset=utf-8" },
    { contentType: "application/x-www-form-urlencoded; charset=utf-8\0" }, { contentType: "application/json" }, { origin: "null" }, { origin: "https://example.invalid" }]) {
    assert.equal(allowedProductionRequest(productionOrigin + productionLanguage, "POST", { ...validForm, ...field }), false);
  }
  for (const path of ["/settings/account/password", "/logout", "/alice/rocket/actions/runs/1/approve", "/api/v1/users",
    productionLanguage + "?saved=1", productionLanguage + "#form"]) {
    assert.equal(allowedProductionRequest(productionOrigin + path, "POST", validForm), false);
  }
  for (const method of ["PUT", "DELETE", "PATCH", "OPTIONS"]) {
    assert.equal(allowedProductionRequest(productionOrigin + productionLanguage, method, validForm), false);
  }
  for (const url of ["http://localhost:4189/settings/language", "http://user@127.0.0.1:4189/settings/language", "https://example.invalid/settings/language"]) {
    assert.equal(allowedProductionRequest(url, "POST", validForm), false);
  }
  for (const path of [productionLanguage, productionLanguage + "?saved=1", productionExplore]) {
    assert.equal(allowedProductionRequest(productionOrigin + path, "GET"), true);
  }
  for (const path of [productionLanguage + "?saved=1&saved=1", productionLanguage + "?next=/private", productionExplore + "?q=private"]) {
    assert.equal(allowedProductionRequest(productionOrigin + path, "GET"), false);
  }
});

test("request interception reads only form body and two validation headers, retaining sanitized faults", async () => {
  let handler, continued = 0, aborted = 0;
  const headers = [];
  const context = { route: async (_pattern, operation) => { handler = operation; }, routeWebSocket: async () => {},
    on: () => {}, addInitScript: async () => {} };
  const faults = await contain(context, allowedProductionRequest, async () => { continued++; });
  const route = body => ({ request: () => ({ url: () => productionOrigin + productionLanguage, method: () => "POST", postData: () => body,
    headers: () => assert.fail("Do not enumerate cookie/authorization headers"),
    headerValue: async name => { headers.push(name); return name === "origin" ? productionOrigin : validForm.contentType; } }),
    continue: () => { continued++; }, abort: () => { aborted++; } });
  await handler(route("locale=es"));
  await handler(route("locale=es&private=value"));
  assert.equal(continued, 1); assert.equal(aborted, 1);
  assert.deepEqual(headers, ["content-type", "origin", "content-type", "origin"]);
  assert.deepEqual(faults, ["unexpected-request"]);
});

function browserModel({ wrongSave = false, staleSwap = false, noJsFailure = false } = {}) {
  let account = null;
  const contexts = [], captures = [], stages = [];
  const browser = { newContext: async options => {
    const browserLanguage = options.extraHTTPHeaders["Accept-Language"];
    const context = { options, closed: false, setDefaultTimeout: () => {}, setDefaultNavigationTimeout: () => {},
      addCookies: async () => {}, route: async () => {}, routeWebSocket: async () => {}, on: () => {}, addInitScript: async () => {},
      close: async () => { context.closed = true; }, newPage: async () => {
        let url, locale, selected, marker = false, navigate, responseWait;
        const response = (path, language, refresh = false) => ({ status: () => 200, url: () => productionOrigin + path,
          allHeaders: async () => ({ "content-language": language, "content-security-policy": "default-src 'self'" }),
          headerValue: async name => ({ "content-language": language, "content-security-policy": "default-src 'self'", ...(refresh ? { "hx-refresh": "true" } : {}) })[name] ?? null,
          request: () => ({ method: () => "GET", headerValue: async name => name === "hx-request" ? "true" : null }) });
        const full = path => {
          url = productionOrigin + path; locale = account ?? browserLanguage; selected = account ?? ""; marker = false;
          return response(path, locale);
        };
        const page = { goto: async value => full(new URL(value).pathname), url: () => url,
          waitForNavigation: () => new Promise(resolve => { navigate = resolve; }),
          waitForResponse: predicate => new Promise(resolve => { responseWait = { predicate, resolve }; }),
          waitForURL: async expected => assert.equal(url, expected),
          goBack: async () => { url = productionOrigin + productionLanguage + "?saved=1"; selected = account ?? ""; },
          evaluate: async operation => {
            const source = operation.toString();
            if (source.includes("scrollWidth")) return false;
            if (source.includes("__visualCspViolations")) return 0;
            if (source.includes("__visualLocaleDocument = true")) { marker = true; return; }
            assert.ok(source.includes("__visualLocaleDocument === true")); return marker;
          },
        };
        function locator(selector) {
          return { first: () => locator(selector), locator: child => locator(selector + " " + child), waitFor: async () => {},
            inputValue: async () => selected, selectOption: async value => { selected = value; },
            getAttribute: async name => {
              if (name === "lang") return locale;
              if (name === "dir") return "ltr";
              if (name === "data-catalog-key") return "1." + locale;
              if (name === "method") return "post";
              if (name === "hx-boost") return "false";
              throw Error("Unexpected fixture attribute");
            },
            click: async () => {
              if (selector.includes('button[type="submit"]')) {
                assert.equal(allowedProductionRequest(productionOrigin + productionLanguage, "POST", { ...validForm, body: "locale=" + selected }), true);
                if (!(noJsFailure && options.javaScriptEnabled === false)) account = selected || null;
                if (wrongSave) account = "en";
                navigate(full(productionLanguage + "?saved=1")); return;
              }
              assert.ok(selector.includes('a[href="/explore"]'));
              const language = account ?? browserLanguage, refresh = language !== locale;
              const value = response(productionExplore, language, refresh);
              assert.equal(await responseWait.predicate(value), true);
              if (refresh) {
                if (!staleSwap) navigate(full(productionLanguage + "?saved=1"));
                else navigate(response(productionLanguage, locale));
              } else url = productionOrigin + productionExplore;
              responseWait.resolve(value);
            },
          };
        }
        page.locator = locator; return page;
      } };
    contexts.push(context); return context;
  } };
  return { contexts, captures, stages,
    run: () => languageChecks(browser, { authenticate: async () => {}, healthy: () => {} }, async (_page, id) => captures.push(id), value => stages.push(value)) };
}

test("native language saves, no-JS reset and stale-document navigation use the real acceptance assertions", async () => {
  const model = browserModel(); await model.run();
  assert.equal(model.captures.length, 3);
  assert.equal(model.contexts.length, 2);
  assert.equal(model.contexts[1].options.javaScriptEnabled, false);
  assert.equal(model.contexts[1].options.extraHTTPHeaders["Accept-Language"], "es");
  assert.ok(model.contexts.every(context => context.closed));
  assert.ok(model.stages.includes("language-refreshed-navigation-no-loop"));
});

test("wrong saved catalog, broken no-JS save or stale cross-catalog swap fails the candidate", async () => {
  for (const failure of [{ wrongSave: true }, { noJsFailure: true }, { staleSwap: true }]) {
    const model = browserModel(failure);
    await assert.rejects(model.run(), assert.AssertionError);
    assert.ok(model.contexts.every(context => context.closed));
  }
});
