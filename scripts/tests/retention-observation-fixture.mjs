import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, relative, isAbsolute } from "node:path";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import { ownedCargo } from "../workspace/cargo-tree.mjs";
import { cargoProcess } from "../workspace/build.mjs";
import { boundedInput } from "../workspace/retention/policy.mjs";
export const namespace = "a".repeat(20);
export async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "rbit-retention-observe-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const ctx = { root, checkout: root, cache: join(root, "cache"), state: join(root, "state") };
  const lane = join(ctx.cache, namespace, "shared");
  await mkdir(lane, { recursive: true });
  await mkdir(join(root, "crates", "model"), { recursive: true });
  await writeFile(join(root, "Cargo.toml"), '[workspace]\nmembers = [\n "crates/model",\n]\n[workspace.package]\nversion="1.0.0"\n');
  await writeFile(join(root, "Cargo.lock"), "version = 4\n");
  await writeFile(join(root, "crates", "model", "Cargo.toml"), '[package]\nname="model"\nversion="1.0.0"\n');
  // Tests read only their own inputs: no actual user/ancestor Cargo configuration.
  const read = async (path, optional) => {
    const child = relative(root, path);
    if (isAbsolute(child) || child === ".." || child.startsWith(`..${process.platform === "win32" ? "\\" : "/"}`)) {
      if (optional) return null;
      throw new Error("Fixture refuses external input");
    }
    return boundedInput(path, optional);
  };
  const request = { namespace, command: "check", args: ["-p", "model"], env: { CARGO_HOME: join(root, "cargo-home") } };
  return { ctx, lane, read, request };
}
export async function completion(ctx, lease) {
  let launched;
  const started = new Promise(resolve => { launched = resolve; });
  const child = new EventEmitter();
  child.pid = 1234;
  child.stdout = new PassThrough();
  child.stderr = new PassThrough();
  const result = ownedCargo(ctx, [], {}, () => {}, {
    lease,
    native: { resolve: async () => "cargo.exe", launch: () => { launched(); return child; }, verify: async () => true },
    output: { write: () => {} }, diagnostics: { write: () => {} },
  }, (ctx, args, env, artifact, options) => cargoProcess(ctx, args, env, artifact, options));
  await started;
  child.stdout.end();
  child.stderr.end();
  await new Promise(resolve => setImmediate(resolve));
  child.emit("close", 0, null);
  return result;
}
export async function record(ctx) {
  return JSON.parse(await readFile(join(ctx.cache, namespace, "managed-observation.json"), "utf8"));
}
