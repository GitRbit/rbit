import assert from "node:assert/strict";
import test from "node:test";
import { join } from "node:path";
import { readFile } from "node:fs/promises";
import { leased } from "../workspace/lease.mjs";
import { beginSourceProof, finishSourceProof, failSourceProof } from "../workspace/source-proof/guard.mjs";
import { selectionFailure } from "../workspace/source-proof/diagnostics.mjs";
import { completion } from "./retention-observation-fixture.mjs";
import { sourceFixture } from "./source-proof-fixture.mjs";

async function command(f, body) {
  const ctx = f.context("a");
  return leased(join(ctx.cache, ".lease"), async lease => {
    const guard = await beginSourceProof(ctx, f.request(), lease, f);
    try {
      await body(guard, ctx);
      return await finishSourceProof(guard.handle, await completion(ctx, lease, { invocation: guard.invocation }), lease);
    } catch (error) { await failSourceProof(guard.handle, lease); throw error; }
  });
}
test("unmatched companion diagnostics retain refusal without paths or a trusted ledger", async t => {
  const f = await sourceFixture(t);
  let output;
  await assert.rejects(command(f, async (guard, ctx) => {
    output = await f.unit();
    const message = await f.message(ctx, output);
    message.filenames.push(join(f.lane, "target", "debug", "missing.dll.lib"));
    guard.observe(message);
  }), error => {
    const evidence = error.sourceProofSelection;
    assert.equal(evidence.selected, 0); assert.equal(evidence.reconstructed, 0);
    assert.equal(evidence.packageUnits, 1); assert.equal(evidence.targetUnits, 1);
    assert.equal(evidence.featureUnits, 1); assert.equal(evidence.sourceUnits, 1);
    assert.equal(evidence.candidates[0].knownFiles, 1);
    assert.equal(error.message.includes(f.root), false);
    assert.equal(error.message.includes("missing.dll"), false);
    return true;
  });
  const ledger = JSON.parse(await readFile(join(f.lane, ".source-proof.json"), "utf8"));
  assert.equal(ledger.data.status, "untrusted"); assert.deepEqual(ledger.data.entries, {});
});
test("unchanged controls reported as rebuilt stay refused and explain reconstruction failure", async t => {
  const f = await sourceFixture(t);
  let output;
  await command(f, async (guard, ctx) => { output = await f.unit(); guard.observe(await f.message(ctx, output)); });
  await assert.rejects(command(f, async (guard, ctx) => guard.observe(await f.message(ctx, output, false))), error => {
    assert.equal(error.sourceProofSelection.selected, 1);
    assert.equal(error.sourceProofSelection.reconstructed, 0);
    assert.equal(error.sourceProofSelection.candidates[0].controlsChanged, false);
    assert.equal(error.sourceProofSelection.candidates[0].approved, true);
    return true;
  });
});
test("diagnostics bound hostile names and large candidate sets without echoing input payloads", () => {
  const privatePath = "C:/private/credential-secret";
  const units = new Map(Array.from({ length: 100 }, (_, index) => [index, {
    id: String(index), pkg: { id: "package", name: privatePath, targets: [] }, target: privatePath,
    profile: privatePath, prefix: "lib-", features: [], companions: [], controlSignature: "control",
  }]));
  const state = { units: new Map(), invalidated: new Set(), request: { lane: privatePath } };
  const error = selectionFailure({ reason: "compiler-artifact", package_id: "package",
    target: { name: privatePath, kind: [privatePath] }, features: [], filenames: [privatePath], fresh: false },
    units, [...units.values()], [...units.values()], state, {});
  assert.equal(error.sourceProofSelection.candidates.length, 8);
  assert.ok(error.message.length < 4096);
  assert.equal(error.message.includes(privatePath), false);
  assert.equal(error.message.includes("credential-secret"), false);
});
