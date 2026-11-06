// Exact CI binary identity and a bounded public-only input manifest.
import { lstat, readFile, readdir } from "node:fs/promises";
import { posix } from "node:path";
import { fixtureLimits, productionAssetRoot, productionWorkspace, productionTest } from "./production-policy.mjs";
import { checkedMount } from "./mounts.mjs";
import { fileEvidence, publicAssets, requireLinuxBinary } from "./production-files.mjs";

const hash = value => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const exactKeys = (value, keys) => value && typeof value === "object" && !Array.isArray(value)
  && Object.keys(value).sort().join(",") === [...keys].sort().join(",");

export function selectProductionBinary(metadata, listing) {
  const packages = metadata.packages?.filter(pkg => pkg.name === "rbit-web") ?? [];
  if (packages.length !== 1 || typeof packages[0].id !== "string" || typeof packages[0].manifest_path !== "string"
    || posix.resolve(packages[0].manifest_path) !== `${productionWorkspace}/crates/rbit-web/Cargo.toml`) {
    throw Error("FIXTURE_BUILD_ROOT_REFUSED");
  }
  const candidates = Object.values(listing?.["rust-binaries"] ?? {}).filter(binary => binary && binary["package-id"] === packages[0].id
    && binary["binary-name"] === "integration" && binary.kind === "test" && binary["build-platform"] === "target");
  if (candidates.length !== 1) throw Error("FIXTURE_BINARY_IDENTITY_REFUSED");
  const path = candidates[0]["binary-path"];
  if (typeof path !== "string" || !posix.isAbsolute(path)) throw Error("FIXTURE_BINARY_PATH_REFUSED");
  const local = posix.relative(productionWorkspace, posix.resolve(path));
  if (posix.isAbsolute(local) || local.startsWith("../") || !/^(?:target|\.cargo-build)\/debug\/deps\/integration-[a-f0-9]+$/.test(local)) {
    throw Error("FIXTURE_BINARY_PATH_REFUSED");
  }
  return path;
}

export function validateFixtureManifest(manifest, revision) {
  if (!exactKeys(manifest, ["schema", "revision", "runtime", "platform", "assetRoot", "test", "binary", "assets", "lockSha256"])
    || manifest.schema !== 1 || !/^[a-f0-9]{40}$/.test(revision ?? "") || manifest.revision !== revision
    || manifest.runtime !== "debug-functional" || manifest.platform !== "linux-x86_64"
    || manifest.assetRoot !== productionAssetRoot || manifest.test !== productionTest || !hash(manifest.lockSha256)
    || !exactKeys(manifest.binary, ["bytes", "sha256"]) || !Number.isSafeInteger(manifest.binary.bytes)
    || manifest.binary.bytes <= 0 || manifest.binary.bytes > fixtureLimits.binaryBytes || !hash(manifest.binary.sha256)
    || !Array.isArray(manifest.assets) || manifest.assets.length > fixtureLimits.assetFiles) throw Error("FIXTURE_MANIFEST_REFUSED");
}

export async function verifyProductionArtifact(workspace, revision) {
  const directory = await checkedMount(workspace, "actions-production-fixture");
  if ((await readdir(directory)).sort().join(",") !== "actions-fixture,fixture.json,public-assets") throw Error("FIXTURE_INPUT_EXTRA_FILE");
  const file = await checkedMount(directory, "fixture.json");
  const info = await lstat(file);
  if (!info.isFile() || info.nlink !== 1 || info.size > fixtureLimits.manifestBytes) throw Error("FIXTURE_MANIFEST_LIMIT");
  let manifest;
  try { manifest = JSON.parse(await readFile(file, "utf8")); } catch { throw Error("FIXTURE_MANIFEST_REFUSED"); }
  validateFixtureManifest(manifest, revision);
  const binary = await checkedMount(directory, "actions-fixture"), assets = await checkedMount(directory, "public-assets");
  if ((await lstat(binary)).nlink !== 1) throw Error("FIXTURE_BINARY_LINK_REFUSED");
  await requireLinuxBinary(binary);
  if (JSON.stringify(await fileEvidence(binary, fixtureLimits.binaryBytes)) !== JSON.stringify(manifest.binary)
    || JSON.stringify(await publicAssets(assets)) !== JSON.stringify(manifest.assets)) throw Error("FIXTURE_HASH_MISMATCH");
  return { binary, assets, manifest };
}
