import assert from "node:assert/strict";
import test from "node:test";
import { join, dirname } from "node:path";
import { readFile, writeFile, stat, utimes, symlink, mkdir, unlink, open } from "node:fs/promises";
import { leased, acquire, UncertainProcessTreeError } from "../workspace/lease.mjs";
import { beginSourceProof, finishSourceProof, failSourceProof } from "../workspace/source-proof/guard.mjs";
import { completion } from "./retention-observation-fixture.mjs";
import { sourceFixture } from "./source-proof-fixture.mjs";
import { cargo } from "../workspace/build.mjs";
import { inputSnapshot, absentInput } from "../workspace/source-proof/inputs.mjs";
import { scanUnits } from "../workspace/source-proof/units.mjs";

async function command(f, ctx, body, { proof = true } = {}) {
  return leased(join(ctx.cache, ".lease"), async lease => {
    const guard = await beginSourceProof(ctx, f.request(), lease, f);
    try {
      await body(guard, lease);
      return await finishSourceProof(guard.handle, proof ? await completion(ctx, lease, { invocation: guard.invocation }) : undefined, lease);
    } catch (error) { await failSourceProof(guard.handle, lease); throw error; }
  });
}

test("unowned completion never rereads source inputs or admits its output", async t => {
  const f = await sourceFixture(t), ctx = f.context("a"), metadata = f.metadata;
  let reads = 0;
  f.metadata = (...args) => { assert.equal(++reads, 1, "unowned output needs no completion snapshot"); return metadata(...args); };
  const report = await command(f, ctx, async guard => {
    await f.change(ctx, "B");
    guard.observe(await f.message(ctx, await f.unit(undefined, { content: "B" })));
  }, { proof: false });
  assert.deepEqual(report, { trusted: false, reason: "OWNED_NATIVE_COMPLETION_REQUIRED", approved: 0 });
  assert.equal(reads, 1);
  const stored = JSON.parse(await readFile(join(f.lane, ".source-proof.json"), "utf8"));
  assert.equal(stored.data.status, "untrusted");
  assert.deepEqual(stored.data.entries, {});
  f.metadata = metadata;
  const recovery = await command(f, ctx, async guard => {
    assert.equal(guard.report.reusable, 0);
    assert.equal(guard.report.invalidated, 1);
    guard.observe(await f.message(ctx, await f.unit(undefined, { content: "B" })));
  });
  assert.equal(recovery.approved, 1);
});

test("three worktrees and backdated source never reuse another root; selected and unselected variants stay distinct", async t => {
  const f = await sourceFixture(t), a = f.context("a"), b = f.context("b"), c = f.context("c");
  await f.change(b, "B", true); await f.change(c, "C", true);
  const first = await f.unit();
  const unselected = await f.unit("b".repeat(16), { features: ["special"], profile: "custom" });
  await command(f, a, async guard => {
    await assert.rejects(readFile(first.stamp), /ENOENT/);
    await assert.rejects(readFile(unselected.stamp), /ENOENT/);
    guard.observe(await f.message(a, await f.unit()));
  });
  await command(f, a, async guard => {
    assert.equal(guard.report.invalidated, 0);
    guard.observe(await f.message(a, first, true));
  });
  for (const [ctx, value] of [[b, "B"], [c, "C"], [a, "A"]]) {
    await command(f, ctx, async guard => {
      await assert.rejects(readFile(first.stamp), /ENOENT/);
      guard.observe(await f.message(ctx, await f.unit(undefined, { content: value })));
    });
    await assert.rejects(readFile(unselected.stamp), /ENOENT/, "unbuilt feature/profile never becomes approved");
  }
});

test("old-helper overwrite with unchanged small Cargo controls and preserved mtime is detected by companion identity", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  let output;
  await command(f, ctx, async guard => { output = await f.unit(); guard.observe(await f.message(ctx, output)); });
  const original = await stat(output.artifact);
  await writeFile(output.artifact, "B");
  await utimes(output.artifact, original.atime, original.mtime);
  await command(f, ctx, async guard => {
    assert.equal(guard.report.invalidated, 1);
    await assert.rejects(readFile(output.stamp), /ENOENT/);
    guard.observe(await f.message(ctx, await f.unit()));
  });
});

