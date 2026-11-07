// Contract fakes exercise assertion/lifecycle failures; they are not browser evidence.
import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { overviewChecks } from "../../devtools/actions-visual/production-overview.mjs";
import { jobChecks } from "../../devtools/actions-visual/production-job.mjs";
import { expectedLogText } from "../../devtools/actions-visual/production-log-evidence.mjs";
import { productionOrigin, productionOverview, productionJob } from "../../devtools/actions-visual/production-policy.mjs";

function model({ loseDraft = false, loseOpen = false, wrongRaw = false, wrongNative = false, mirroredLog = false, alteredLog = false } = {}) {
  const contexts = [], captures = [], stages = [], evidence = [];
  let finished = false;
  const fixture = { authenticate: async () => {}, healthy: () => {}, finishLogJob: async () => { finished = true; } };
  const browser = { newContext: async options => {
    const context = { closed: false, options, setDefaultTimeout() {}, setDefaultNavigationTimeout() {},
      addCookies: async () => {}, route: async () => {}, routeWebSocket: async () => {},
      on() {}, addInitScript: async () => {}, close: async () => { context.closed = true; } };
    contexts.push(context);
    context.newPage = async () => {
      let url, branch = "", focus, open = false;
      const page = {
        goto: async value => {
          url = new URL(value); assert.equal(url.origin, productionOrigin);
          branch = url.searchParams.get("branch") ?? "";
          open = finished && url.pathname === productionJob;
          return { status: () => 200, allHeaders: async () => ({ "content-security-policy": "script-src 'self'" }) };
        },
        keyboard: { press: async key => { assert.equal(key, "Enter"); assert.match(focus, /> summary/); open = !open; } },
        evaluate: async operation => {
          const source = operation.toString();
          if (source.includes("scrollWidth")) return false;
          if (source.includes("__visualCspViolations")) return 0;
          assert.ok(source.includes("await fetch"));
          const body = expectedLogText();
          return { status: 200, type: "text/plain; charset=utf-8", csp: "sandbox; default-src 'none'",
            encoding: finished ? "gzip" : null, bytes: Buffer.byteLength(body), lines: 1207,
            sha256: wrongRaw ? "0".repeat(64) : createHash("sha256").update(body).digest("hex") };
        },
        waitForNavigation: async () => {},
        waitForFunction: async (predicate, previous) => {
          previous.isConnected = false;
          if (loseDraft) { branch = ""; focus = null; }
          if (loseOpen) open = false;
          assert.equal(predicate(previous), true);
        },
      };
      function locator(selector) {
        return {
          locator: child => locator(`${selector} ${child}`),
          elementHandle: async () => ({ isConnected: true, dispose: async () => {} }),
          focus: async () => { focus = selector; },
          fill: async value => { assert.equal(selector, "#runs-branch"); branch = value; },
          inputValue: async () => branch,
          evaluate: async () => selector.endsWith(".code-lines")
            ? { direction: mirroredLog ? "rtl" : "ltr", gutterBefore: !mirroredLog,
              lines: ["waiting for finish", alteredLog ? "*** task token" : "task token ***"] }
            : focus === selector,
          click: async () => {
            if (selector.includes("summary")) open = !open;
            else { assert.match(selector, /button\[type="submit"\]/); url = new URL(productionOrigin + productionOverview + "?branch=main"); }
          },
          textContent: async () => { assert.equal(selector, "#job h1"); return "Build / stable <synthetic> & matrix"; },
          allTextContents: async () => { assert.match(selector, /\.line-code$/); return expectedLogText().split("\n").slice(205, 1205); },
          count: async () => {
            if (selector.includes('#runs a[href*=')) return url.searchParams.has("before") ? 1 : branch === "main" ? 14 : branch === "missing" ? 0 : 25;
            if (selector === '#runs a[rel="next"]') return url.searchParams.has("before") ? 0 : 1;
            if (selector.endsWith("synthetic")) return 0;
            if (selector.includes('data-line="1"')) return selector.startsWith("#step") ? 1 : 0;
            if (selector.includes("data-line=")) return 1;
            if (selector.endsWith(".code-line")) return selector.startsWith("#step") ? 1205 : 1000;
            throw Error(`Unexpected contract selector: ${selector}`);
          },
          getAttribute: async name => {
            if (name === "open") return open ? "" : null;
            if (name === "href") return `${productionOverview}?before=2`;
            if (name === "lang") return options.extraHTTPHeaders["Accept-Language"];
            if (name === "data-theme") return options.colorScheme === "dark" ? "dna-dark" : "dna-day";
            if (name === "dir") return options.extraHTTPHeaders["Accept-Language"] === "ar-XB" ? "rtl" : "ltr";
            if (name === "hx-trigger") return finished ? null : "every 3s";
            if (name === "hx-boost") return "false";
            if (name === "action") return productionOverview;
            if (name === "method") return selector.startsWith('form[role=') ? wrongNative ? "post" : "get" : "post";
            throw Error(`Unexpected contract attribute: ${name}`);
          },
        };
      }
      page.locator = locator;
      return page;
    };
    return context;
  } };
  const capture = async (_page, id) => { captures.push(id); };
  const stage = value => { stages.push(value); };
  return { contexts, captures, stages, evidence,
    overview: () => overviewChecks(browser, fixture, capture, stage),
    job: () => jobChecks(browser, fixture, capture, stage, value => evidence.push(value)) };
}

