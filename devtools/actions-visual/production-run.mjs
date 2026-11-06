import { themeChecks } from "./production-theme.mjs";
// Contained debug-functional production page checks. Never a release benchmark.
import { chromium } from "@playwright/test";
import { readFile, lstat, writeFile, readdir, stat } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import { artifactBudget, image } from "./policy.mjs";
import { revision, digest } from "./provenance.mjs";
import { environment, namespaceStartupReady } from "./environment.mjs";
import { initialNamespaceChroot, childNamespaceChroot } from "./namespace-probe.mjs";
import { browserSeccompPolicy } from "./seccomp.mjs";
import { launchSandboxedBrowser, verifySandbox, SandboxVerificationError } from "./sandbox.mjs";
import { setupFailure } from "./diagnostics.mjs";
import { validateFixtureManifest } from "./production-artifact.mjs";
import { fileEvidence, publicAssets, requireLinuxBinary } from "./production-files.mjs";
import { fixtureLimits, productionBinary, productionAssetRoot } from "./production-policy.mjs";
import { ProductionFixture } from "./production-process.mjs";
import { productionChecks } from "./production-checks.mjs";
import { overviewChecks } from "./production-overview.mjs";
import { languageChecks } from "./production-language.mjs";
import { jobChecks } from "./production-job.mjs";

const started = performance.now();
const report = { schema: 1, status: "invalid", mode: "production-run", runtime: "debug-functional",
  revision: revision(process.env.VISUAL_REVISION), image, captures: [], failures: [],
  note: "Synthetic functional browser fixture; not a release performance measurement." };
let browser, fixture, stage = "input";
const fixtureErrors = new Set(["FIXTURE_FILE_REFUSED", "FIXTURE_FILE_DRIFT", "FIXTURE_FILE_LIMIT", "FIXTURE_MANIFEST_REFUSED",
  "FIXTURE_MANIFEST_LIMIT", "FIXTURE_HASH_MISMATCH", "FIXTURE_BINARY_PLATFORM_REFUSED", "FIXTURE_ASSET_REFUSED",
  "FIXTURE_ASSET_DIRECTORY_REFUSED", "FIXTURE_ASSET_LINK_REFUSED", "FIXTURE_ASSET_LIMIT", "FIXTURE_REQUIRED_ASSET_MISSING",
  "FIXTURE_READY_REFUSED", "FIXTURE_OUTPUT_LIMIT", "FIXTURE_SPAWN_FAILED", "FIXTURE_EXITED_BEFORE_READY",
  "FIXTURE_READY_TIMEOUT", "FIXTURE_EXITED_EARLY", "FIXTURE_AUTH_NOT_READY", "FIXTURE_CONTROL_REFUSED",
  "FIXTURE_STOP_REFUSED", "FIXTURE_CLOSE_UNPROVED", "FIXTURE_FORCED_STOP", "FIXTURE_EXIT_REFUSED", "PRODUCTION_CONTEXT_CLOSE_FAILED", "PRODUCTION_LANGUAGE_CHAIN_UNPROVED"]);