test("source ABA and external deploy include changes fail handoff rather than approving a wrong-source binary", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  await assert.rejects(command(f, ctx, async guard => {
    const original = await stat(join(ctx.checkout, "tools/model/src/lib.rs"));
    await f.change(ctx, "B"); const output = await f.unit(undefined, { content: "B" });
    await f.change(ctx, "A"); await utimes(join(ctx.checkout, "tools/model/src/lib.rs"), original.atime, original.mtime);
    guard.observe(await f.message(ctx, output));
  }), /INPUT_CHANGED_DURING_COMMAND/);
  await command(f, ctx, async guard => { guard.observe(await f.message(ctx, await f.unit())); });
  const file = join(ctx.checkout, "deploy/example.toml"), before = await stat(file);
  await writeFile(file, "mode = 'B'\n"); await utimes(file, before.atime, before.mtime);
  await command(f, ctx, async guard => {
    assert.equal(guard.report.invalidated, 1);
    guard.observe(await f.message(ctx, await f.unit()));
  });
});

test("missing native proof never approves outputs; forged ledger cannot choose or authorize deletion", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  const report = await command(f, ctx, async guard => { guard.observe(await f.message(ctx, await f.unit())); }, { proof: false });
  assert.equal(report.trusted, false);
  const stamp = join(f.lane, "intermediates/debug/.fingerprint/model-aaaaaaaaaaaaaaaa/lib-model");
  await writeFile(join(f.lane, ".source-proof.json"), '{"data":{"delete":"unrelated"},"mac":"forged"}');
  await assert.rejects(command(f, ctx, async () => assert.fail("must refuse before execution")), /LEDGER_REFUSED/);
  assert.equal(await readFile(stamp, "utf8"), "0123456789abcdef");
});

test("linked freshness leaf or ancestor is refused without touching its external sentinel", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  const output = await f.unit();
  const outside = join(f.root, "sentinel"); await writeFile(outside, "0123456789abcdef");
  const { unlink } = await import("node:fs/promises"); await unlink(output.stamp);
  try { await symlink(outside, output.stamp, "file"); }
  catch (error) { if (error.code === "EPERM") return t.skip("Symlink privilege unavailable"); throw error; }
  await assert.rejects(command(f, ctx, async () => assert.fail()), /REFUSED|Linked/);
  assert.equal(await readFile(outside, "utf8"), "0123456789abcdef");
});

test("large companion overwrite is detected even with the original mtime and identical small controls", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  let output;
  await command(f, ctx, async guard => { output = await f.unit(undefined, { content: Buffer.alloc(5 * 1024 ** 2) }); guard.observe(await f.message(ctx, output)); });
  const before = await stat(output.artifact);
  const file = await open(output.artifact, "r+"); await file.write(Buffer.from("B"), 0, 1, 5 * 1024 ** 2 - 1); await file.close();
  await utimes(output.artifact, before.atime, before.mtime);
  await command(f, ctx, async guard => {
    assert.equal(guard.report.invalidated, 1);
    guard.observe(await f.message(ctx, await f.unit(undefined, { content: Buffer.alloc(5 * 1024 ** 2) })));
  });
});

test("tracked deletion and sparse absence are explicit inputs rather than forced materialization", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  await unlink(join(ctx.checkout, "deploy/example.toml"));
  await command(f, ctx, async guard => { guard.observe(await f.message(ctx, await f.unit())); });
  await writeFile(join(ctx.checkout, "deploy/example.toml"), "mode = 'A'\n");
  await command(f, ctx, async guard => {
    assert.equal(guard.report.invalidated, 1);
    guard.observe(await f.message(ctx, await f.unit()));
  });
});

