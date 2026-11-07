import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { authored, checkSizes, checkEdges, checkMigrations, checkLedger, sourceFiles, strictCrateNames } from "../rewrite/check.mjs";

test("legacy ceilings cannot grow and new authored files have no blanket exemptions", () => {
  const baseline = { authoredSizeBaseline: [{ path: "legacy.rs", lines: 500 }] };
  const sources = new Map([["legacy.rs", "x\n".repeat(500)], ["new.rs", "x\n".repeat(401)]]);
  assert.equal(checkSizes(sources, baseline).length, 1);
  sources.set("legacy.rs", "x\n".repeat(501));
  assert.equal(checkSizes(sources, baseline).length, 2);
  assert.equal(authored("crates/rbit-web/assets/vendor/htmx.js"), false);
  assert.equal(authored("crates/rbit-web/src/routes/new.rs"), true);
  assert.equal(authored("scripts/workspace/windows-job.ps1"), true);
  assert.equal(authored("scripts/workspace/WindowsJob.cs"), true);
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

test("password protocol fixtures cannot introduce production admission dependencies", () => {
  const web = { id: "web", name: "rbit-web", dependencies: [
    { name: "rbit-identity", rename: "credentials", kind: "dev" },
    { name: "rbit-kernel", kind: "dev" },
  ] };
  const packages = [web, { id: "identity", name: "rbit-identity", dependencies: [] },
    { id: "kernel", name: "rbit-kernel", dependencies: [] }];
  const metadata = { packages, workspace_members: packages.map((pkg) => pkg.id) };
  const baseline = { crates: [{ name: "rbit-web", internalDependencies: [] }] };
  assert.deepEqual(checkEdges(metadata, baseline), []);
  web.dependencies[0].kind = null;
  assert.match(checkEdges(metadata, baseline)[0], /rbit-web -> rbit-identity: forbidden dependency \(normal\)/);
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

test("review prototypes obey source ceilings including their stylesheets", () => {
  const root = mkdtempSync(join(tmpdir(), "rbit-prototype-sources-"));
  try {
    const directory = join(root, "docs", "design", "prototypes", "actions");
    mkdirSync(directory, { recursive: true });
    writeFileSync(join(directory, "view.mjs"), "// renderer\n");
    writeFileSync(join(directory, "prototype.css"), ":root {}\n".repeat(401));
    writeFileSync(join(directory, "README.md"), "Review guidance\n");
    const files = sourceFiles(root);
    assert.deepEqual(files, ["docs/design/prototypes/actions/prototype.css", "docs/design/prototypes/actions/view.mjs"]);
    assert.equal(authored("docs/design/prototypes/actions/prototype.css"), true);
    assert.equal(authored("docs/design/prototypes/actions/README.md"), false);
    assert.equal(checkSizes(new Map([[files[0], ":root {}\n".repeat(401)]]), { authoredSizeBaseline: [] }).length, 1);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test("feature retirement requires replacement and acceptance evidence", () => {
  const root = mkdtempSync(join(tmpdir(), "rbit-ledger-"));
  try {
    writeFileSync(join(root, "baseline.rs"), "// baseline");
    const feature = { id: "example", status: "baseline-retained", source: "baseline.rs", test: "baseline.rs" };
    assert.deepEqual(checkLedger(root, { features: [feature] }), []);
    assert.match(checkLedger(root, { features: [feature, feature] })[0], /duplicate/);
    feature.status = "ported";
    assert.equal(checkLedger(root, { features: [feature] }).length, 2);
    feature.replacement = ["baseline.rs"]; feature.acceptance = "baseline.rs";
    assert.deepEqual(checkLedger(root, { features: [feature] }), []);
    feature.status = "changed";
    assert.match(checkLedger(root, { features: [feature] })[0], /decision/);
    feature.decision = "../escape.md";
    assert.match(checkLedger(root, { features: [feature] })[0], /decision/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test("development harness sources count toward source ceilings but installed packages do not", () => {
  const root = mkdtempSync(join(tmpdir(), "rbit-development-policy-"));
  try {
    mkdirSync(join(root, "devtools/harness/node_modules/dependency"), { recursive: true });
    writeFileSync(join(root, "devtools/harness/runner.mjs"), "export const own = true;\n");
    writeFileSync(join(root, "devtools/harness/node_modules/dependency/index.js"), "third party\n");
    assert.deepEqual(sourceFiles(root), ["devtools/harness/runner.mjs"]);
    assert.equal(checkSizes(new Map([["devtools/harness/runner.mjs", "x\n".repeat(401)]]),
      { authoredSizeBaseline: [] }).length, 1);
  } finally { rmSync(root, { recursive: true, force: true }); }
});


test("locale adapters accept exact model contracts without widening application boundaries", () => {
  const modelEdge = () => ({ name: "rbit-identity-models", kind: null,
    uses_default_features: false, features: [] });
  const packages = [
    ...["rbit-services", "rbit-web"].map(name => ({ id: name, name, dependencies: [modelEdge()] })),
    ...["rbit-identity-models", "rbit-identity", "rbit-kernel", "rbit-storage"].map(name => (
      { id: name, name, dependencies: [] })),
  ];
  const metadata = { packages, workspace_members: packages.map(pkg => pkg.id) };
  const baseline = { crates: ["rbit-services", "rbit-web", "rbit-storage"].map(name => (
    { name, internalDependencies: [] })) };
  assert.deepEqual(checkEdges(metadata, baseline), []);
  for (const adapter of packages.slice(0, 2)) {
    adapter.dependencies[0].features = ["password-work"];
    assert.match(checkEdges(metadata, baseline).join("\n"), /contract dependency must disable defaults and request no features/);
    adapter.dependencies[0].features = [];
    adapter.dependencies[0].uses_default_features = true;
    assert.match(checkEdges(metadata, baseline).join("\n"), /contract dependency must disable defaults and request no features/);
    adapter.dependencies[0].uses_default_features = false;
  }
  const web = packages[1];
  for (const name of ["rbit-identity", "rbit-kernel", "rbit-storage"]) {
    web.dependencies.push({ name, kind: null });
    assert.match(checkEdges(metadata, baseline).join("\n"), new RegExp(`rbit-web -> ${name}: forbidden dependency`));
    web.dependencies.pop();
  }
  const models = packages.find(pkg => pkg.name === "rbit-identity-models");
  models.dependencies.push({ name: "rbit-storage", kind: null });
  assert.match(checkEdges(metadata, baseline).join("\n"), /rbit-identity-models -> rbit-storage: forbidden dependency/);
});


test("promoted workflow source loses all old physical exemptions and receives the full AST target", () => {
  const baseline = { crates: [{ name: "rbit-workflow" }, { name: "rbit-storage" }], authoredSizeBaseline: [
    { path: "crates/rbit-workflow/src/spec.rs", lines: 1045 },
    { path: "crates/rbit-workflow/tests/conformance.rs", lines: 533 },
    { path: "crates/rbit-storage/src/legacy.rs", lines: 800 },
  ] };
  const files = [
    ["crates/rbit-workflow/src/spec.rs", "x\n".repeat(401)],
    ["crates/rbit-workflow/tests/conformance.rs", "x\n".repeat(401)],
    ["crates/rbit-workflow-copy/source.rs", "x\n".repeat(400)],
    ["crates/rbit-storage/src/legacy.rs", "x\n".repeat(800)],
  ];
  assert.equal(checkSizes(files, baseline).length, 2);
  files[0][1] = "x\n".repeat(400); files[1][1] = "x\n".repeat(400);
  assert.deepEqual(checkSizes(files, baseline), []);
  const metadata = { workspace_members: ["workflow", "storage", "new"], packages: [
    { id: "workflow", name: "rbit-workflow" }, { id: "storage", name: "rbit-storage" },
    { id: "new", name: "rbit-new" }, { id: "dependency", name: "serde" },
  ] };
  assert.deepEqual(strictCrateNames(metadata, baseline), ["rbit-workflow", "rbit-new"]);
});


test("complete mail crate has strict physical and cumulative target coverage", () => {
  const baseline = { crates: [{ name: "rbit-mail" }], authoredSizeBaseline: [
    { path: "crates/rbit-mail/src/messages.rs", lines: 664 },
  ] };
  assert.equal(checkSizes([["crates/rbit-mail/src/messages.rs", "x\n".repeat(401)]], baseline).length, 1);
  assert.deepEqual(checkSizes([["crates/rbit-mail/src/messages/envelope.rs", "x\n".repeat(400)]], baseline), []);
  assert.deepEqual(strictCrateNames({ workspace_members: ["mail"], packages: [{ id: "mail", name: "rbit-mail" }] }, baseline), ["rbit-mail"]);
});
