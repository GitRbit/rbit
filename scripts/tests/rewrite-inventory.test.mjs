import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { readSnapshot } from "../rewrite/git-snapshot.mjs";
import { summarizeSnapshot } from "../rewrite/inventory.mjs";

test("immutable snapshot ignores dirty files, preserves bytes and handles paths with spaces", () => {
  const root = mkdtempSync(join(tmpdir(), "rbit-snapshot-"));
  const git = (...args) => execFileSync("git", ["-C", root, ...args], { windowsHide: true, encoding: "utf8" });
  try {
    git("init", "--quiet");
    writeFileSync(join(root, "source file.rs"), "// baseline\nconst X: u8 = 1;\n");
    git("-c", "core.autocrlf=false", "add", ".");
    git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "--quiet", "-m", "Fixture");
    writeFileSync(join(root, "source file.rs"), "dirty\n");
    const snapshot = readSnapshot(root, "HEAD");
    assert.match(snapshot.commit, /^[a-f0-9]{40,64}$/);
    assert.equal(snapshot.files.get("source file.rs").toString(), "// baseline\nconst X: u8 = 1;\n");
    assert.equal(readSnapshot(root, "HEAD", () => false).files.size, 0);
    assert.throws(() => readSnapshot(root, "missing-revision"));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test("inventory records immutable migrations, API verbs and legacy size ceilings", () => {
  const files = new Map(Object.entries({
    "crates/rbit-example/Cargo.toml": '[package]\nname = "rbit-example"\n[dependencies]\nrbit-core.workspace = true\n',
    "crates/rbit-example/src/lib.rs": "// source\n".repeat(401),
    "crates/rbit-storage/migrations/0001_initial.sql": "CREATE TABLE users (id INTEGER);\n",
    "crates/rbit-web/src/routes/home.rs": '.route(\n "/explore", get(home))\n',
    "crates/rbit-web/openapi/core.json": JSON.stringify({ paths: { "/user": { get: {}, post: {}, parameters: [] } } }),
  }).map(([path, text]) => [path, Buffer.from(text)]));
  const report = summarizeSnapshot({ commit: "a".repeat(40), files });
  assert.deepEqual(report.totals, { crates: 1, rustFiles: 2, rustLines: 403, rustFilesOver400: 1 });
  assert.deepEqual(report.crates[0].internalDependencies, ["rbit-core"]);
  assert.equal(report.migrations[0].sha256.length, 64);
  assert.deepEqual(report.openapiOperations.map((op) => op.method), ["GET", "POST"]);
  assert.deepEqual(report.routeDeclarations[0], { path: "/explore", source: "crates/rbit-web/src/routes/home.rs", line: 1 });
  assert.equal(report.authoredSizeBaseline[0].lines, 401);
  assert.deepEqual(summarizeSnapshot({ commit: "a".repeat(40), files }), report);
});
