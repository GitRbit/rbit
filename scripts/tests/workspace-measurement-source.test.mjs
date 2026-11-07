import assert from "node:assert/strict";
import test from "node:test";
import { mkdir, writeFile, readFile, stat, utimes, unlink } from "node:fs/promises";
import { join } from "node:path";
import { sourceFixture } from "./source-proof-fixture.mjs";
import { sourceSeal, requireSourceSeal } from "../workspace/retention/session-source.mjs";

test("seal covers ignored public CSS and retains edit/revert stability independently from content", async t => {
  const f = await sourceFixture(t), ctx = f.context("a");
  const css = join(ctx.checkout, "crates/rbit-web/assets/app.css");
  await mkdir(join(ctx.checkout, "crates/rbit-web/assets"), { recursive: true });
  await writeFile(join(ctx.checkout, ".gitignore"), (await readFile(join(ctx.checkout, ".gitignore"), "utf8")) + "crates/rbit-web/assets/app.css\n");
  await writeFile(css, "body{color:red}");
  const original = await stat(css);
  const before = await sourceSeal(ctx);
  await writeFile(css, "body{color:tan}");
  assert.notEqual((await sourceSeal(ctx)).sha256, before.sha256);
  await writeFile(css, "body{color:red}");
  await utimes(css, original.atime, original.mtime);
  const after = await sourceSeal(ctx);
  assert.equal(after.sha256, before.sha256);
  assert.notEqual(after.stabilitySha256, before.stabilitySha256);
  assert.doesNotMatch(JSON.stringify(after), /worktrees|app\.css|body|\.build-cache/);
});

test("tracked absence is explicit and creation/removal changes its stability witness", async t => {
  const f = await sourceFixture(t), ctx = f.context("a"), path = join(ctx.checkout, "deploy/example.toml");
  await unlink(path);
  const before = await sourceSeal(ctx);
  await writeFile(path, "temporary");
  await unlink(path);
  const after = await sourceSeal(ctx);
  assert.equal(after.sha256, before.sha256);
  assert.notEqual(after.stabilitySha256, before.stabilitySha256);
  assert.throws(() => requireSourceSeal({ ...after, environment: "private" }), /UNPROVED/);
});