test("unhashed hyphenated bins select the rebuilt release unit, never debug/check/custom units", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  const original = f.metadata;
  f.metadata = async context => {
    const data = await original(context);
    data.packages[0].targets[0].name = "model-tool";
    data.packages[0].targets[0].kind = ["bin"];
    return data;
  };
  for (const [hash, profile] of [["a", "debug"], ["b", "release"], ["c", "custom"], ["d", "release"]]) {
    await f.unit(hash.repeat(16), { profile, kind: "bin", targetName: "model-tool" });
  }
  const suffix = process.platform === "win32" ? ".exe" : "";
  const executable = join(f.lane, "target/release", `model-tool${suffix}`);
  const dependencyExport = join(f.lane, "target/release/model-tool.d");
  const messageFor = async fresh => {
    const data = await f.metadata(ctx);
    return { reason: "compiler-artifact", package_id: data.packages[0].id, target: data.packages[0].targets[0],
      filenames: [executable], features: [], fresh };
  };
  await mkdir(dirname(executable), { recursive: true });
  await command(f, ctx, async guard => {
    await f.unit("b".repeat(16), { profile: "release", kind: "bin", targetName: "model-tool" });
    await writeFile(join(f.lane, "intermediates/release/deps", `model_tool${suffix}`), "release executable");
    await writeFile(executable, "release executable");
    await writeFile(dependencyExport, "model-tool: source-a.rs\n");
    guard.observe(await messageFor(false));
  });
  const report = await command(f, ctx, async guard => {
    assert.equal(guard.report.invalidated, 0);
    await writeFile(dependencyExport, "model-tool: source-a.rs\n");
    await utimes(dependencyExport, 2, 2);
    guard.observe(await messageFor(true));
  });
  assert.equal(report.approved, 1);
  assert.equal(report.fresh, 1);
  await writeFile(dependencyExport, "model-tool: source-b.rs\n");
  await command(f, ctx, async guard => {
    assert.equal(guard.report.invalidated, 1, "changed export bytes still invalidate provenance");
    await f.unit("b".repeat(16), { profile: "release", kind: "bin", targetName: "model-tool" });
    await writeFile(dependencyExport, "model-tool: source-a.rs\n");
    guard.observe(await messageFor(false));
  });
});

test("cached build-script execution inherits exact compiled-script proof and generated-output signatures", async t => {
  const f = await sourceFixture(t), ctx = f.context("a"), original = f.metadata;
  const source = join(ctx.checkout, "tools/model/build.rs"); await writeFile(source, "fn main() {}\n");
  f.metadata = async context => {
    const data = await original(context);
    data.packages[0].targets.push({ name: "build-script-build", src_path: source, kind: ["custom-build"] });
    return data;
  };
  const scriptHash = "c".repeat(16), runtimeHash = "d".repeat(16), stamp = "0123456789abcdef";
  const compiledDir = join(f.lane, "intermediates/debug/.fingerprint", `model-${scriptHash}`);
  const runtimeDir = join(f.lane, "intermediates/debug/.fingerprint", `model-${runtimeHash}`);
  const binary = join(f.lane, "intermediates/debug/build", `model-${scriptHash}`, "build-script-build.exe");
  const out = join(f.lane, "intermediates/debug/build", `model-${runtimeHash}`, "out");
  const data = await f.metadata(ctx), pkg = data.packages[0];
  const artifact = { reason: "compiler-artifact", package_id: pkg.id, target: pkg.targets[1], features: [], filenames: [binary], fresh: false };
  const execution = { reason: "build-script-executed", package_id: pkg.id, out_dir: out };
  await command(f, ctx, async guard => {
    await mkdir(compiledDir, { recursive: true }); await mkdir(runtimeDir, { recursive: true });
    await mkdir(dirname(binary), { recursive: true }); await mkdir(out, { recursive: true });
    await writeFile(binary, "compiled script"); await writeFile(join(out, "generated.rs"), "source A");
    await writeFile(join(compiledDir, "build-script-build-script-build"), stamp);
    await writeFile(join(compiledDir, "build-script-build-script-build.json"), JSON.stringify({ target: 1, profile: 1, features: "[]", deps: [], local: [], rustflags: [] }));
    await writeFile(join(runtimeDir, "run-build-script-build-script-build"), stamp);
    await writeFile(join(runtimeDir, "run-build-script-build-script-build.json"),
      `{"target":0,"profile":0,"features":"","deps":[[1,"build_script_build",false,${Buffer.from(stamp, "hex").readBigUInt64LE()}]],"local":[],"rustflags":[]}`);
    guard.observe(artifact); guard.observe(execution);
  });
  const report = await command(f, ctx, async guard => { guard.observe({ ...artifact, fresh: true }); guard.observe(execution); });
  assert.equal(report.approved, 2);
  assert.equal(report.invalidated, 0);
  await writeFile(join(out, "generated.rs"), "source B");
  await command(f, ctx, async guard => {
    assert.equal(guard.report.invalidated, 1);
    await writeFile(join(runtimeDir, "run-build-script-build-script-build"), stamp);
    await writeFile(join(out, "generated.rs"), "source A");
    guard.observe({ ...artifact, fresh: true }); guard.observe(execution);
  });
});

