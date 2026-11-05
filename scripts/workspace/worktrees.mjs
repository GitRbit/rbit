// Git owns checkout registration/removal. No force, recursive filesystem move or delete.
import { readdir, readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { featureName, git, guarded, managedDirectory } from "./paths.mjs";
import { alive, leased } from "./lease.mjs";

export function list(ctx) {
  return git(ctx.checkout, ["worktree", "list", "--porcelain", "-z"])
    .split("\0\0").filter(Boolean).map((block) => Object.fromEntries(block.split("\0").filter(Boolean).map((line) => {
      const index = line.indexOf(" ");
      return index < 0 ? [line, true] : [line.slice(0, index), line.slice(index + 1)];
    })));
}

export async function create(ctx, name, revision = "github/main") {
  featureName(name);
  const parent = await managedDirectory(ctx, join(ctx.root, "worktrees"));
  const path = await guarded(ctx.root, join(parent, name));
  // Resolve an immutable commit before creating the checkout. Fetch is explicit/user controlled.
  const sha = git(ctx.checkout, ["rev-parse", "--verify", `${revision}^{commit}`]);
  git(ctx.checkout, ["check-ref-format", "--branch", `feature/${name}`]);
  await managedDirectory(ctx, ctx.cache);
  await leased(join(ctx.cache, ".lease"), async () => {
    git(ctx.checkout, ["worktree", "add", "-b", `feature/${name}`, path, sha]);
  });
  return { path, branch: `feature/${name}`, revision: sha };
}

async function running(ctx, checkout) {
  const { context } = await import("./paths.mjs");
  const state = (await context(checkout)).state;
  try {
    for (const generation of await readdir(join(state, "bin"))) {
      try {
        const record = JSON.parse(await readFile(join(state, "bin", generation, "running.json"), "utf8"));
        // Unknown/other-host process state fails closed.
        if (record.host !== (await import("node:os")).hostname() || alive(record.pid)) return true;
      } catch (error) { if (error.code !== "ENOENT") throw error; }
    }
  } catch (error) { if (error.code !== "ENOENT") throw error; }
  return false;
}

export async function remove(ctx, name) {
  featureName(name);
  const path = await guarded(ctx.root, join(ctx.root, "worktrees", name));
  if (resolve(path) === resolve(ctx.checkout)) throw new Error("Cannot remove the current checkout");
  await managedDirectory(ctx, ctx.cache);
  return leased(join(ctx.cache, ".lease"), async () => {
    const entry = list(ctx).find((item) => resolve(item.worktree) === resolve(path));
    if (!entry || entry.locked || entry.prunable) throw new Error("Checkout is not registered and removable");
    if (git(path, ["status", "--porcelain", "--untracked-files=all"])) throw new Error("Checkout has uncommitted or untracked work");
    if (await running(ctx, path)) throw new Error("Checkout has an active runtime");
    // Git refuses ignored files too. Never pass --force; branch and state/cache stay intact.
    git(ctx.checkout, ["worktree", "remove", path]);
    return { removed: path };
  }, { timeout: 0 });
}
