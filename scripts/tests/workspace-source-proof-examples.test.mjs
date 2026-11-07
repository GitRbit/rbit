import assert from "node:assert/strict";
import test from "node:test";
import { dirname, join } from "node:path";
import { mkdir, readFile, unlink, utimes, writeFile, stat } from "node:fs/promises";
import { leased } from "../workspace/lease.mjs";
import { beginSourceProof, finishSourceProof, failSourceProof } from "../workspace/source-proof/guard.mjs";
import { completion } from "./retention-observation-fixture.mjs";
import { sourceFixture } from "./source-proof-fixture.mjs";

async function fixture(t) {
  const f = await sourceFixture(t);
  const original = f.metadata;
  f.metadata = async ctx => {
    const data = await original(ctx);
    data.packages[0].targets = [{ name: "measure_listings", kind: ["example"],
      src_path: join(ctx.checkout, "tools/model/examples/measure_listings.rs") }];
    return data;
  };
  for (const name of ["a", "b", "c"]) {
    const root = join(f.context(name).checkout, "tools/model/examples");
    await mkdir(root);
    await writeFile(join(root, "measure_listings.rs"), "fn main() {}\n");
  }
  async function emit(profile = "debug", features = [], hash = "b".repeat(16)) {
    const unit = await f.unit(hash, { profile, features, kind: "example", targetName: "measure_listings" });
    await unlink(unit.artifact);
    const suffix = process.platform === "win32" ? ".exe" : "";
    const outputs = [];
    for (const root of ["intermediates", "target"]) {
      const directory = join(f.lane, root, profile, "examples");
      await mkdir(directory, { recursive: true });
      for (const extension of [suffix, ".pdb", ".d"]) {
        const path = join(directory, `measure_listings${extension}`);
        await writeFile(path, `example ${profile} ${features.join()} ${extension}`);
        if (root === "target" && extension !== ".d") outputs.push(path);
      }
    }
    return { ...unit, artifact: outputs[0], outputs };
  }
  const command = body => leased(join(f.context("a").cache, ".lease"), async lease => {
    const proof = await beginSourceProof(f.context("a"), f.request(), lease, f);
    try {
      await body(proof);
      return await finishSourceProof(proof.handle, await completion(f.context("a"), lease, { invocation: proof.invocation }), lease);
    } catch (error) { await failSourceProof(proof.handle, lease); throw error; }
  });
  const observe = async (proof, unit, fresh = false) => {
    const message = await f.message(f.context("a"), unit, fresh);
    message.target = (await f.metadata(f.context("a"))).packages[0].targets[0];
    message.filenames = unit.outputs;
    proof.observe(message);
  };
  return { f, emit, command, observe };
}

test("unhashed example executable and PDB are approved only after reconstruction and reused warm", async t => {
  const { f, emit, command, observe } = await fixture(t);
  let unit;
  const first = await command(async proof => { unit = await emit(); await observe(proof, unit); });
  assert.equal(first.approved, 1);
  const warm = await command(async proof => {
    assert.equal(proof.report.reusable, 1);
    // Cargo may rewrite the derived primary export on a fresh invocation.
    await writeFile(join(dirname(unit.outputs[0]), "measure_listings.d"), "example debug  .d");
    await observe(proof, unit, true);
  });
  assert.equal(warm.fresh, 1);
  assert.equal(warm.rebuilt, 0);
  assert.equal(warm.approved, 1);
  assert.equal((await readFile(unit.artifact, "utf8")).startsWith("example debug"), true);
  assert.equal(f.lane.includes(".build-cache"), true);
});

test("example codegen overwrite with restored mtime invalidates its stamp, preserving outputs", async t => {
  const { emit, command, observe } = await fixture(t);
  let unit;
  await command(async proof => { unit = await emit(); await observe(proof, unit); });
  const previous = await stat(unit.outputs[1]);
  const foreign = "X".repeat(previous.size);
  await writeFile(unit.outputs[1], foreign);
  await utimes(unit.outputs[1], previous.atime, previous.mtime);
  await command(async proof => {
    assert.equal(proof.report.invalidated, 1);
    await assert.rejects(readFile(unit.stamp), /ENOENT/);
    assert.equal(await readFile(unit.outputs[1], "utf8"), foreign);
  });
});

test("unselected example feature variants stay unapproved; profile exports cannot authorize another profile", async t => {
  const { emit, command, observe } = await fixture(t);
  let debug;
  await command(async proof => {
    debug = await emit();
    await emit("release", [], "c".repeat(16));
    await emit("debug", ["extra"], "d".repeat(16));
    await observe(proof, debug);
  });
  await command(async proof => {
    assert.equal(proof.report.invalidated, 2);
    assert.equal(proof.report.reusable, 1);
    const message = await emit("release", [], "c".repeat(16));
    message.outputs = debug.outputs;
    await observe(proof, message);
  }).then(() => assert.fail("cross-profile artifact must not be selected"), error => {
    assert.match(error.message, /SOURCE_PROOF_SELECTED_UNIT_UNOBSERVED/);
  });
});

test("an unknown example output cannot be authorized by a known executable companion", async t => {
  const { emit, command, observe } = await fixture(t);
  await assert.rejects(command(async proof => {
    const unit = await emit();
    const unknown = join(dirname(unit.outputs[0]), "measure_listings.unrecognized");
    await writeFile(unknown, "not an inventoried compiler output");
    unit.outputs.push(unknown);
    await observe(proof, unit);
  }), /SOURCE_PROOF_SELECTED_UNIT_UNOBSERVED/);
});
