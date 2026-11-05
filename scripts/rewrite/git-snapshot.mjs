/** Read immutable Git objects without checking out, building, or inspecting runtime data. */
import { execFileSync } from "node:child_process";

const MAX_SNAPSHOT_BYTES = 128 * 1024 * 1024;

function git(root, args, options = {}) {
  return execFileSync("git", ["-C", root, ...args], {
    maxBuffer: MAX_SNAPSHOT_BYTES, windowsHide: true, stdio: ["pipe", "pipe", "pipe"], ...options,
  });
}

/** Resolve a commit first so moving references cannot mix revisions within an inventory. */
export function readSnapshot(root, revision, select = () => true) {
  const commit = git(root, ["rev-parse", "--verify", "--end-of-options", `${revision}^{commit}`], { encoding: "utf8" }).trim();
  const paths = git(root, ["ls-tree", "-rz", "--name-only", commit]).toString("utf8")
    .split("\0").filter((path) => path && select(path));
  if (paths.some((path) => /[\r\n]/.test(path))) throw new Error("Snapshot paths must not contain line breaks");
  const input = paths.map((path) => `${commit}:${path}\n`).join("");
  const output = git(root, ["cat-file", "--batch"], { input });
  const files = new Map();
  let offset = 0;
  for (const path of paths) {
    const end = output.indexOf(10, offset);
    if (end < offset) throw new Error(`Missing object header for ${path}`);
    const header = output.subarray(offset, end).toString("utf8");
    const match = /^[a-f0-9]+ blob (\d+)$/.exec(header);
    if (!match) throw new Error(`Unsupported Git object for ${path}: ${header}`);
    const size = Number(match[1]);
    offset = end + 1;
    if (!Number.isSafeInteger(size) || offset + size >= output.length || output[offset + size] !== 10) {
      throw new Error(`Truncated Git object for ${path}`);
    }
    files.set(path, output.subarray(offset, offset + size));
    offset += size + 1;
  }
  if (offset !== output.length) throw new Error("Unexpected trailing snapshot bytes");
  return { commit, files };
}
