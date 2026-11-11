import assert from "node:assert/strict";
import test from "node:test";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { mkdtemp, mkdir, writeFile, readFile, rm, symlink, link } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { selectProductionBinary, verifyProductionArtifact } from "../../devtools/actions-visual/production-artifact.mjs";
import { fileEvidence, publicAssets } from "../../devtools/actions-visual/production-files.mjs";
import { assertSourceAssets } from "../../devtools/actions-visual/export-production.mjs";
import { productionOrigin, productionWorkspace, productionAssetRoot, productionTest,
  allowedProductionRequest, publicAssetPath } from "../../devtools/actions-visual/production-policy.mjs";

const revision = "a".repeat(40);
test("the exact public asset policy covers every tracked runtime asset", () => {
  const prefix = "crates/rbit-web/assets/";
  const root = fileURLToPath(new URL("../../", import.meta.url));
  const tracked = execFileSync("git", ["ls-files", "-z", "--", prefix], { cwd: root, encoding: "utf8", windowsHide: true })
    .split("\0").filter(Boolean).map(path => path.slice(prefix.length));
  assert.ok(tracked.length > 0);
  assert.deepEqual(tracked.filter(path => !publicAssetPath(path)), []);
  assert.equal(publicAssetPath("mermaid-frame.html"), true);
  for (const path of ["private.html", "mermaid-frame.html/secret.js", "secret.key", "runtime.redb"]) {
    assert.equal(publicAssetPath(path), false);
  }
});
test("only tracked public assets and the known fresh CSS build output may be exported", () => {
  const tracked = ["app.bundle.js", "vendor/htmx.min.js"];
  const assets = [...tracked, "app.css", "deferred.css"].map(path => ({ path }));
  assertSourceAssets(assets, tracked);
  assert.throws(() => assertSourceAssets([...assets, { path: "runtime.js" }], tracked), /ASSET_SET_REFUSED/);
  assert.throws(() => assertSourceAssets(assets.slice(1), tracked), /ASSET_SET_REFUSED/);
});
test("nextest identity selects exactly the existing Linux web integration target, never a basename guess", () => {
  const metadata = { packages: [{ name: "rbit-web", id: "web", manifest_path: `${productionWorkspace}/crates/rbit-web/Cargo.toml` }] };
  const binary = { "package-id": "web", "binary-name": "integration", kind: "test", "build-platform": "target",
    "binary-path": `${productionWorkspace}/target/debug/deps/integration-123abc` };
  assert.equal(selectProductionBinary(metadata, { "rust-binaries": { web: binary } }), binary["binary-path"]);
  for (const change of [{ "package-id": "other" }, { kind: "bin" }, { "build-platform": "host" },
    { "binary-path": "/private/integration-123abc" }, { "binary-path": `${productionWorkspace}/target/release/deps/integration-123abc` }]) {
    assert.throws(() => selectProductionBinary(metadata, { "rust-binaries": { web: { ...binary, ...change } } }));
  }
  assert.throws(() => selectProductionBinary(metadata, { "rust-binaries": { first: binary, second: binary } }), /IDENTITY_REFUSED/);
  assert.throws(() => selectProductionBinary({ packages: [{ ...metadata.packages[0], manifest_path: "/private/Cargo.toml" }] },
    { "rust-binaries": { web: binary } }), /BUILD_ROOT_REFUSED/);
});

