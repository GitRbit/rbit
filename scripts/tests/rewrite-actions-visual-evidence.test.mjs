import assert from "node:assert/strict";
import test from "node:test";
import { chmod, mkdtemp, mkdir, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { launchEvidence } from "../../devtools/actions-visual/launch-evidence.mjs";
import { integerSetting, prepareHome, processSettings } from "../../devtools/actions-visual/environment.mjs";
import { seccompPolicy } from "../../devtools/actions-visual/seccomp.mjs";

test("bounded Playwright exit evidence and Chromium stderr retain only allowed scalars", () => {
  const secret = "private-profile credential=never-retain";
  const evidence = launchEvidence(new Error([
    `browserType.launch: ${secret}`,
    `[pid=123][err] [123:456:FATAL:base/threading/platform_thread_posix.cc:162] pthread_create: Operation not permitted (1) ${secret}`,
    `[pid=123][err] [123:456:ERROR:private_file.cc:123] ${secret}`,
    `[pid=123][err] [123:456:ERROR:dbus/bus.cc:408] Failed to connect to the bus ${secret}`,
    "[pid=123] <process did exit: exitCode=133, signal=null>",
  ].join("\n")));
  assert.deepEqual(evidence, { exit: { parsed: true, code: 133, signal: null },
    stderrSources: ["DBUS", "THREAD_PLATFORM"], stderrReasons: ["DBUS_CONNECTION_FAILED", "THREAD_CREATE_EPERM"], stderrLinesObserved: 3 });
  assert.equal(JSON.stringify(evidence).includes(secret), false);
  assert.equal(JSON.stringify(evidence).includes("private_file"), false);
  assert.deepEqual(launchEvidence(new Error("<process did exit: exitCode=null, signal=SIGSYS>")).exit,
    { parsed: true, code: null, signal: "SIGSYS" });
});

test("unknown and malformed exit data cannot become arbitrary retained fields", () => {
  for (const line of ["<process did exit: exitCode=999, signal=null>",
    "<process did exit: exitCode=1, signal=SIGPRIVATE>", "exitCode=1, signal=null",
    "<process did exit: exitCode=private, signal=null>"]) {
    assert.deepEqual(launchEvidence(new Error(line)).exit, { parsed: false });
  }
  const message = "x".repeat(200000) + "\n[pid=7][err] chrome_crashpad_handler: --database is required\n<process did exit: exitCode=1, signal=null>";
  assert.deepEqual(launchEvidence(new Error(message)).stderrReasons, ["CRASHPAD_DATABASE_REQUIRED"]);
  assert.deepEqual(launchEvidence(undefined), { exit: { parsed: false }, stderrSources: [], stderrReasons: [], stderrLinesObserved: 0 });
});

test("kernel observations parse only expected numeric fields and omit unrelated process text", () => {
  assert.deepEqual(processSettings("Name:\tprivate-process\nNoNewPrivs:\t1\nSeccomp:\t2\nSeccomp_filters:\t1\nCapEff:\t0000000000000000\n"),
    { noNewPrivileges: 1, seccomp: 2, seccompFilters: 1, effectiveCapabilitiesZero: true });
  assert.deepEqual(processSettings("NoNewPrivs:\t99\nSeccomp:\tprivate\nCapEff:\tunknown\n"),
    { noNewPrivileges: null, seccomp: null, seccompFilters: null, effectiveCapabilitiesZero: null });
  for (const value of ["-1", "999999999999999999999", "1 credential=private", "", undefined]) assert.equal(integerSetting(value), null);
  assert.equal(integerSetting("6191584\n"), 6191584);
  assert.equal(integerSetting("2", 1), null);
});

test("fresh HOME is private, owned and proven writable without leftover probe files", { skip: process.platform !== "linux" }, async () => {
  const directory = await mkdtemp(join(tmpdir(), "rbit-visual-home-"));
  try {
    const home = join(directory, "home");
    assert.deepEqual(await prepareHome(home, process.getuid()), { created: true, owned: true, writable: true });
    assert.deepEqual(await readdir(home), []);
    assert.deepEqual(await prepareHome(home, process.getuid()), { created: false, owned: true, writable: true });
    await chmod(home, 0o755);
    assert.deepEqual(await prepareHome(home, process.getuid()), { created: false, owned: false, writable: false });
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("linked HOME never writes through or changes an outside sentinel", { skip: process.platform !== "linux" }, async () => {
  const directory = await mkdtemp(join(tmpdir(), "rbit-visual-home-link-"));
  try {
    const outside = join(directory, "outside"); await mkdir(outside, { mode: 0o700 });
    await writeFile(join(outside, "sentinel"), "unchanged");
    const home = join(directory, "home"); await symlink(outside, home);
    assert.deepEqual(await prepareHome(home, process.getuid()), { created: false, owned: false, writable: false });
    assert.equal(await readFile(join(outside, "sentinel"), "utf8"), "unchanged");
    assert.deepEqual(await readdir(outside), ["sentinel"]);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("pinned upstream policy observation remains exact, with clone3 still denied by default", async () => {
  const policy = await seccompPolicy();
  assert.equal(policy.defaultAction, "SCMP_ACT_ERRNO");
  assert.equal(policy.defaultErrnoRet, undefined);
  assert.equal(policy.syscalls.some(rule => rule.names.includes("clone3")), false);
  assert.deepEqual(policy.syscalls[0].names, ["clone", "setns", "unshare"]);
});
