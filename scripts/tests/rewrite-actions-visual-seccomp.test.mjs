import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { browserSeccompPolicy, seccompPolicy, assertBrowserSeccompDelta } from "../../devtools/actions-visual/seccomp.mjs";
import { namespaceStartupReady } from "../../devtools/actions-visual/environment.mjs";
import { initialNamespaceChroot, childNamespaceChroot } from "../../devtools/actions-visual/namespace-probe.mjs";
import { digest } from "../../devtools/actions-visual/provenance.mjs";

test("effective policy differs from verified upstream by exactly one chroot allow and nothing else", async () => {
  const upstream = await seccompPolicy(), effective = await browserSeccompPolicy();
  assert.equal(digest(JSON.stringify(effective)), "106b9b2b44d775c324f57bd196bf6b6a46add952e8955804cfc6e2e97b75813c");
  assert.deepEqual(effective.syscalls.at(-1), {
    names: ["chroot"], action: "SCMP_ACT_ALLOW", args: [],
    comment: "Allow namespace-local Chromium filesystem isolation; kernel capability checks remain enforced.",
    includes: {}, excludes: {},
  });
  const stripped = structuredClone(effective); stripped.syscalls.pop();
  assert.deepEqual(stripped, upstream);
  assert.equal(effective.defaultAction, "SCMP_ACT_ERRNO");
  assert.equal(effective.syscalls.some(rule => rule.names.includes("clone3")), false);
  assertBrowserSeccompDelta(effective, upstream);
  for (const mutate of [
    policy => policy.syscalls.at(-1).names.push("mount"),
    policy => policy.syscalls.push({ names: ["clone3"], action: "SCMP_ACT_ALLOW" }),
    policy => { policy.syscalls[0].includes.caps = ["CAP_SYS_ADMIN"]; },
    policy => { policy.defaultAction = "SCMP_ACT_ALLOW"; },
  ]) {
    const changed = structuredClone(effective); mutate(changed);
    assert.throws(() => assertBrowserSeccompDelta(changed, upstream), /Unexpected browser seccomp delta/);
  }
});

test("nonroot capability bounding/permitted/effective/ambient sets, NNP and seccomp are prerequisites", () => {
  const ready = { uid: 1001, status: { noNewPrivileges: 1, seccomp: 2,
    effectiveCapabilitiesZero: true, boundingCapabilitiesZero: true,
    permittedCapabilitiesZero: true, ambientCapabilitiesZero: true } };
  assert.equal(namespaceStartupReady(ready), true);
  for (const key of ["effectiveCapabilitiesZero", "boundingCapabilitiesZero", "permittedCapabilitiesZero", "ambientCapabilitiesZero"]) {
    for (const value of [false, null, undefined]) {
      assert.equal(namespaceStartupReady({ ...ready, status: { ...ready.status, [key]: value } }), false);
    }
  }
  assert.equal(namespaceStartupReady({ ...ready, uid: 0 }), false);
  assert.equal(namespaceStartupReady({ ...ready, status: { ...ready.status, noNewPrivileges: 0 } }), false);
  assert.equal(namespaceStartupReady({ ...ready, status: { ...ready.status, seccomp: 0 } }), false);
});

function scratch() {
  const calls = [];
  return { calls, storage: { create: async () => { calls.push("create"); return "/tmp/private-probe"; },
    remove: async path => { assert.equal(path, "/tmp/private-probe"); calls.push("remove"); } } };
}

test("initial-namespace EPERM must be observed exactly and scratch cleanup follows closed execution", async () => {
  const owned = scratch();
  const result = await initialNamespaceChroot(async (command, args, options) => {
    owned.calls.push("closed-execution");
    assert.equal(command, "/usr/sbin/chroot");
    assert.deepEqual(args, ["/tmp/private-probe", "/__rbit_probe_no_command__"]);
    assert.equal(options.windowsHide, true); assert.equal(options.killSignal, "SIGKILL");
    assert.deepEqual(Object.keys(options.env).sort(), ["HOME", "LANG", "LC_ALL", "PATH"]);
    throw { code: 125, signal: null, stderr: "chroot: cannot change root directory: Operation not permitted PRIVATE" };
  }, owned.storage);
  assert.deepEqual(result, { denied: true, confirmed: false, code: 125, reason: "INITIAL_NAMESPACE_EPERM" });
  assert.deepEqual(owned.calls, ["create", "closed-execution", "remove"]);
  assert.equal(JSON.stringify(result).includes("PRIVATE"), false);
  for (const failure of [{ code: 125, stderr: "No such file" }, { code: 127, stderr: "failed command" },
    { code: 125, signal: "SIGKILL", stderr: "cannot change root directory: Operation not permitted" }]) {
    assert.equal((await initialNamespaceChroot(async () => { throw failure; }, scratch().storage)).denied, false);
  }
});

test("child namespace must prove chroot before expected command ENOENT, not merely exit127", async () => {
  const result = await childNamespaceChroot(async (command, args) => {
    assert.equal(command, "/usr/bin/unshare");
    assert.deepEqual(args, ["--user", "--map-root-user",
      "--root", "/tmp/private-probe", "--", "/__rbit_probe_no_command__"]);
    throw { code: 127, signal: null, stderr: "unshare: failed to execute /__rbit_probe_no_command__: No such file or directory" };
  }, scratch().storage);
  assert.equal(result.confirmed, true); assert.equal(result.denied, false);
  for (const failure of [{ code: 127, stderr: "unshare: command missing" }, { code: "ENOENT" },
    { code: 1, stderr: "unshare: unshare failed: Operation not permitted" },
    { code: 127, signal: "SIGKILL", stderr: "unshare: failed to execute /__rbit_probe_no_command__: No such file or directory" },
    { code: 127, stderr: "unshare: failed to execute /different-command: No such file or directory" }]) {
    assert.equal((await childNamespaceChroot(async () => { throw failure; }, scratch().storage)).confirmed, false);
  }
});

test("container isolation and real Chromium proof remain mandatory after the narrow delta", async () => {
  const container = await readFile(new URL("../../devtools/actions-visual/container.mjs", import.meta.url), "utf8");
  for (const pair of ['"--cap-drop", "ALL"', '"--network", "none"', '"--security-opt", "no-new-privileges"',
    '"--read-only"', '"--user",']) assert.ok(container.includes(pair), pair);
  assert.doesNotMatch(container, /--cap-add|seccomp=unconfined|--privileged|--ipc.*host/);
  const runner = await readFile(new URL("../../devtools/actions-visual/run.mjs", import.meta.url), "utf8");
  assert.ok(runner.indexOf("namespaceStartupReady(report.environment)") < runner.indexOf("await initialNamespaceChroot()"));
  assert.ok(runner.indexOf("await childNamespaceChroot()") < runner.indexOf("await chromium.launch"));
  assert.ok(runner.indexOf("await verifySandbox(browser)") < runner.indexOf("for (const model of cases())"));
  assert.match(runner, /chromiumSandbox: true/);
});
