// Canonical paths and guarded filesystem operations for shared developer storage.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, mkdir, realpath } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";

export function git(cwd, args) {
  return execFileSync("git", args, { cwd, encoding: "utf8", windowsHide: true }).trim();
}

export function contains(root, path) {
  const child = relative(resolve(root), resolve(path));
  return child !== "" && child !== ".." && !child.startsWith(`..${sep}`) && !isAbsolute(child);
}

/** Reject symlink/reparse traversal, including an existing leaf, before a mutation. */
export async function guarded(root, path) {
  const requestedRoot = resolve(root);
  path = resolve(path);
  root = await realpath(requestedRoot);
  // Windows TEMP may name the same root through an 8.3 alias. Translate only
  // descendants of the supplied root; resolving the whole candidate would hide
  // symlinks below it before the traversal checks have inspected them.
  if (contains(requestedRoot, path)) path = resolve(root, relative(requestedRoot, path));
  if (!contains(root, path)) throw new Error(`Path is outside managed root: ${path}`);
  let cursor = root;
  for (const part of relative(root, path).split(sep)) {
    cursor = join(cursor, part);
    try {
      if ((await lstat(cursor)).isSymbolicLink()) throw new Error(`Linked paths are not managed: ${cursor}`);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
  return path;
}

export async function context(cwd = process.cwd()) {
  const checkout = await realpath(git(cwd, ["rev-parse", "--show-toplevel"]));
  const common = await realpath(git(checkout, ["rev-parse", "--path-format=absolute", "--git-common-dir"]));
  // This repository uses a conventional primary checkout, not a bare administration repo.
  const root = await realpath(dirname(common));
  if (resolve(common) !== join(root, ".git")) throw new Error("A primary checkout with .git is required");
  const id = createHash("sha256").update(checkout).digest("hex").slice(0, 16);
  return { root, checkout, id, state: join(root, ".worktree-state", id), cache: join(root, ".build-cache") };
}

export async function managedDirectory(ctx, path) {
  await guarded(ctx.root, path);
  await mkdir(path, { recursive: true });
  await guarded(ctx.root, path);
  return path;
}

export function featureName(name) {
  if (!/^[a-z0-9][a-z0-9-]{0,79}$/.test(name ?? "")) throw new Error("Use a lowercase feature name with letters, digits and hyphens");
  return name;
}