test("production browser egress permits fixed synthetic reads and public asset types only", () => {
  for (const path of ["/alice/rocket/actions/runs/1", "/alice/rocket/actions/runs/2", "/alice/rocket/actions/runs/2/jobs/2",
    "/alice/rocket/actions/runs/2/jobs/2/logs?step=0", "/alice/rocket/actions/runs/2/jobs/2/logs/raw",
    "/alice/rocket/actions/runs/2/jobs/2/logs/live?from=0", "/alice/rocket/actions/runs/2/jobs/2/logs/live?from=1207",
    "/alice/rocket/commits", "/assets/app.bundle.js?v=123", "/assets/vendor/htmx.min.js"]) {
    assert.equal(allowedProductionRequest(productionOrigin + path, "GET"), true);
  }
  for (const path of ["/alice/rocket/actions/runs/3", "/alice/private/actions/runs/1", "/api/v1/users", "/assets/.env", "/assets/private.sqlite"]) {
    assert.equal(allowedProductionRequest(productionOrigin + path, "GET"), false);
  }
  for (const path of ["/alice/rocket/actions/runs/2/jobs/3", "/alice/rocket/actions/runs/2/jobs/2/logs?step=9",
    "/alice/rocket/actions/runs/2/jobs/2/logs?step=0&step=1", "/alice/rocket/actions/runs/2/jobs/2/logs/raw?file=private",
    "/alice/rocket/actions/runs/2/jobs/2/logs/live", "/alice/rocket/actions/runs/2/jobs/2/logs/live?from=x",
    "/alice/rocket/actions/runs/2/jobs/2/logs/live?from=0&from=1", "/alice/rocket/actions/runs/2/jobs/2/logs/live?from=0&step=1",
    "/alice/rocket/actions/runs/2/jobs/3/logs/live?from=0"]) {
    assert.equal(allowedProductionRequest(productionOrigin + path, "GET"), false);
  }
  for (const method of ["POST", "PUT", "DELETE", "CONNECT"]) assert.equal(allowedProductionRequest(productionOrigin + "/alice/rocket/actions/runs/1", method), false);
  assert.equal(allowedProductionRequest("http://localhost:4189/alice/rocket/actions/runs/1", "GET"), false);
  assert.equal(allowedProductionRequest("https://example.test/", "GET"), false);
  for (const path of [".env", "../app.js", "vendor/../../private.js", "data.sqlite", "runtime.redb", "config.toml", "private/key.pem"]) {
    assert.equal(publicAssetPath(path), false);
  }
});

async function artifact(root) {
  const directory = join(root, "actions-production-fixture"), assets = join(directory, "public-assets");
  await mkdir(join(assets, "vendor"), { recursive: true });
  await writeFile(join(assets, "app.css"), "body{}");
  await writeFile(join(assets, "deferred.css"), ".lpk{}");
  await writeFile(join(assets, "app.bundle.js"), "export {};");
  await writeFile(join(assets, "mermaid-frame.html"), "<!doctype html><p>public diagram frame</p>");
  await writeFile(join(assets, "vendor/htmx.min.js"), "void 0;");
  const elf = Buffer.alloc(20); elf.set([0x7f, 0x45, 0x4c, 0x46, 2, 1]); elf.writeUInt16LE(62, 18);
  await writeFile(join(directory, "actions-fixture"), elf);
  const manifest = { schema: 1, revision, runtime: "debug-functional", platform: "linux-x86_64", assetRoot: productionAssetRoot,
    test: productionTest, binary: await fileEvidence(join(directory, "actions-fixture"), 1024), assets: await publicAssets(assets), lockSha256: "b".repeat(64) };
  await writeFile(join(directory, "fixture.json"), JSON.stringify(manifest));
  return { directory, assets, manifest };
}

test("artifact verification rejects drift, wrong revision, private additions and credential-shaped manifest additions", async () => {
  const root = await mkdtemp(join(tmpdir(), "rbit-production-artifact-"));
  try {
    const owned = await artifact(root);
    assert.equal((await verifyProductionArtifact(root, revision)).manifest.runtime, "debug-functional");
    await assert.rejects(verifyProductionArtifact(root, "c".repeat(40)), /MANIFEST_REFUSED/);
    await writeFile(join(owned.directory, "fixture.json"), JSON.stringify({ ...owned.manifest, cookie: "secret" }));
    await assert.rejects(verifyProductionArtifact(root, revision), /MANIFEST_REFUSED/);
    await writeFile(join(owned.directory, "fixture.json"), JSON.stringify(owned.manifest));
    await writeFile(join(owned.directory, "private.sqlite"), "not user data");
    await assert.rejects(verifyProductionArtifact(root, revision), /EXTRA_FILE/);
    await rm(join(owned.directory, "private.sqlite"));
    await writeFile(join(owned.assets, "app.css"), "changed{}");
    await assert.rejects(verifyProductionArtifact(root, revision), /HASH_MISMATCH/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test("asset link refusal preserves the external sentinel", async t => {
  const root = await mkdtemp(join(tmpdir(), "rbit-production-links-"));
  try {
    const owned = await artifact(root), outside = join(root, "sentinel.js");
    await writeFile(outside, "PRIVATE");
    try { await symlink(outside, join(owned.assets, "redirect.js")); }
    catch (error) { if (["EPERM", "EACCES"].includes(error.code)) { t.skip("symlink privilege unavailable"); return; } throw error; }
    await assert.rejects(publicAssets(owned.assets), /LINK_REFUSED/);
    await rm(join(owned.assets, "redirect.js"));
    await link(outside, join(owned.assets, "hardlink.js"));
    await assert.rejects(publicAssets(owned.assets), /LINK_REFUSED/);
    assert.equal(await readFile(outside, "utf8"), "PRIVATE");
  } finally { await rm(root, { recursive: true, force: true }); }
});
