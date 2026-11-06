import assert from "node:assert/strict";
import test from "node:test";
import { launchSandboxedBrowser, verifySandbox, SandboxVerificationError } from "../../devtools/actions-visual/sandbox.mjs";
import { browserVersion } from "../../devtools/actions-visual/policy.mjs";
import { setupFailure } from "../../devtools/actions-visual/diagnostics.mjs";

const rows = [
  { label: "Layer 1 Sandbox", value: "Namespace" },
  ...["PID namespaces", "Network namespaces", "Seccomp-BPF sandbox"].map(label => ({ label, value: "Yes" })),
];
function fixture({ failAt, message = "private path token=SECRET", table = rows,
  closeFails = false, detachFails = false, args = ["--enable-automation"], requireAutomation = false } = {}) {
  const events = [];
  function step(stage, value) {
    events.push(stage);
    if (failAt === stage) throw new Error(message);
    return value;
  }
  const page = {
    goto: async (url, options) => {
      assert.equal(url, "chrome://sandbox"); assert.equal(options.timeout, 10000);
      step("internal-navigation");
    },
    locator: selector => {
      assert.equal(selector, "tr");
      return { evaluateAll: async () => step("table-read", table) };
    },
  };
  const context = {
    newPage: async () => step("page-create", page),
    close: async () => { events.push("context-close"); if (closeFails) throw new Error(message); },
  };
  const session = {
    send: async command => {
      assert.equal(command, "Browser.getBrowserCommandLine");
      if (requireAutomation && !args.includes("--enable-automation")) {
        throw new Error("Protocol error: Command line not returned because --enable-automation not set.");
      }
      return step("cdp-command", { arguments: args });
    },
    detach: async () => { events.push("cdp-detach"); if (detachFails) throw new Error(message); },
  };
  const browser = {
    version: () => browserVersion,
    newBrowserCDPSession: async () => step("cdp-session", session),
    newContext: async options => {
      assert.deepEqual(options, { serviceWorkers: "block" });
      return step("context-create", context);
    },
  };
  return { browser, events };
}

test("real launch options enable command-line proof without replacing sandbox evidence", async () => {
  const chromium = { launch: async options => {
    assert.equal(options.channel, "chromium");
    assert.equal(options.chromiumSandbox, true);
    assert.equal(options.headless, true);
    assert.equal(options.timeout, 15000);
    assert.deepEqual(options.args, ["--enable-automation"]);
    return fixture({ args: options.args, requireAutomation: true }).browser;
  } };
  const browser = await launchSandboxedBrowser(chromium);
  assert.equal((await verifySandbox(browser, 1001)).forbiddenFlagsAbsent, true);
  await assert.rejects(verifySandbox(fixture({ args: [], requireAutomation: true }).browser, 1001),
    error => error.evidence.stage === "cdp-command" && error.evidence.code === "PROTOCOL_ERROR");
  const negative = { launch: async options => fixture({ args: options.args, requireAutomation: true,
    table: rows.map(row => row.label === "PID namespaces" ? { ...row, value: "No" } : row) }).browser };
  await assert.rejects(verifySandbox(await launchSandboxedBrowser(negative), 1001),
    error => error.evidence.code === "SANDBOX_UNPROVED");
});

test("proof intermediates identify failures without retaining raw diagnostics or arguments", async () => {
  for (const stage of ["cdp-session", "cdp-command", "context-create", "page-create", "internal-navigation", "table-read"]) {
    const { browser, events } = fixture({ failAt: stage, args: ["--profile=SECRET"] });
    await assert.rejects(verifySandbox(browser, 1001), error => {
      assert.ok(error instanceof SandboxVerificationError);
      assert.equal(setupFailure("sandbox-proof", error), "SANDBOX_CHECK_FAILED");
      assert.equal(error.evidence.stage, stage);
      assert.equal(error.evidence.code, "UNCLASSIFIED");
      assert.doesNotMatch(JSON.stringify(error), /SECRET|private path/);
      assert.equal(error.evidence.cdpDetached, stage === "cdp-session" ? null : true);
      assert.equal(error.evidence.contextClosed, ["page-create", "internal-navigation", "table-read"].includes(stage) ? true : null);
      return true;
    });
    if (events.includes("page-create")) assert.equal(events.at(-1), "context-close");
  }
});

test("known closed, protocol, abort and timeout cases yield fixed codes only", async () => {
  for (const [message, code] of [
    ["Target page, context or browser has been closed SECRET", "TARGET_CLOSED"],
    ["Protocol error (Browser.method): SECRET", "PROTOCOL_ERROR"],
    ["page.goto: net::ERR_ABORTED at SECRET", "NAVIGATION_ABORTED"],
    ["Timeout 10000ms exceeded SECRET", "TIMEOUT"],
  ]) {
    await assert.rejects(verifySandbox(fixture({ failAt: "internal-navigation", message }).browser, 1001), error => {
      assert.equal(error.evidence.code, code);
      assert.doesNotMatch(JSON.stringify(error.evidence), /SECRET/);
      return true;
    });
  }
});

test("row evidence is allowlisted while real negative or unknown rows still reject proof", async () => {
  for (const value of ["No", "private path token=SECRET"]) {
    const table = [...rows.filter(row => row.label !== "PID namespaces"), { label: "PID namespaces", value },
      { label: "SECRET", value: "SECRET" }];
    await assert.rejects(verifySandbox(fixture({ table }).browser, 1001), error => {
      assert.equal(error.evidence.stage, "proof-validation");
      assert.equal(error.evidence.code, "SANDBOX_UNPROVED");
      assert.equal(error.evidence.rows.length, 4);
      assert.equal(error.evidence.rows[1].value, value === "No" ? "no" : "unrecognized");
      assert.doesNotMatch(JSON.stringify(error.evidence), /SECRET|private path/);
      return true;
    });
  }
});

test("cleanup completes before returning proof and cannot mask the primary failure", async () => {
  const passing = fixture();
  assert.deepEqual(await verifySandbox(passing.browser, 1001), {
    nonroot: true, forbiddenFlagsAbsent: true, namespace: true, seccompBpf: true,
  });
  assert.deepEqual(passing.events.slice(-2), ["cdp-detach", "context-close"]);
  for (const failed of [fixture({ closeFails: true }), fixture({ detachFails: true })]) {
    await assert.rejects(verifySandbox(failed.browser, 1001), error => {
      assert.match(error.evidence.code, /CLEANUP_FAILED/); return true;
    });
  }
  await assert.rejects(verifySandbox(fixture({ failAt: "internal-navigation", message: "net::ERR_ABORTED",
    closeFails: true, detachFails: true }).browser, 1001), error => {
    assert.equal(error.evidence.code, "NAVIGATION_ABORTED");
    assert.equal(error.evidence.cdpDetached, false); assert.equal(error.evidence.contextClosed, false);
    return true;
  });
});
