import { sandboxProof, browserVersion } from "./policy.mjs";

const rejected = new Set(["BROWSER_VERSION_MISMATCH", "SANDBOX_ROOT_USER",
  "SANDBOX_DISABLED_FLAGS", "SANDBOX_UNPROVED"]);
function failureCode(error) {
  const text = typeof error?.message === "string" ? error.message.slice(0, 32768) : "";
  if (rejected.has(text)) return text;
  if (/target page, context or browser has been closed|session closed|target closed/i.test(text)) return "TARGET_CLOSED";
  if (/net::ERR_ABORTED/i.test(text)) return "NAVIGATION_ABORTED";
  if (/timeout \d+ms exceeded|timed out/i.test(text)) return "TIMEOUT";
  if (/protocol error/i.test(text)) return "PROTOCOL_ERROR";
  return "UNCLASSIFIED";
}
function observedRows(rows) {
  return ["layer 1 sandbox", "pid namespaces", "network namespaces", "seccomp-bpf sandbox"].map(label => {
    const row = rows.find(item => item.label.toLowerCase() === label);
    const value = row?.value.trim().toLowerCase();
    return { label, value: ["namespace", "suid", "none", "yes", "no", "enabled"].includes(value)
      ? value : row ? "unrecognized" : "missing" };
  });
}
export class SandboxVerificationError extends Error {
  constructor(evidence) {
    super("SANDBOX_CHECK_FAILED");
    this.evidence = Object.freeze(evidence);
  }
}
// Playwright 1.65 no longer adds this flag to its default Chromium switches.
// Browser.getBrowserCommandLine requires it; it does not disable a sandbox layer.
export function launchSandboxedBrowser(chromium) {
  return chromium.launch({ channel: "chromium", chromiumSandbox: true, headless: true,
    timeout: 15000, args: ["--enable-automation"] });
}
// This internal diagnostic is not a network navigation or screenshot fixture.
export async function verifySandbox(browser, uid = process.getuid?.()) {
  const evidence = { stage: "version", code: null, cdpDetached: null, contextClosed: null };
  let session, context, failure, proof;
  try {
    if (browser.version() !== browserVersion) throw new Error("BROWSER_VERSION_MISMATCH");
    evidence.stage = "cdp-session";
    session = await browser.newBrowserCDPSession();
    evidence.stage = "cdp-command";
    const { arguments: args } = await session.send("Browser.getBrowserCommandLine");
    evidence.stage = "context-create";
    context = await browser.newContext({ serviceWorkers: "block" });
    evidence.stage = "page-create";
    const page = await context.newPage();
    evidence.stage = "internal-navigation";
    await page.goto("chrome://sandbox", { timeout: 10000 });
    evidence.stage = "table-read";
    const rows = await page.locator("tr").evaluateAll(nodes => nodes.map(node => {
      const cells = [...node.querySelectorAll("td,th")].map(cell => cell.textContent.trim());
      return { label: cells[0] ?? "", value: cells[1] ?? "" };
    }));
    evidence.rows = observedRows(rows);
    evidence.stage = "proof-validation";
    proof = sandboxProof({ uid, arguments: args, rows });
  } catch (error) { failure = failureCode(error); }
  // Cleanup must finish without replacing the original failure or retaining text.
  if (session) {
    try { await session.detach(); evidence.cdpDetached = true; }
    catch { evidence.cdpDetached = false; failure ??= "CDP_CLEANUP_FAILED"; }
  }
  if (context) {
    try { await context.close(); evidence.contextClosed = true; }
    catch { evidence.contextClosed = false; failure ??= "CONTEXT_CLEANUP_FAILED"; }
  }
  if (failure) throw new SandboxVerificationError({ ...evidence, code: failure });
  return proof;
}