test("extension consumes real-page assertions for seven bounded candidates and closes every context", async () => {
  const fixture = model(); await fixture.overview(); await fixture.job();
  assert.equal(fixture.captures.length, 7);
  assert.equal(fixture.contexts.length, 7);
  assert.ok(fixture.contexts.every(context => context.closed && context.options.serviceWorkers === "block"));
  assert.equal(fixture.contexts.filter(context => !context.options.javaScriptEnabled).length, 2);
  assert.equal(fixture.evidence.length, 1);
  assert.equal(fixture.evidence[0].live.sha256, fixture.evidence[0].final.sha256);
  assert.doesNotMatch(JSON.stringify(fixture.evidence), /task token|line 0500|cookie|responseURL/);
});

test("lost filter drafts, lost step-open state and unproved raw data fail before candidate images", async () => {
  const draft = model({ loseDraft: true }); await assert.rejects(draft.overview());
  assert.equal(draft.captures.length, 0); assert.ok(draft.contexts.every(context => context.closed));
  for (const options of [{ loseOpen: true }, { wrongRaw: true }]) {
    const fixture = model(options); await assert.rejects(fixture.job());
    assert.equal(fixture.captures.length, 0); assert.ok(fixture.contexts.every(context => context.closed));
  }
  const native = model({ wrongNative: true }); await assert.rejects(native.overview());
  assert.ok(native.contexts.every(context => context.closed));
});

test("used driver reuses the same fixture and proves masked raw data before the first Job capture", async () => {
  const root = new URL("../../devtools/actions-visual/", import.meta.url);
  const driver = await readFile(new URL("production-run.mjs", root), "utf8");
  assert.ok(driver.includes("await overviewChecks(browser, fixture"));
  assert.ok(driver.includes("await jobChecks(browser, fixture"));
  assert.ok(driver.indexOf("await jobChecks") < driver.indexOf("await productionChecks"));
  const job = await readFile(new URL("production-job.mjs", root), "utf8");
  assert.ok(job.indexOf("await rawLogEvidence(page, false)") < job.indexOf('capture(page, "production-job-live-desktop")'));
  assert.doesNotMatch(job, /page\.request|browser\.request|APIRequestContext|\.post\(|\.route\(/);
});


test("real RTL acceptance rejects mirrored log gutters and changed machine text", async () => {
  for (const options of [{ mirroredLog: true }, { alteredLog: true }]) {
    const fixture = model(options);
    await assert.rejects(fixture.job(), assert.AssertionError);
    assert.deepEqual(fixture.captures, ["production-job-live-desktop"]);
    assert.ok(fixture.contexts.every(context => context.closed));
  }
});
