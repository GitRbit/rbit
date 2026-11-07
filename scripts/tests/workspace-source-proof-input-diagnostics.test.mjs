import assert from "node:assert/strict";
import test from "node:test";
import { join } from "node:path";
import { readFile } from "node:fs/promises";
import { inputFailure } from "../workspace/source-proof/diagnostics.mjs";
import { beginSourceProof, finishSourceProof, failSourceProof } from "../workspace/source-proof/guard.mjs";
import { leased } from "../workspace/lease.mjs";
import { completion } from "./retention-observation-fixture.mjs";
import { sourceFixture } from "./source-proof-fixture.mjs";

test("actual changed source stays untrusted and identifies content versus stability without paths", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  await assert.rejects(leased(join(ctx.cache, ".lease"), async lease => {
    const guard = await beginSourceProof(ctx, f.request(), lease, f);
    try {
      await f.change(ctx, "B"); guard.observe(await f.message(ctx, await f.unit(undefined, { content: "B" })));
      return await finishSourceProof(guard.handle, await completion(ctx, lease, { invocation: guard.invocation }), lease);
    } catch (error) { await failSourceProof(guard.handle, lease); throw error; }
  }), error => {
    const evidence = error.sourceProofInputs;
    assert.equal(evidence.digestChanged, true); assert.equal(evidence.stabilityChanged, true);
    assert.ok(evidence.packages.some(row => row.name === "model" && row.content));
    assert.ok(evidence.observations.some(row => row.kind === "package-file" && row.fields.length));
    assert.equal(error.message.includes(f.root), false);
    assert.equal(error.message.includes("lib.rs"), false);
    return true;
  });
  const ledger = JSON.parse(await readFile(join(f.lane, ".source-proof.json"), "utf8"));
  assert.equal(ledger.data.status, "untrusted"); assert.deepEqual(ledger.data.entries, {});
});

test("directory-only interval changes report field names, not host stat values or a source-content accusation", () => {
  const id = "a".repeat(64), unchanged = new Map();
  const before = { digest: "same", stability: "old", locals: unchanged,
    observations: [{ id, kind: "policy-absent-ancestor", signature: ["device", "inode", "1", "private-before", "private-before"] }] };
  const after = { digest: "same", stability: "new", locals: unchanged,
    observations: [{ id, kind: "policy-absent-ancestor", signature: ["device", "inode", "2", "private-after", "private-after"] }] };
  const evidence = inputFailure(before, after);
  assert.equal(evidence.sourceProofInputs.digestChanged, false);
  assert.equal(evidence.sourceProofInputs.stabilityChanged, true);
  assert.deepEqual(evidence.sourceProofInputs.observations[0].fields, ["size", "mtime", "ctime"]);
  assert.equal(evidence.sourceProofInputs.observations[0].kind, "policy-absent-ancestor");
  assert.equal(evidence.sourceProofInputs.changedPackages, 0);
  assert.equal(evidence.message.includes("private-before"), false);
  assert.equal(evidence.message.includes("private-after"), false);
});

test("diagnostic rows remain bounded across additions, removals and hostile identifiers", () => {
  const secret = "C:/private/token-secret";
  const snapshot = (after = false) => ({ digest: after ? "new" : "old", stability: after ? "new" : "old",
    observations: Array.from({ length: 200 }, (_, i) => ({ id: secret + i,
      kind: secret, signature: after ? "absent" : ["device", "inode", "size", "mtime", "ctime"] })),
    locals: new Map(Array.from({ length: 100 }, (_, i) => [i,
      { name: secret, content: after ? "new" : "old", identity: "same", provenance: after ? "new" : "old" }])) });
  const error = inputFailure(snapshot(), snapshot(true));
  assert.equal(error.sourceProofInputs.changedObservations, 200);
  assert.equal(error.sourceProofInputs.observations.length, 8);
  assert.equal(error.sourceProofInputs.packages.length, 8);
  assert.ok(error.message.length < 4096);
  assert.equal(error.message.includes(secret), false);
  const largestBefore = snapshot(), largestAfter = snapshot(true);
  for (const value of largestBefore.locals.values()) value.name = "n".repeat(128);
  for (const value of largestAfter.locals.values()) value.name = "n".repeat(128);
  for (const row of largestBefore.observations) row.kind = "policy-absent-ancestor";
  for (const row of largestAfter.observations) { row.kind = "policy-absent-ancestor"; row.signature = ["a", "b", "c", "d", "e"]; }
  assert.ok(Buffer.byteLength(inputFailure(largestBefore, largestAfter).message) < 4096);
  const empty = { digest: "old", stability: "old", observations: [], locals: new Map() };
  assert.doesNotThrow(() => inputFailure(empty, snapshot()));
  assert.doesNotThrow(() => inputFailure(snapshot(), empty));
});