function code(error) {
  if (error?.code === "ERR_ASSERTION") return "ASSERTION_FAILED";
  if (fixtureErrors.has(error?.message)) return error.message;
  return setupFailure(stage, error);
}
async function input() {
  const path = "/fixture/fixture.json", info = await lstat(path);
  if (!info.isFile() || info.isSymbolicLink() || info.size > fixtureLimits.manifestBytes) throw Error("FIXTURE_MANIFEST_REFUSED");
  let manifest;
  try { manifest = JSON.parse(await readFile(path, "utf8")); } catch { throw Error("FIXTURE_MANIFEST_REFUSED"); }
  validateFixtureManifest(manifest, report.revision);
  await requireLinuxBinary(productionBinary);
  if (JSON.stringify(await fileEvidence(productionBinary, fixtureLimits.binaryBytes)) !== JSON.stringify(manifest.binary)
    || JSON.stringify(await publicAssets(productionAssetRoot)) !== JSON.stringify(manifest.assets)) throw Error("FIXTURE_HASH_MISMATCH");
  report.input = { binarySha256: manifest.binary.sha256, publicAssetsSha256: digest(JSON.stringify(manifest.assets)),
    assetFiles: manifest.assets.length, lockSha256: manifest.lockSha256 };
}
async function capture(page, id) {
  fixture.healthy();
  const bytes = await page.screenshot({ animations: "disabled", fullPage: true });
  artifactBudget([...report.captures.map(item => ({ name: `${item.id}.png`, bytes: item.bytes })),
    { name: `${id}.png`, bytes: bytes.length }, { name: "reserved.json", bytes: 65536 }]);
  await writeFile(`/artifacts/${id}.png`, bytes, { flag: "wx" });
  report.captures.push({ id, bytes: bytes.length, sha256: digest(bytes) });
}
try {
  if (process.argv.length !== 2 || process.platform !== "linux" || process.getuid?.() === 0) throw Error("LINUX_NONROOT_REQUIRED");
  await input();
  stage = "container-preflight";
  report.environment = await environment(chromium.executablePath());
  if (!report.environment.home.owned || !report.environment.home.writable) throw Error("OWNED_HOME_NOT_READY");
  if (!report.environment.executable.exists || !report.environment.executable.executable
    || !report.environment.executable.bundled) throw Error("BUNDLED_EXECUTABLE_NOT_READY");
  if (!namespaceStartupReady(report.environment)) throw Error("CONTAINER_CONTROLS_UNPROVED");
  report.seccompPolicySha256 = digest(JSON.stringify(await browserSeccompPolicy()));
  report.seccompDelta = "namespace-chroot-only-v1";
  stage = "initial-namespace-probe";
  report.initialNamespaceChroot = await initialNamespaceChroot();
  if (!report.initialNamespaceChroot.denied) throw Error("INITIAL_NAMESPACE_CHROOT_UNPROVED");
  stage = "child-namespace-probe";
  report.childNamespaceChroot = await childNamespaceChroot();
  if (!report.childNamespaceChroot.confirmed) throw Error("CHILD_NAMESPACE_CHROOT_UNPROVED");
  stage = "browser-launch";
  browser = await launchSandboxedBrowser(chromium);
  stage = "sandbox-proof";
  report.sandbox = await verifySandbox(browser);
  report.browserVersion = browser.version();
  stage = "fixture-ready";
  fixture = new ProductionFixture();
  await fixture.ready();
  await overviewChecks(browser, fixture, capture, value => { stage = value; });
  await jobChecks(browser, fixture, capture, value => { stage = value; }, value => { report.logChecks = value; });
  await productionChecks(browser, fixture, capture, value => { stage = value; });
  await languageChecks(browser, fixture, capture, value => { stage = value; });
  await themeChecks(browser, fixture, capture, value => { stage = value; });
  report.status = "candidate-review-required";
} catch (error) {
  if (error instanceof SandboxVerificationError) report.sandboxVerification = error.evidence;
  report.failures.push({ fixture: "production-run", stage, code: code(error) });
} finally {
  try { if (browser) await browser.close(); }
  catch { report.failures.push({ fixture: "cleanup", code: "BROWSER_CLOSE_FAILED" }); }
  try { if (fixture) await fixture.stop(); }
  catch (error) { report.failures.push({ fixture: "cleanup", code: code(error) }); }
  if (fixture) report.fixtureProcess = fixture.status();
  if (report.failures.length) report.status = "invalid";
  report.elapsedMs = Math.round(performance.now() - started);
  await writeFile("/artifacts/manifest.json", JSON.stringify(report, null, 2) + "\n", { flag: "wx" });
  artifactBudget(await Promise.all((await readdir("/artifacts")).map(async name => ({ name, bytes: (await stat(`/artifacts/${name}`)).size }))));
}
if (report.status !== "candidate-review-required") process.exitCode = 1;
console.log(`Production Actions visual review: ${report.status}; ${report.captures.length} images; ${report.failures.length} failures`);
