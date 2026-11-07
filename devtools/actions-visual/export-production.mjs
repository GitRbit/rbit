// CI-only focused nextest build/export plus public runtime assets.
import { execFile } from "node:child_process";
import { constants } from "node:fs";
import { promisify } from "node:util";
import { mkdir, copyFile, chmod, writeFile, realpath } from "node:fs/promises";
import { resolve, dirname, relative, join } from "node:path";
import { fileURLToPath } from "node:url";
import { productionWorkspace, productionAssetRoot, productionTest, fixtureLimits } from "./production-policy.mjs";
import { selectProductionBinary, verifyProductionArtifact } from "./production-artifact.mjs";
import { checkedMount } from "./mounts.mjs";
import { fileEvidence, publicAssets } from "./production-files.mjs";
import { revision } from "./provenance.mjs";
const execute = promisify(execFile);
// Closed target selection: list builds this integration binary without running it.
export const productionBuildArgs = Object.freeze([
  "nextest", "list", "--package", "rbit-web", "--test", "integration", "--locked",
  "--list-type", "binaries-only", "--message-format", "json",
]);

export function assertSourceAssets(assets, tracked) {
  const expected = [...new Set([...tracked, "app.css"])].sort();
  if (assets.map(asset => asset.path).sort().join("\n") !== expected.join("\n")) throw Error("FIXTURE_ASSET_SET_REFUSED");
}

async function main() {
  if (process.argv.length !== 2 || process.platform !== "linux" || process.env.GITHUB_ACTIONS !== "true"
    || process.env.GITHUB_REPOSITORY !== "GitRbit/rbit") throw Error("FIXTURE_EXPORT_CONTEXT_REFUSED");
  const workspace = await realpath(fileURLToPath(new URL("../..", import.meta.url)));
  if (workspace !== productionWorkspace) throw Error("FIXTURE_BUILD_ROOT_REFUSED");
  const run = async (program, args, timeout = 900000) => (await execute(program, args, {
    cwd: workspace, encoding: "utf8", timeout, maxBuffer: 32 * 1024 * 1024, windowsHide: true,
  })).stdout;
  const commit = revision((await run("git", ["rev-parse", "HEAD"])).trim());
  if (commit !== process.env.GITHUB_SHA) throw Error("FIXTURE_REVISION_REFUSED");
  const metadata = JSON.parse(await run("cargo", ["metadata", "--no-deps", "--locked", "--format-version=1"]));
  // Build/reuse only this target in the fixed debug-functional CI environment.
  const listing = JSON.parse(await run("cargo", productionBuildArgs, 2400000));
  const source = await checkedMount(workspace, relative(workspace, selectProductionBinary(metadata, listing)));
  const assetRoot = await checkedMount(workspace, "crates/rbit-web/assets");
  if (assetRoot !== productionAssetRoot) throw Error("FIXTURE_BUILD_ROOT_REFUSED");
  const assets = await publicAssets(assetRoot), binary = await fileEvidence(source, fixtureLimits.binaryBytes);
  const prefix = "crates/rbit-web/assets/";
  const tracked = (await run("git", ["ls-files", "-z", "--", prefix])).split("\0").filter(Boolean).map(path => {
    if (!path.startsWith(prefix)) throw Error("FIXTURE_ASSET_SET_REFUSED");
    return path.slice(prefix.length);
  });
  assertSourceAssets(assets, tracked); // app.css is the only permitted untracked build output.
  const directory = join(workspace, "actions-production-fixture");
  await mkdir(directory, { mode: 0o700 });
  const destination = join(directory, "actions-fixture");
  await copyFile(source, destination, constants.COPYFILE_EXCL);
  await chmod(destination, 0o500);
  if (JSON.stringify(await fileEvidence(destination, fixtureLimits.binaryBytes)) !== JSON.stringify(binary)) throw Error("FIXTURE_COPY_DRIFT");
  const publicRoot = join(directory, "public-assets");
  await mkdir(publicRoot, { mode: 0o700 });
  for (const asset of assets) {
    const target = join(publicRoot, asset.path);
    await mkdir(dirname(target), { recursive: true, mode: 0o700 });
    await copyFile(await checkedMount(assetRoot, asset.path), target, constants.COPYFILE_EXCL);
    await chmod(target, 0o400);
  }
  if (JSON.stringify(await publicAssets(publicRoot)) !== JSON.stringify(assets)) throw Error("FIXTURE_COPY_DRIFT");
  const lock = await fileEvidence(await checkedMount(workspace, "Cargo.lock"), 4 * 1024 * 1024);
  const manifest = { schema: 1, revision: commit, runtime: "debug-functional", platform: "linux-x86_64",
    assetRoot: productionAssetRoot, test: productionTest, binary, assets, lockSha256: lock.sha256 };
  await writeFile(join(directory, "fixture.json"), JSON.stringify(manifest) + "\n", { flag: "wx", mode: 0o400 });
  await verifyProductionArtifact(workspace, commit);
  console.log("Production fixture: exported one exact binary and public-only assets");
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { await main(); }
  catch (error) { console.error(/^FIXTURE_[A-Z_]+$/.test(error.message) ? error.message : "FIXTURE_EXPORT_FAILED"); process.exitCode = 1; }
}
