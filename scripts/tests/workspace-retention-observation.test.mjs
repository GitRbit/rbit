import assert from "node:assert/strict";
import test from "node:test";
import { copyFile, link, mkdir, readFile, readdir, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { cargo } from "../workspace/build.mjs";
import { leased } from "../workspace/lease.mjs";
import { managedRetentionMode, beginRetention, finishRetention } from "../workspace/retention/observe.mjs";
import { boundedInput, policySnapshot } from "../workspace/retention/policy.mjs";
import { namespaceUsage } from "../workspace/retention/usage.mjs";
import { fixture, completion, record, namespace } from "./retention-observation-fixture.mjs";
const budget = 16 * 1024 ** 3;

test("observation is default off and cannot silently run without owned completion mode", () => {
  assert.equal(managedRetentionMode({}), "off");
  assert.equal(managedRetentionMode({ RBIT_BUILD_MANAGED_RETENTION: "off" }), "off");
  assert.equal(managedRetentionMode({ RBIT_BUILD_MANAGED_RETENTION: "observe", RBIT_WORKSPACE_CARGO_TREE: "windows-job" }), "observe");
  for (const mode of ["1", "true", "apply", "delete", ""]) assert.throws(() => managedRetentionMode({ RBIT_BUILD_MANAGED_RETENTION: mode }), /must be/);
  assert.throws(() => managedRetentionMode({ RBIT_BUILD_MANAGED_RETENTION: "observe" }), /requires owned/);
});

test("trusted record derives graph internally and keeps legacy shared artifacts unevictable", async t => {
  const { ctx, lane, read, request } = await fixture(t);
  await writeFile(join(lane, "preserve"), "compiler artifact");
  await leased(join(ctx.cache, ".lease"), async lease => {
    const handle = await beginRetention(ctx, request, lease, { read });
    const proof = await completion(ctx, lease);
    const report = await finishRetention(handle, proof, [], budget, lease);
    assert.equal(report.graph.confidence, "known");
    assert.equal(report.decision.apply, false);
    assert.equal(report.decision.reason, "CURRENT_SUCCESS_UNPROVED");
    assert.equal(report.eligibleForEviction, false);
    const saved = await record(ctx);
    assert.equal(saved.completion.treeStopped, true);
    assert.equal(saved.lane.kind, "legacy-shared");
    assert.equal(saved.lane.eligibleForEviction, false);
    assert.equal(saved.handoff.exact, true);
    assert.equal(await readFile(join(lane, "preserve"), "utf8"), "compiler artifact");
    const serialized = JSON.stringify(saved);
    for (const privateValue of [ctx.root, request.env.CARGO_HOME, "compiler artifact", 'name="model"', lease.token]) assert.equal(serialized.includes(privateValue), false);
    await assert.rejects(finishRetention(handle, proof, [], budget, lease), /authority/);
  });
});

test("serialized observations, boolean proofs and foreign leases cannot publish records", async t => {
  const { ctx, read, request } = await fixture(t);
  let oldHandle, oldProof;
  await leased(join(ctx.cache, ".lease"), async lease => {
    oldHandle = await beginRetention(ctx, request, lease, { read });
    oldProof = await completion(ctx, lease);
    await assert.rejects(finishRetention(JSON.parse(JSON.stringify(oldHandle)), oldProof, [], budget, lease), /authority/);
    await assert.rejects(finishRetention(oldHandle, { method: "windows-job", treeStopped: true }, [], budget, lease), /authority/);
  });
  await leased(join(ctx.cache, ".lease"), async lease => {
    const handle = await beginRetention(ctx, request, lease, { read });
    const proof = await completion(ctx, lease);
    await assert.rejects(finishRetention(oldHandle, proof, [], budget, lease), /authority/);
    await assert.rejects(finishRetention(handle, oldProof, [], budget, lease), /authority/);
  });
  await assert.rejects(record(ctx), error => error.code === "ENOENT");
});

test("exact independent handoff is proved from bytes rather than a supplied boolean", async t => {
  const { ctx, lane, read, request } = await fixture(t);
  const source = join(lane, "target", "app.exe");
  const destination = join(ctx.state, "bin", "generation", "app.exe");
  await mkdir(join(lane, "target")); await mkdir(join(ctx.state, "bin", "generation"), { recursive: true });
  await writeFile(source, "exact binary"); await copyFile(source, destination);
  await leased(join(ctx.cache, ".lease"), async lease => {
    const handle = await beginRetention(ctx, request, lease, { read });
    await finishRetention(handle, await completion(ctx, lease), [{ source, destination }], budget, lease);
    assert.equal((await record(ctx)).handoff.files[0].bytes, 12);
  });
  await writeFile(destination, "wrong binary");
  await leased(join(ctx.cache, ".lease"), async lease => {
    const handle = await beginRetention(ctx, request, lease, { read });
    await assert.rejects(finishRetention(handle, await completion(ctx, lease), [{ source, destination }], budget, lease), /NOT_INDEPENDENT/);
  });
});

test("input drift and unsupported workspace layouts never become a known scheduling graph", async t => {
  const { ctx, read, request } = await fixture(t);
  await leased(join(ctx.cache, ".lease"), async lease => {
    const handle = await beginRetention(ctx, request, lease, { read });
    await writeFile(join(ctx.checkout, "Cargo.lock"), "changed lock inputs");
    const report = await finishRetention(handle, await completion(ctx, lease), [], budget, lease);
    assert.equal(report.graph.confidence, "conservative");
    assert.equal((await record(ctx)).inputId, null);
  });
  await writeFile(join(ctx.checkout, "Cargo.toml"), '[workspace]\nmembers=["crates/*"]\n');
  const snapshot = await policySnapshot(ctx, namespace, request.command, request.args, request.env, read);
  assert.equal(snapshot.graph.confidence, "conservative");
});

test("records are bounded replacement files and unknown existing files are never overwritten", async t => {
  const { ctx, read, request } = await fixture(t);
  for (let index = 0; index < 2; index++) {
    await leased(join(ctx.cache, ".lease"), async lease => {
      const handle = await beginRetention(ctx, request, lease, { read });
      await finishRetention(handle, await completion(ctx, lease), [], budget, lease);
    });
  }
  assert.deepEqual((await readdir(join(ctx.cache, namespace))).sort(), ["managed-observation.json", "shared"]);
  const path = join(ctx.cache, namespace, "managed-observation.json");
  await writeFile(path, "private unrelated sentinel");
  await leased(join(ctx.cache, ".lease"), async lease => {
    const handle = await beginRetention(ctx, request, lease, { read });
    await assert.rejects(finishRetention(handle, await completion(ctx, lease), [], budget, lease));
  });
  assert.equal(await readFile(path, "utf8"), "private unrelated sentinel");
});

test("bounded namespace walks stop at limits and never follow linked descendants", async t => {
  const { ctx, lane } = await fixture(t);
  await writeFile(join(lane, "one"), "one"); await writeFile(join(lane, "two"), "two");
  assert.equal((await namespaceUsage(lane, { maximumEntries: 1 })).complete, false);
  const outside = join(ctx.root, "outside"); await mkdir(outside); await writeFile(join(outside, "sentinel"), "private");
  await symlink(outside, join(lane, "link"), process.platform === "win32" ? "junction" : "dir");
  assert.equal((await namespaceUsage(lane)).complete, false);
  assert.equal(await readFile(join(outside, "sentinel"), "utf8"), "private");
});

test("hard-linked outputs do not establish independent runtime copies", async t => {
  const { ctx, lane, read, request } = await fixture(t);
  const source = join(lane, "app.exe");
  const destination = join(ctx.state, "bin", "generation", "app.exe");
  await mkdir(join(ctx.state, "bin", "generation"), { recursive: true });
  await writeFile(source, "binary"); await link(source, destination);
  await leased(join(ctx.cache, ".lease"), async lease => {
    const handle = await beginRetention(ctx, request, lease, { read });
    await assert.rejects(finishRetention(handle, await completion(ctx, lease), [{ source, destination }], budget, lease), /NOT_INDEPENDENT/);
  });
  await assert.rejects(record(ctx), error => error.code === "ENOENT");
});

test("actual helper defaults to no observation writes and refuses a forged native receipt", async t => {
  const { ctx, read } = await fixture(t);
  let reads = 0;
  await cargo(ctx, "check", [], { key: namespace, env: {}, execute: async () => {}, readRetentionInput: async (...args) => { reads++; return read(...args); } });
  assert.equal(reads, 0);
  await assert.rejects(record(ctx), error => error.code === "ENOENT");
  if (process.platform !== "win32") return;
  await assert.rejects(cargo(ctx, "check", [], {
    key: namespace,
    env: { RBIT_WORKSPACE_CARGO_TREE: "windows-job", RBIT_BUILD_MANAGED_RETENTION: "observe" },
    readRetentionInput: read,
    execute: async () => ({ method: "windows-job", treeStopped: true }),
  }), /verified native/);
  await assert.rejects(record(ctx), error => error.code === "ENOENT");
});

test("actual helper records only after owned completion with sanitized disk-derived input", { skip: process.platform !== "win32" }, async t => {
  const { ctx, read } = await fixture(t);
  const privateSelector = "https://private:credential@example.invalid/repository";
  const result = await cargo(ctx, "check", ["-p", privateSelector], {
    key: namespace,
    env: { RBIT_WORKSPACE_CARGO_TREE: "windows-job", RBIT_BUILD_MANAGED_RETENTION: "observe", CARGO_REGISTRIES_PRIVATE_TOKEN: "private-secret" },
    readRetentionInput: read,
    execute: async (context, _args, _env, _artifact, { lease }) => completion(context, lease),
  });
  assert.equal(result.retention.recorded, true);
  const serialized = JSON.stringify(await record(ctx));
  for (const secret of [ctx.root, privateSelector, "private-secret", "credential@example", "CARGO_REGISTRIES_PRIVATE_TOKEN"]) {
    assert.equal(serialized.includes(secret), false);
  }
});

test("policy digests change with actual member/config inputs and never return raw environment or paths", async t => {
  const { ctx, read, request } = await fixture(t);
  const capture = () => policySnapshot(ctx, namespace, request.command, request.args, { ...request.env, RUSTFLAGS: "private-flag" }, read);
  const original = await capture();
  await writeFile(join(ctx.root, "crates", "model", "Cargo.toml"), '[package]\nname="model"\nversion="2.0.0"\n');
  const memberChanged = await capture();
  assert.notEqual(memberChanged.inputId, original.inputId);
  await mkdir(join(ctx.root, ".cargo"));
  await writeFile(join(ctx.root, ".cargo", "config.toml"), '[build]\njobs=1\n');
  const configurationChanged = await capture();
  assert.notEqual(configurationChanged.inputId, memberChanged.inputId);
  const serialized = JSON.stringify(configurationChanged);
  assert.equal(serialized.includes("private-flag"), false);
  assert.equal(serialized.includes(ctx.root), false);
});

test("oversized policy inputs and malformed walk bounds fail closed without reading arbitrary data", async t => {
  const { ctx, lane, request, read } = await fixture(t);
  const path = join(ctx.root, "bounded"); await writeFile(path, "five!");
  await assert.rejects(boundedInput(path, false, 4), /REFUSED/);
  assert.equal((await boundedInput(path, false, 5)).toString(), "five!");
  for (const maximumEntries of [0, -1, Infinity, 200_001]) {
    assert.deepEqual(await namespaceUsage(lane, { maximumEntries }), { bytes: 0, entries: 0, complete: false });
  }
  const unexpected = await policySnapshot(ctx, namespace, "https://secret@example.invalid", [], request.env, read);
  assert.equal(unexpected.graph.confidence, "conservative");
  assert.equal(JSON.stringify(unexpected).includes("secret@example"), false);
});

test("configuration includes and quoted keys are conservative rather than pretending all inputs were hashed", async t => {
  const { ctx, read, request } = await fixture(t);
  await mkdir(join(ctx.root, ".cargo"));
  const path = join(ctx.root, ".cargo", "config.toml");
  for (const configuration of ['include = ["private.toml"]', '"include" = ["private.toml"]', "'include' = ['private.toml']"]) {
    await writeFile(path, configuration);
    const snapshot = await policySnapshot(ctx, namespace, request.command, request.args, request.env, read);
    assert.equal(snapshot.graph.confidence, "conservative");
    assert.equal(JSON.stringify(snapshot).includes("private.toml"), false);
  }
});
