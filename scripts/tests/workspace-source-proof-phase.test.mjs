import assert from "node:assert/strict";
import test from "node:test";
import { join } from "node:path";
import { writeFile, unlink, utimes } from "node:fs/promises";
import { leased, UncertainProcessTreeError } from "../workspace/lease.mjs";
import { beginSourceProof, finishSourceProof, failSourceProof } from "../workspace/source-proof/guard.mjs";
import { sourceFixture } from "./source-proof-fixture.mjs";
import { completion } from "./retention-observation-fixture.mjs";

async function observedFixture(t, onMetadata = async () => {}) {
  const f = await sourceFixture(t), ctx = f.context("a"), request = f.request();
  const home = request.env.CARGO_HOME;
  // Deterministic parent-stability baseline; no user's Cargo directory is read.
  await utimes(home, 1, 1);
  let calls = 0;
  const dependencies = { ...f, metadata: async () => {
    const value = await f.metadata(ctx);
    await onMetadata(++calls, { home, value });
    return value;
  } };
  return { f, ctx, request, dependencies, home, calls: () => calls };
}
async function command(value, body = async () => {}, receipt = "owned") {
  const { f, ctx, request, dependencies } = value;
  return leased(join(ctx.cache, ".lease"), async lease => {
    const guard = await beginSourceProof(ctx, request, lease, dependencies);
    try {
      const output = await f.unit();
      guard.observe(await f.message(ctx, output));
      await body();
      const proof = receipt === "none" ? undefined : await completion(ctx, lease,
        receipt === "foreign" ? {} : { invocation: guard.invocation, ...(receipt === "native-failure" ? { code: 101 } : {}) });
      return await finishSourceProof(guard.handle, proof, lease);
    } catch (error) { await failSourceProof(guard.handle, lease); throw error; }
  });
}
async function journal(home) {
  const path = join(home, ".global-cache-journal");
  await writeFile(path, "synthetic metadata cache maintenance");
  await unlink(path);
}

test("post-native metadata journal churn cannot invalidate the earlier completed-command policy interval", async t => {
  const value = await observedFixture(t, async (call, { home }) => { if (call === 2) await journal(home); });
  const result = await command(value);
  assert.equal(result.trusted, true);
  assert.equal(result.approved, 1);
  assert.equal(value.calls(), 2);
});

test("admission captures its own metadata effects before starting the native command", async t => {
  const value = await observedFixture(t, async (call, { home }) => { if (call === 1) await journal(home); });
  const result = await command(value);
  assert.equal(result.trusted, true);
  assert.equal(value.calls(), 2);
});

for (const name of ["config", "config.toml"]) {
  test(`native-command ${name} create/delete ABA still refuses admission`, async t => {
    const value = await observedFixture(t);
    await assert.rejects(command(value, async () => {
      await writeFile(join(value.home, name), "[build]\njobs=1\n");
      await unlink(join(value.home, name));
    }), /SOURCE_PROOF_INPUT_CHANGED_DURING_COMMAND/);
  });
}

test("native-command sibling directory churn remains conservatively untrusted", async t => {
  const value = await observedFixture(t);
  await assert.rejects(command(value, () => journal(value.home)), /SOURCE_PROOF_INPUT_CHANGED_DURING_COMMAND/);
});

for (const changed of ["policy", "manifest", "source"]) {
  test(`changed ${changed} cannot sign a completed compiler unit`, async t => {
    const value = await observedFixture(t);
    await assert.rejects(command(value, async () => {
      if (changed === "policy") await writeFile(join(value.home, "config.toml"), "[build]\njobs=1\n");
      else if (changed === "manifest") await writeFile(join(value.ctx.checkout, "tools/model/Cargo.toml"), '[package]\nname="model"\nversion="0.2.0"\n');
      else await value.f.change(value.ctx, "B");
    }), /SOURCE_PROOF_INPUT_CHANGED_DURING_COMMAND/);
  });
}

test("completion rederives the dependency closure rather than reusing admission metadata", async t => {
  const value = await observedFixture(t, async (call, { value: metadata }) => {
    if (call !== 2) return;
    const id = "registry+https://example.invalid/index#fixture-dependency@1.0.0";
    metadata.packages.push({ id, name: "fixture-dependency", source: "registry+https://example.invalid/index" });
    metadata.resolve.nodes[0].dependencies.push(id);
    metadata.resolve.nodes.push({ id, dependencies: [] });
  });
  await assert.rejects(command(value), /SOURCE_PROOF_INPUT_CHANGED_DURING_COMMAND/);
});

test("failed completion metadata never approves emitted compiler artifacts", async t => {
  const value = await observedFixture(t, async call => { if (call === 2) throw new Error("SYNTHETIC_METADATA_FAILURE"); });
  await assert.rejects(command(value), /SYNTHETIC_METADATA_FAILURE/);
});

test("unowned, foreign or failed native completion cannot enter completion metadata phase", async t => {
  for (const receipt of ["none", "foreign", "native-failure"]) {
    const value = await observedFixture(t, async call => assert.equal(call, 1, "metadata after unproved completion"));
    if (receipt === "foreign") await assert.rejects(command(value, undefined, receipt), UncertainProcessTreeError);
    else if (receipt === "native-failure") await assert.rejects(command(value, undefined, receipt), /Cargo exited 101/);
    else {
      value.request.env.RBIT_WORKSPACE_CARGO_TREE = "off";
      const result = await command(value, undefined, receipt);
      assert.equal(result.trusted, false);
      assert.equal(result.approved, 0);
    }
    assert.equal(value.calls(), 1);
  }
});
