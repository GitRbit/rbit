import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { authored, checkSizes, checkEdges, checkMigrations, sourceFiles } from "../rewrite/check.mjs";

test("legacy ceilings cannot grow and new authored files have no blanket exemptions", () => {
  const baseline = { authoredSizeBaseline: [{ path: "legacy.rs", lines: 500 }] };
  const sources = new Map([["legacy.rs", "x\n".repeat(500)], ["new.rs", "x\n".repeat(401)]]);
  assert.equal(checkSizes(sources, baseline).length, 1);
  sources.set("legacy.rs", "x\n".repeat(501));
  assert.equal(checkSizes(sources, baseline).length, 2);
  assert.equal(authored("crates/rbit-web/assets/vendor/htmx.js"), false);
  assert.equal(authored("crates/rbit-web/src/routes/new.rs"), true);
});

test("Cargo resolved names catch renamed or dev dependency boundary leaks", () => {
  const packages = [
    { id: "kernel", name: "rbit-kernel", dependencies: [{ name: "rbit-storage", rename: "innocent", kind: "dev" }] },
    { id: "storage", name: "rbit-storage", dependencies: [] },
  ];
  const baseline = { crates: [{ name: "rbit-storage", internalDependencies: [] }] };
  assert.match(checkEdges({ packages, workspace_members: ["kernel", "storage"] }, baseline)[0], /forbidden dependency \(dev\)/);
  packages[0].dependencies = [];
  assert.deepEqual(checkEdges({ packages, workspace_members: ["kernel", "storage"] }, baseline), []);
  packages[0].name = "rbit-unknown";
  assert.match(checkEdges({ packages, workspace_members: ["kernel", "storage"] }, baseline)[0], /explicit architecture policy/);
});

test("migration fingerprints permit checkout line endings but reject mutation and removal", () => {
  const root = mkdtempSync(join(tmpdir(), "rbit-migrations-"));
  try {
    const baseline = { migrations: [{ path: "initial.sql", sha256: createHash("sha256").update("SELECT 1;\n").digest("hex") }] };
    writeFileSync(join(root, "initial.sql"), "SELECT 1;\r\n");
    assert.deepEqual(checkMigrations(root, baseline), []);
    writeFileSync(join(root, "initial.sql"), "SELECT 2;\n");
    assert.match(checkMigrations(root, baseline)[0], /changed/);
    rmSync(join(root, "initial.sql"));
    assert.match(checkMigrations(root, baseline)[0], /missing/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test("source scan ignores cache roots and refuses directory links", () => {
  const root = mkdtempSync(join(tmpdir(), "rbit-sources-"));
  try {
    mkdirSync(join(root, "crates")); mkdirSync(join(root, ".build-cache"));
    writeFileSync(join(root, "crates", "source.rs"), "// valid");
    writeFileSync(join(root, ".build-cache", "cached.rs"), "// ignored");
    assert.deepEqual(sourceFiles(root), ["crates/source.rs"]);
    symlinkSync(join(root, ".build-cache"), join(root, "crates", "escape"), "junction");
    assert.throws(() => sourceFiles(root), /contains a link/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
