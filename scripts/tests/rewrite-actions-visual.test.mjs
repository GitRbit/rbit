import assert from "node:assert/strict";
import { setupFailure } from "../../devtools/actions-visual/diagnostics.mjs";
import test from "node:test";
import { mkdtemp, writeFile, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { allowedRequest, sandboxProof, cases, artifactBudget } from "../../devtools/actions-visual/policy.mjs";
import { seccompPolicy } from "../../devtools/actions-visual/seccomp.mjs";
import { verifyArtifacts } from "../../devtools/actions-visual/artifacts.mjs";
import { verifySandbox } from "../../devtools/actions-visual/sandbox.mjs";
import { EventEmitter } from "node:events";
import { dockerClient } from "../../devtools/actions-visual/docker.mjs";
import { createOwnership, cleanupOwned } from "../../devtools/actions-visual/ownership.mjs";
import { checkedMount } from "../../devtools/actions-visual/mounts.mjs";
import { revision, sourceDigest } from "../../devtools/actions-visual/provenance.mjs";
const good = () => ({ uid: 1001, arguments: ["chromium", "--enable-automation"], rows: [
  { label: "Layer 1 Sandbox", value: "Namespace" },
  { label: "PID namespaces", value: "Yes" }, { label: "Network namespaces", value: "Yes" },
  { label: "Seccomp-BPF sandbox", value: "Yes" },
] });

test("browser egress accepts exact loopback routes and read methods only", () => {
  for (const path of ["/job?q=error", "/assets/app.css", "/styles/responsive.css", "/raw-log.txt"]) {
    assert.equal(allowedRequest(`http://127.0.0.1:4187${path}`, "GET"), true);
  }
  for (const url of ["http://localhost:4187/job", "https://127.0.0.1:4187/job",
    "http://127.0.0.1:4188/job", "http://127.0.0.1:4187/.git/config",
    "http://127.0.0.1:4187/assets/../../package.json", "http://user@127.0.0.1:4187/job",
    "https://example.com/", "file:///tmp/secret", "data:text/html,anything"]) {
    assert.equal(allowedRequest(url, "GET"), false, url);
  }
  for (const method of ["POST", "PUT", "DELETE", "CONNECT", "PATCH"]) {
    assert.equal(allowedRequest("http://127.0.0.1:4187/intent", method), false);
  }
});

test("sandbox proof rejects root, missing rows and every disabling form", () => {
  assert.equal(sandboxProof(good()).namespace, true);
  for (const flag of ["--no-sandbox", "--disable-setuid-sandbox", "--disable-gpu-sandbox",
    "--disable-seccomp-filter-sandbox=true", "--disable-namespace-sandbox", "--single-process"]) {
    const evidence = good(); evidence.arguments.push(flag);
    assert.throws(() => sandboxProof(evidence), /SANDBOX_DISABLED_FLAGS/);
  }
  assert.throws(() => sandboxProof({ ...good(), uid: 0 }), /ROOT/);
  assert.throws(() => sandboxProof({ ...good(), arguments: [] }), /FLAGS/);
  assert.throws(() => sandboxProof({ ...good(), rows: [] }), /UNPROVED/);
  const missing = good(); missing.rows[3].value = "No";
  assert.throws(() => sandboxProof(missing), /UNPROVED/);
});

test("sandbox diagnostic never upgrades launch or diagnostic failure to success", async () => {
  let closed = false;
  const browser = {
    version: () => "158.0.8127.18",
    newBrowserCDPSession: async () => ({ send: async () => ({ arguments: good().arguments }), detach: async () => {} }),
    newContext: async () => ({
      newPage: async () => ({ goto: async () => { throw new Error("unsupported host"); } }),
      close: async () => { closed = true; },
    }),
  };
  await assert.rejects(verifySandbox(browser), /unsupported host/);
  assert.equal(closed, true);
  await assert.rejects(verifySandbox({ version: () => "unexpected" }), /VERSION_MISMATCH/);
});

test("capture matrix has 18 base images plus explicit bounded state/permission cases", () => {
  const matrix = cases();
  assert.equal(matrix.length, 29);
  assert.equal(new Set(matrix.map(item => item.id)).size, 29);
  assert.equal(matrix.filter(item => item.id.endsWith("-rtl-long")).length, 6);
  for (const item of matrix) {
    assert.match(item.id, /^[a-z0-9-]+$/);
    assert.equal(allowedRequest("http://127.0.0.1:4187" + item.path, "GET"), true);
    assert.ok([360, 1440].includes(item.viewport.width));
  }
  assert.throws(() => artifactBudget(Array.from({ length: 41 }, (_, index) => ({ name: `${index}.png`, bytes: 1 }))), /BUDGET/);
  assert.throws(() => artifactBudget([{ name: "a.png", bytes: 64 * 1024 * 1024 + 1 }]), /BUDGET/);
});

test("official split seccomp reconstruction keeps deny default and namespace permissions", async () => {
  const policy = await seccompPolicy();
  assert.equal(policy.defaultAction, "SCMP_ACT_ERRNO");
  assert.deepEqual(policy.syscalls[0].names, ["clone", "setns", "unshare"]);
  assert.equal(policy.syscalls[0].action, "SCMP_ACT_ALLOW");
  assert.ok(policy.syscalls[1].names.includes("read"));
  assert.equal(policy.syscalls.length, 27);
});

test("artifact upload refuses unexpected data and provenance changes with source bytes", async () => {
  const directory = await mkdtemp(join(tmpdir(), "rbit-visual-contract-"));
  try {
    await writeFile(join(directory, "job-dna-day-360.png"), "synthetic");
    const first = await sourceDigest(directory);
    assert.deepEqual(await verifyArtifacts(directory), { images: 1, bytes: 9 });
    await writeFile(join(directory, "job-dna-day-360.png"), "changed");
    assert.notEqual(await sourceDigest(directory), first);
    await writeFile(join(directory, "secret.txt"), "not uploadable");
    await assert.rejects(verifyArtifacts(directory), /UNEXPECTED_ARTIFACT_NAME/);
    assert.throws(() => revision("not-a-revision"), /REVISION_REQUIRED/);
    assert.equal(revision("a".repeat(40)), "a".repeat(40));
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("source and upload audits reject linked paths", { skip: process.platform === "win32" }, async () => {
  const directory = await mkdtemp(join(tmpdir(), "rbit-visual-links-"));
  try {
    await writeFile(join(directory, "manifest.json"), "{}");
    await symlink(join(directory, "manifest.json"), join(directory, "linked.png"));
    await assert.rejects(sourceDigest(directory), /SOURCE_LINK_REFUSED/);
    await assert.rejects(verifyArtifacts(directory), /UNEXPECTED_ARTIFACT_TYPE/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("mount preparation refuses caller escape and linked ancestors below the anchor", async () => {
  const directory = await mkdtemp(join(tmpdir(), "rbit-visual-mount-"));
  try {
    await writeFile(join(directory, "app.css"), "public");
    assert.equal(await checkedMount(directory, "app.css"), join(directory, "app.css"));
    await assert.rejects(checkedMount(directory, "../outside"), /MOUNT_ESCAPE/);
    await assert.rejects(checkedMount(directory, directory), /MOUNT_ESCAPE/);
    if (process.platform !== "win32") {
      await symlink(directory, join(directory, "redirect"));
      await assert.rejects(checkedMount(directory, "redirect/app.css"), /MOUNT_LINK_REFUSED/);
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("cleanup never deletes a colliding or foreign container and is repeatable", async () => {
  const directory = await mkdtemp(join(tmpdir(), "rbit-visual-owner-"));
  try {
    const control = join(directory, "control");
    const owner = await createOwnership(control);
    const cid = "b".repeat(64);
    await writeFile(owner.cidfile, cid);
    const removed = [];
    let present = true, matching = false;
    const command = async args => {
      if (args[0] === "ps") {
        const filter = args.at(-1);
        if (filter.startsWith("label=")) {
          assert.equal(filter, `label=${owner.label}`);
          return matching && present ? cid : "";
        }
        assert.equal(filter, `id=${cid}`);
        return present ? cid : "";
      }
      if (args[0] === "inspect") return matching ? owner.nonce : "foreign-owner";
      assert.deepEqual(args, ["rm", "--force", cid]);
      removed.push(cid); present = false; return "";
    };
    await assert.rejects(cleanupOwned(control, command), /OWNER_MISMATCH/);
    assert.deepEqual(removed, []);
    matching = true;
    assert.equal(await cleanupOwned(control, command), "cleaned");
    assert.equal(await cleanupOwned(control, command), "cleaned");
    assert.deepEqual(removed, [cid]);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("interrupted create without a cidfile finds only the prepublished unique label", async () => {
  const directory = await mkdtemp(join(tmpdir(), "rbit-visual-create-"));
  try {
    const control = join(directory, "control");
    const owner = await createOwnership(control), cid = "c".repeat(64);
    const calls = [];
    await cleanupOwned(control, async args => {
      calls.push(args);
      if (args[0] === "ps") return cid;
      if (args[0] === "inspect") return owner.nonce;
      assert.deepEqual(args, ["rm", "--force", cid]); return "";
    });
    assert.equal(calls[0].at(-1), `label=${owner.label}`);
    assert.equal(calls.filter(args => args[0] === "rm").length, 1);
    assert.equal(await cleanupOwned(join(directory, "absent"), async () => {
      throw new Error("Docker must not be invoked without evidence");
    }), "not-created");
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("client errors retain ownership until close and preserve interruption drain", async () => {
  const child = new EventEmitter(); child.stdout = new EventEmitter();
  child.kill = () => true;
  const client = dockerClient(() => child);
  let settled = false;
  const operation = client.docker(["create"]);
  const rejection = assert.rejects(operation, /DOCKER_COMMAND_FAILED/).then(() => { settled = true; });
  child.emit("error", new Error("simulated client I/O error"));
  await Promise.resolve();
  assert.equal(settled, false);
  let drained = false;
  const stopping = client.interruptDocker().then(() => { drained = true; });
  await Promise.resolve();
  assert.equal(drained, false);
  child.stdout.emit("data", Buffer.from("late output"));
  child.emit("close", 1, null);
  await Promise.all([rejection, stopping]);
  assert.equal(settled, true); assert.equal(drained, true);
});

test("interruption awaits Docker client closure and drains late stdout before completing", async () => {
  const child = new EventEmitter(); child.stdout = new EventEmitter();
  const kills = []; child.kill = signal => { kills.push(signal); };
  const client = dockerClient((executable, args, options) => {
    assert.equal(executable, "docker"); assert.equal(options.windowsHide, true); return child;
  });
  const command = client.docker(["start", "--attach", "d".repeat(64)]);
  const rejected = assert.rejects(command, /DOCKER_COMMAND_FAILED/);
  let closed = false;
  const stopping = client.interruptDocker().then(() => { closed = true; });
  await Promise.resolve();
  assert.equal(closed, false);
  assert.deepEqual(kills, ["SIGTERM"]);
  child.stdout.emit("data", Buffer.from("late output"));
  child.emit("close", null, "SIGTERM");
  await stopping; await rejected;
  assert.equal(closed, true);
});

test("launch evidence is classified into fixed codes without retaining private diagnostics", () => {
  const sentinel = "private-profile-path credential=example-secret";
  for (const [message, expected] of [
    ["Failed to move to new namespace: PID namespaces supported, Network namespace supported, but failed: errno = Operation not permitted", "BROWSER_NAMESPACE_DENIED"],
    ["No usable sandbox!", "BROWSER_NO_USABLE_SANDBOX"],
    ["Executable doesn't exist at /private/profile", "BROWSER_EXECUTABLE_MISSING"],
    ["error while loading shared libraries: libunknown.so", "BROWSER_LIBRARIES_MISSING"],
    ["Host system is missing dependencies to run browsers.", "BROWSER_LIBRARIES_MISSING"],
    ["Read-only file system", "BROWSER_READONLY_PATH"],
    ["Timeout 15000ms exceeded.", "BROWSER_LAUNCH_TIMEOUT"],
    ["Target page, context or browser has been closed; signal=SIGTRAP", "BROWSER_SIGNAL_TRAP"],
  ]) {
    const actual = setupFailure("browser-launch", new Error(`${message} ${sentinel}`));
    assert.equal(actual, expected);
    assert.equal(actual.includes(sentinel), false);
  }
  assert.equal(setupFailure("sandbox-proof", new Error("SANDBOX_UNPROVED")), "SANDBOX_UNPROVED");
  assert.equal(setupFailure("browser-launch", new Error(`SANDBOX_${sentinel}`)), "SETUP_FAILED");
  assert.equal(setupFailure("provenance", new Error("No usable sandbox!")), "SETUP_FAILED");
  assert.equal(setupFailure("browser-launch", undefined), "SETUP_FAILED");
});
