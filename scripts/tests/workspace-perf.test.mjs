import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { executableFromRecord, serveOptions } from "../perf/serve.mjs";

test("benchmark launcher selects explicit helper copies without changing fixture data defaults", () => {
  const root = resolve("fixture-root");
  const defaults = serveOptions([], root);
  assert.equal(defaults.executable, null);
  assert.equal(defaults.profile, "release");
  assert.equal(defaults.dataDir, join(root, "target", "perf", "data"));
  const exact = serveOptions(["--executable", "private-copy/rbit", "--source-revision", "main-sha"], root);
  assert.equal(exact.executable, join(root, "private-copy", "rbit"));
  assert.equal(exact.declaredSourceRevision, "main-sha");
  assert.throws(() => serveOptions(["--executable", "a", "--target-dir", "b"], root), /exactly one/);
});

test("benchmark records use exactly one named server binary and accept PowerShell UTF-8 BOM", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "rbit-perf-record-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const record = join(root, "build.json");
  await writeFile(record, "\uFEFF" + JSON.stringify({ executables: [{ name: "rbit-runner", path: "runner" }, { name: "rbit", path: "exact/rbit" }] }));
  assert.equal(executableFromRecord(record, root), join(root, "exact", "rbit"));
  await writeFile(record, JSON.stringify({ executables: [{ name: "rbit", path: "one" }, { name: "rbit", path: "two" }] }));
  assert.throws(() => executableFromRecord(record, root), /exactly one/);
});
