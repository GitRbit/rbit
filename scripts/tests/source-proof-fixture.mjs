import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, realpath, rm, writeFile, utimes } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { boundedInput } from "../workspace/retention/policy.mjs";
import { cargo as sharedCargo } from "../workspace/build.mjs";
import { absentInput } from "../workspace/source-proof/inputs.mjs";
export const namespace = "1".repeat(20);
export async function sourceFixture(t) {
  const root = await realpath(await mkdtemp(join(tmpdir(), "rbit-source-proof-")));
  t.after(() => rm(root, { recursive: true, force: true }));
  const git = (cwd, args) => execFileSync("git", args, { cwd, windowsHide: true, stdio: "ignore" });
  git(root, ["init"]);
  await mkdir(join(root, "tools/model/src"), { recursive: true });
  await mkdir(join(root, "deploy"));
  await writeFile(join(root, ".gitignore"), ".build-cache/\n.worktree-state/\nworktrees/\n");
  await writeFile(join(root, "Cargo.toml"), '[workspace]\nmembers = [\n "tools/model",\n]\n');
  await writeFile(join(root, "Cargo.lock"), "version = 4\n");
  await writeFile(join(root, "tools/model/Cargo.toml"), '[package]\nname="model"\nversion="0.1.0"\n');
  await writeFile(join(root, "tools/model/src/lib.rs"), "pub const VALUE: &str = \"A\";\n");
  await writeFile(join(root, "deploy/example.toml"), "mode = 'A'\n");
  git(root, ["add", "."]);
  git(root, ["-c", "user.name=ChaseTheCode", "-c", "user.email=60942013+ChaseTheCode@users.noreply.github.com", "commit", "-m", "Synthetic source proof fixture"]);
  for (const name of ["a", "b", "c"]) git(root, ["worktree", "add", "--detach", join(root, "worktrees", name), "HEAD"]);
  const cache = join(root, ".build-cache"), lane = join(cache, namespace, "shared");
  await mkdir(lane, { recursive: true });
  await mkdir(join(root, "cargo-home"));
  await mkdir(join(root, ".worktree-state"));
  const context = name => ({ root, checkout: join(root, "worktrees", name), cache, state: join(root, ".worktree-state", name) });
  const metadata = async ctx => {
    const path = join(ctx.checkout, "tools/model");
    const id = `path+${pathToFileURL(path).href}#model@0.1.0`;
    return { packages: [{ id, name: "model", source: null, manifest_path: join(path, "Cargo.toml"),
      targets: [{ name: "model", src_path: join(path, "src/lib.rs"), kind: ["lib"] }] }], resolve: { nodes: [{ id, dependencies: [] }] } };
  };
  const readPolicy = async (path, optional, maximum) => {
    if (!path.startsWith(root)) { if (optional) return null; throw new Error("Fixture external input refused"); }
    return boundedInput(path, optional, maximum);
  };
  const request = () => ({ lane, namespace, command: "check", args: ["-p", "model"], env: { CARGO_HOME: join(root, "cargo-home") } });
  async function unit(hash = "a".repeat(16), { profile = "debug", features = [], content = "A", stamp = true, kind = "lib", rootName = "model", targetName = "model" } = {}) {
    const directory = join(lane, "intermediates", profile, ".fingerprint", `${rootName}-${hash}`);
    const deps = join(lane, "intermediates", profile, "deps");
    await mkdir(directory, { recursive: true }); await mkdir(deps, { recursive: true });
    const control = `${kind}-${targetName}`;
    if (stamp) await writeFile(join(directory, control), "0123456789abcdef");
    await writeFile(join(directory, `${control}.json`), JSON.stringify({ rustc: 1, target: 1, profile: 1, features: JSON.stringify(features), deps: [], local: [], rustflags: [] }));
    await writeFile(join(directory, `dep-${control}`), "fixed depinfo");
    await writeFile(join(directory, "invoked.timestamp"), "fixed Cargo marker");
    const artifact = join(deps, `libmodel-${hash}.rmeta`);
    await writeFile(artifact, content);
    await writeFile(join(deps, `model-${hash}.d`), `${artifact}: tools/model/src/lib.rs\n`);
    return { hash, directory, artifact, stamp: join(directory, control), features, profile };
  }
  async function message(ctx, value, fresh = false) {
    const data = await metadata(ctx);
    return { reason: "compiler-artifact", package_id: data.packages[0].id, target: data.packages[0].targets[0],
      filenames: [value.artifact], features: value.features, fresh };
  }
  async function change(ctx, value, old = false) {
    const file = join(ctx.checkout, "tools/model/src/lib.rs");
    await writeFile(file, `pub const VALUE: &str = \"${value}\";\n`);
    if (old) await utimes(file, 1, 1);
  }
  // The reader deliberately models external config as absent without inspecting the
  // real user's files; model its external stability the same way. Internal missing
  // config paths use the real observer, including the ABA regression.
  const absentPolicy = async (path, budget) => { if (path.startsWith(root)) await absentInput(path, budget); };
  return { root, context, lane, metadata, readPolicy, absentPolicy, request, unit, message, change, readFile };
}

// Old helper behavior tests deliberately isolate their executor/lease assertions.
// Source-provenance tests above use the real guard and real temporary Cargo layouts.
export const fixtureProvenance = Object.freeze({
  begin: async () => ({ handle: Object.freeze({}), observe: () => {} }),
  finish: async () => ({ trusted: true, fixture: true }), fail: async () => {},
});
export function cargo(ctx, command, args = [], options = {}) {
  return sharedCargo(ctx, command, args, { provenance: fixtureProvenance, ...options });
}