test("actual helper reports native admission from all artifact frames and lock writers never demand valid metadata", { skip: process.platform !== "win32" }, async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  const env = { ...f.request().env, RBIT_WORKSPACE_CARGO_TREE: "windows-job" };
  const result = await cargo(ctx, "build", ["-p", "model"], { key: f.request().namespace, env, proofInputs: f,
    execute: async (_context, _args, _env, _artifact, options) => {
      const output = await f.unit();
      options.onMessage(await f.message(ctx, output));
      return completion(ctx, options.lease, { invocation: options.invocation });
    },
  });
  assert.equal(result.sourceProof.trusted, true);
  assert.equal(result.sourceProof.approved, 1);
  assert.equal(result.sourceProof.rebuilt, 1);
  await cargo(ctx, "generate-lockfile", [], { key: f.request().namespace, env,
    proofInputs: { metadata: () => assert.fail("lock writer must not resolve existing locked metadata") },
    execute: async () => { await writeFile(join(ctx.checkout, "Cargo.lock"), "new lock fixture\n"); },
  });
});

test("foreign receipt plus failed ledger quarantine retains the actual blocked lease", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  await assert.rejects(leased(join(ctx.cache, ".lease"), async lease => {
    const guard = await beginSourceProof(ctx, f.request(), lease, f);
    const receipt = await completion(ctx, lease); // Same lease, different invocation.
    await writeFile(join(f.lane, ".source-proof.json"), "foreign control file");
    await finishSourceProof(guard.handle, receipt, lease);
  }), UncertainProcessTreeError);
  await assert.rejects(acquire(join(ctx.cache, ".lease"), { timeout: 0 }), /Blocked lease/);
});

test("failed native command and replayed guard handle cannot approve any selected units", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  await leased(join(ctx.cache, ".lease"), async lease => {
    const guard = await beginSourceProof(ctx, f.request(), lease, f);
    guard.observe(await f.message(ctx, await f.unit()));
    await assert.rejects(completion(ctx, lease, { code: 101, invocation: guard.invocation }), /101/);
    await failSourceProof(guard.handle, lease);
    await assert.rejects(finishSourceProof(guard.handle, await completion(ctx, lease, { invocation: guard.invocation }), lease), /AUTHORITY_UNPROVED/);
  });
  await command(f, ctx, async guard => {
    assert.equal(guard.report.invalidated, 1);
    guard.observe(await f.message(ctx, await f.unit()));
  });
  const record = await readFile(join(f.lane, ".source-proof.json"), "utf8");
  assert.equal(record.includes(f.root), false);
  assert.equal(record.includes("pub const"), false);
});

test("primary library frames may include top-level rlib mirrors and hashed metadata together", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  const mirror = join(f.lane, "target/debug/libmodel.rlib");
  const dependencyExport = join(f.lane, "target/debug/libmodel.d");
  await mkdir(dirname(mirror), { recursive: true });
  await command(f, ctx, async guard => {
    const output = await f.unit(); await writeFile(mirror, "independent primary library");
    await writeFile(dependencyExport, "libmodel: source-a.rs\n");
    const message = await f.message(ctx, output);
    message.filenames.push(mirror); guard.observe(message);
  });
  await command(f, ctx, async guard => {
    assert.equal(guard.report.invalidated, 0);
    await writeFile(dependencyExport, "libmodel: source-a.rs\n");
    await utimes(dependencyExport, 2, 2);
    const message = await f.message(ctx, { artifact: join(f.lane, "intermediates/debug/deps/libmodel-aaaaaaaaaaaaaaaa.rmeta"), features: [] }, true);
    message.filenames.push(mirror); guard.observe(message);
  });
  await assert.rejects(command(f, ctx, async guard => {
    const compilerDepInfo = join(f.lane, "intermediates/debug/deps/model-aaaaaaaaaaaaaaaa.d");
    await utimes(compilerDepInfo, 3, 3);
    guard.observe(await f.message(ctx, { artifact: join(f.lane, "intermediates/debug/deps/libmodel-aaaaaaaaaaaaaaaa.rmeta"), features: [] }, true));
  }), /UNEXPECTED_FRESH_UNIT/, "compiler deps .d keeps strict identity/time checks");
});

