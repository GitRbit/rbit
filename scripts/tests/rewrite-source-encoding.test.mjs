import assert from "node:assert/strict";
import test from "node:test";
import { mkdtemp, mkdir, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { authoredSources, check } from "../rewrite/check.mjs";

async function fixture(t) {
  const root = await realpath(await mkdtemp(join(tmpdir(), "rbit-source-encoding-")));
  t.after(() => rm(root, { recursive: true, force: true }));
  return { root, async write(path, bytes) {
    const target = join(root, path);
    await mkdir(dirname(target), { recursive: true });
    await writeFile(target, bytes);
  } };
}

test("invalid authored bytes fail before baseline files or Cargo metadata are required", async t => {
  const { root, write } = await fixture(t);
  await write("crates/demo/templates/job/header.html", Buffer.from([0x3c, 0xb7, 0x3e]));
  await write("scripts/bad.mjs", Buffer.from([0xff]));
  const result = check(root); // No Cargo.toml or baseline exists: decoding must fail first.
  assert.equal(result.files, 2);
  assert.deepEqual(result.violations, [
    "crates/demo/templates/job/header.html: invalid UTF-8; save authored source as UTF-8",
    "scripts/bad.mjs: invalid UTF-8; save authored source as UTF-8",
  ]);
});

test("valid Unicode, BOM and CRLF stay exact while binary/generated inputs remain excluded", async t => {
  const { root, write } = await fixture(t);
  const source = "\uFEFF<h1>雪 café ·</h1>\r\n";
  await write("crates/demo/template.html", source);
  await write("crates/demo/logo.png", Buffer.from([0xff]));
  await write("crates/rbit-web/assets/app.bundle.js", Buffer.from([0xff]));
  await write("devtools/demo/node_modules/ignored.mjs", Buffer.from([0xff]));
  assert.deepEqual(authoredSources(root), { count: 1,
    files: [["crates/demo/template.html", source]], violations: [] });
});
