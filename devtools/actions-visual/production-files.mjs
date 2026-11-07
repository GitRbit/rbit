// Public asset/binary handoff checks shared by the exporter and contained owner.
import { constants } from "node:fs";
import { lstat, readdir, open } from "node:fs/promises";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { checkedMount } from "./mounts.mjs";
import { fixtureLimits, publicAssetPath } from "./production-policy.mjs";

export async function requireLinuxBinary(path) {
  const handle = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  try {
    const bytes = Buffer.alloc(20);
    const result = await handle.read(bytes, 0, bytes.length, 0);
    if (result.bytesRead !== 20 || bytes.subarray(0, 4).toString("hex") !== "7f454c46"
      || bytes[4] !== 2 || bytes[5] !== 1 || bytes.readUInt16LE(18) !== 62) throw Error("FIXTURE_BINARY_PLATFORM_REFUSED");
  } finally { await handle.close(); }
}

export async function fileEvidence(path, maximum) {
  const before = await lstat(path);
  if (!before.isFile() || before.isSymbolicLink() || before.size > maximum) throw Error("FIXTURE_FILE_REFUSED");
  const handle = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  try {
    const opened = await handle.stat();
    if (opened.dev !== before.dev || opened.ino !== before.ino) throw Error("FIXTURE_FILE_DRIFT");
    const hash = createHash("sha256");
    let bytes = 0;
    for await (const chunk of handle.createReadStream({ autoClose: false })) {
      bytes += chunk.length;
      if (bytes > maximum) throw Error("FIXTURE_FILE_LIMIT");
      hash.update(chunk);
    }
    const after = await lstat(path), descriptor = await handle.stat();
    for (const state of [after, descriptor]) {
      if (bytes !== before.size || before.dev !== state.dev || before.ino !== state.ino || before.size !== state.size
        || before.mtimeMs !== state.mtimeMs || before.ctimeMs !== state.ctimeMs) throw Error("FIXTURE_FILE_DRIFT");
    }
    return { bytes, sha256: hash.digest("hex") };
  } finally { await handle.close(); }
}

export async function publicAssets(root) {
  const result = [];
  let total = 0, directories = 0;
  async function walk(relative = "") {
    for (const entry of await readdir(join(root, relative), { withFileTypes: true })) {
      const path = relative ? `${relative}/${entry.name}` : entry.name;
      const source = await checkedMount(root, path);
      if (entry.isDirectory()) {
        if (!publicAssetPath(`${path}/placeholder.js`) || ++directories > fixtureLimits.assetFiles) throw Error("FIXTURE_ASSET_DIRECTORY_REFUSED");
        await walk(path);
      }
      else {
        if (!entry.isFile() || !publicAssetPath(path)) throw Error("FIXTURE_ASSET_REFUSED");
        if ((await lstat(source)).nlink !== 1) throw Error("FIXTURE_ASSET_LINK_REFUSED");
        const evidence = await fileEvidence(source, fixtureLimits.assetBytes);
        result.push({ path, ...evidence }); total += evidence.bytes;
        if (result.length > fixtureLimits.assetFiles || total > fixtureLimits.assetBytes) throw Error("FIXTURE_ASSET_LIMIT");
      }
    }
  }
  await walk();
  result.sort((left, right) => left.path.localeCompare(right.path, "en"));
  if (!["app.css", "app.bundle.js", "vendor/htmx.min.js"].every(path => result.some(file => file.path === path))) {
    throw Error("FIXTURE_REQUIRED_ASSET_MISSING");
  }
  return result;
}