test("a recognized empty Cargo stamp stays untrusted and can rebuild normally", async t => {
  const f = await sourceFixture(t), ctx = f.context("a"), partial = await f.unit();
  await writeFile(partial.stamp, "");
  const report = await command(f, ctx, async guard => {
    assert.equal(guard.report.reusable, 0);
    assert.equal((await readFile(partial.stamp)).length, 0);
    guard.observe(await f.message(ctx, await f.unit()));
  });
  assert.equal(report.approved, 1);
});

test("codegen-heavy variant sets remain bounded and stat signatures detect exact-mtime-restored rewrites", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  const first = await f.unit("0".repeat(16));
  await utimes(first.artifact, 1, 1);
  const request = f.request();
  const inputs = await inputSnapshot(ctx, request.namespace, request.command, request.args, request.env, f);
  const initial = await scanUnits(f.lane, inputs);
  for (let index = 0; index < 33; index++) {
    const unit = index === 0 ? first : await f.unit(index.toString(16).padStart(16, "0"));
    const file = await open(unit.artifact, "r+"); await file.truncate(4 * 1024 ** 2); await file.close();
    await utimes(unit.artifact, 1, 1);
  }
  const before = await scanUnits(f.lane, inputs);
  assert.equal(before.size, 33, "more than128MiB of codegen must not consume the strict control budget");
  for (const unit of before.values()) assert.equal(unit.companions.find(([path]) => path.endsWith(".rmeta"))[2], null);
  const old = await stat(first.artifact, { bigint: true });
  const file = await open(first.artifact, "r+"); await file.write(Buffer.from("B"), 0, 1, 4 * 1024 ** 2 - 1); await file.close();
  await utimes(first.artifact, 1, 1);
  const changed = await stat(first.artifact, { bigint: true });
  assert.equal(changed.mtimeNs, old.mtimeNs, "mtime is restored exactly, including subsecond precision");
  assert.notEqual(changed.ctimeNs, old.ctimeNs);
  const after = await scanUnits(f.lane, inputs);
  const id = [...initial.keys()][0];
  assert.notEqual(after.get(id).signature, before.get(id).signature);
  const repeated = await scanUnits(f.lane, inputs);
  for (const [key, unit] of after) assert.equal(unit.signature, repeated.get(key).signature, "unchanged set has no iteration-budget-dependent signatures");
});

test("initially absent Cargo configuration cannot be created and deleted during a command unnoticed", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  const config = join(f.request().env.CARGO_HOME, "config.toml");
  await assert.rejects(command(f, ctx, async guard => {
    await writeFile(config, "[build]\njobs=2\n");
    const output = await f.unit();
    await unlink(config);
    guard.observe(await f.message(ctx, output));
  }), /INPUT_CHANGED_DURING_COMMAND/);
});

test("absence observation terminates when even the filesystem root is missing", async () => {
  let calls = 0;
  await assert.rejects(absentInput(join(process.platform === "win32" ? "Z:\\" : "/", "missing/config"), { stability: [] }, {
    stat: async () => { calls++; throw Object.assign(new Error("missing fixture root"), { code: "ENOENT" }); },
  }), /ABSENCE_UNOBSERVED/);
  assert.ok(calls < 128);
});

test("Windows default source proof refuses missing completion instead of taking the legacy off path", { skip: process.platform !== "win32" }, async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  await assert.rejects(leased(join(ctx.cache, ".lease"), async lease => {
    const request = f.request();
    delete request.env.RBIT_WORKSPACE_CARGO_TREE;
    const guard = await beginSourceProof(ctx, request, lease, f);
    await finishSourceProof(guard.handle, undefined, lease);
  }), UncertainProcessTreeError);
  await assert.rejects(acquire(join(ctx.cache, ".lease"), { timeout: 0 }), /Blocked lease/);
});
