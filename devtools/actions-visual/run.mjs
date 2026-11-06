// CI-only: fixed loopback fixture runner; no general URL/browser CLI is exposed.
import { chromium } from "@playwright/test";
import assert from "node:assert/strict";
import { readFile, writeFile, readdir, stat } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import { preview } from "../../docs/design/prototypes/actions/server.mjs";
import { origin, image, cases, artifactBudget } from "./policy.mjs";
import { revision, sourceDigest, digest } from "./provenance.mjs";
import { verifySandbox } from "./sandbox.mjs";
import { contain } from "./containment.mjs";
import { setupFailure } from "./diagnostics.mjs";
import { launchEvidence } from "./launch-evidence.mjs";
import { environment } from "./environment.mjs";
import { keyboard, logControls, noJavascript } from "./interactions.mjs";
const output = "/artifacts";
const source = "/work/docs/design/prototypes/actions";
const started = performance.now();
const report = { schema: 1, status: "invalid", revision: revision(process.env.VISUAL_REVISION),
  image, architecture: process.arch, locale: "en-US", timezone: "UTC", deviceScaleFactor: 1,
  captures: [], failures: [] };
let server, browser, setupStage = "provenance";
async function context(options = {}) {
  const result = await browser.newContext({ locale: "en-US", timezoneId: "UTC",
    reducedMotion: "reduce", deviceScaleFactor: 1, serviceWorkers: "block", ...options });
  result.setDefaultTimeout(5000);
  result.setDefaultNavigationTimeout(10000);
  const faults = await contain(result);
  return { context: result, faults };
}
async function fixture(model) {
  const start = performance.now();
  const owned = await context({ viewport: model.viewport,
    colorScheme: model.theme === "dna-dark" ? "dark" : "light" });
  try {
    const page = await owned.context.newPage();
    await page.goto(origin + model.path, { waitUntil: "networkidle" });
    await page.evaluate(() => document.fonts.ready);
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth
      > document.documentElement.clientWidth + 1);
    await keyboard(page);
    const screenshot = await page.screenshot({ animations: "disabled", fullPage: true });
    artifactBudget([...report.captures.map(item => ({ name: `${item.id}.png`, bytes: item.bytes })),
      { name: `${model.id}.png`, bytes: screenshot.length }, { name: "reserved.json", bytes: 65536 }]);
    await writeFile(`${output}/${model.id}.png`, screenshot);
    report.captures.push({ ...model, bytes: screenshot.length, sha256: digest(screenshot), elapsedMs: Math.round(performance.now() - start) });
    assert.equal(overflow, false, "DOCUMENT_OVERFLOW");
    assert.equal(await page.evaluate(() => window.__visualCspViolations), 0, "CSP_VIOLATION");
    assert.deepEqual(owned.faults, [], "BROWSER_CONTAINMENT");
  } finally { await owned.context.close(); }
}
async function interactionChecks() {
  const owned = await context({ viewport: { width: 1440, height: 1000 } });
  try {
    const page = await owned.context.newPage();
    await page.goto(origin + "/job");
    await logControls(page);
    await page.goto(origin + "/job?q=error");
    await page.locator('[data-match="next"]').click();
    await page.locator("[data-log-feedback]").filter({ hasText: /Match \d+ of/ }).waitFor();
    for (const [state, viewer, visible] of [
      ["approval", "writer", true], ["approval", "reader", false],
      ["deployment", "reviewer", true], ["deployment", "writer", false],
    ]) {
      await page.goto(`${origin}/run?state=${state}&viewer=${viewer}`);
      assert.equal(await page.locator(`button[value="${state === "approval"
        ? "approve-run" : "approve-deployment"}"]`).count() > 0, visible);
    }
    assert.deepEqual(owned.faults, []);
  } finally { await owned.context.close(); }
  const plain = await context({ javaScriptEnabled: false });
  try {
    await noJavascript(await plain.context.newPage());
    assert.deepEqual(plain.faults, []);
  } finally { await plain.context.close(); }
}
try {
  if (process.platform !== "linux" || process.getuid?.() === 0) throw new Error("LINUX_NONROOT_REQUIRED");
  report.sourceSha256 = await sourceDigest(source);
  report.cssSha256 = digest(await readFile("/work/crates/rbit-web/assets/app.css"));
  report.lockSha256 = digest(await readFile(new URL("package-lock.json", import.meta.url)));
  setupStage = "preview-bind";
  server = await preview({ port: 4187 }); // Bind failure is fatal; never reuse an existing server.
  setupStage = "container-preflight";
  report.environment = await environment(chromium.executablePath());
  if (!report.environment.home.owned || !report.environment.home.writable) throw new Error("OWNED_HOME_NOT_READY");
  if (!report.environment.executable.exists || !report.environment.executable.executable
    || !report.environment.executable.bundled) throw new Error("BUNDLED_EXECUTABLE_NOT_READY");
  setupStage = "browser-launch";
  browser = await chromium.launch({ channel: "chromium", chromiumSandbox: true, headless: true, timeout: 15000 });
  setupStage = "sandbox-proof";
  report.sandbox = await verifySandbox(browser); // No timed capture before positive sandbox proof.
  report.browserVersion = browser.version();
  for (const model of cases()) {
    try { await fixture(model); }
    catch { report.failures.push({ fixture: model.id, code: "FIXTURE_ASSERTION_FAILED" }); }
  }
  try { await interactionChecks(); }
  catch { report.failures.push({ fixture: "interactions", code: "INTERACTION_ASSERTION_FAILED" }); }
  report.status = report.failures.length ? "failed" : "candidate-review-required";
} catch (error) {
  if (setupStage === "browser-launch") report.launch = launchEvidence(error);
  report.failures.push({ fixture: "setup", stage: setupStage, code: setupFailure(setupStage, error) });
} finally {
  try { if (browser) await browser.close(); }
  catch { report.status = "invalid"; report.failures.push({ fixture: "cleanup", code: "BROWSER_CLOSE_FAILED" }); }
  if (server) await new Promise(done => server.close(done));
  report.elapsedMs = Math.round(performance.now() - started);
  await writeFile(`${output}/manifest.json`, JSON.stringify(report, null, 2) + "\n");
  const entries = await Promise.all((await readdir(output)).map(async name => ({
    name, bytes: (await stat(`${output}/${name}`)).size,
  })));
  artifactBudget(entries);
}
if (report.status !== "candidate-review-required") process.exitCode = 1;
console.log(`Actions visual review: ${report.status}; ${report.captures.length} images; ${report.failures.length} failures`);
