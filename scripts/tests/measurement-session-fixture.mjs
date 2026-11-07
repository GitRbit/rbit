// Fake compiler frames plus the REAL current source guard/native receipt/lease mechanisms.
import { mkdir, readFile, writeFile, access } from "node:fs/promises";
import { dirname, join } from "node:path";
import { sourceFixture } from "./source-proof-fixture.mjs";
import { completion } from "./retention-observation-fixture.mjs";
import { measureSession } from "../workspace/retention/session.mjs";

export const step = (command = "check", args = ["-p", "model", "--locked"]) => ({ command, args });
export const plan = steps => ({ schema: 1, arms: [{ steps }] });
export async function sessionFixture(t) {
  const f = await sourceFixture(t), ctx = f.context("a");
  const env = { CARGO_HOME: join(f.root, "cargo-home"), RBIT_BUILD_MANAGED_RETENTION: "measure-session",
    RBIT_WORKSPACE_CARGO_TREE: "windows-job", RBIT_BUILD_CANDIDATE_HEADROOM_BYTES: "1" };
  const seen = [];
  async function execute(context, args, environment, artifact, options) {
    const lane = dirname(environment.CARGO_TARGET_DIR), hash = "a".repeat(16);
    const fingerprint = join(lane, "intermediates/debug/.fingerprint", `model-${hash}`);
    const output = join(lane, "intermediates/debug/deps", `libmodel-${hash}.rlib`);
    const stamp = join(fingerprint, "lib-model");
    let fresh = true;
    try { await access(stamp); } catch { fresh = false; }
    if (!fresh) {
      await mkdir(fingerprint, { recursive: true });
      await mkdir(dirname(output), { recursive: true });
      await writeFile(stamp, "0123456789abcdef");
      await writeFile(stamp + ".json", JSON.stringify({ rustc: 1, target: 1, profile: 1, features: "[]", deps: [], local: [], rustflags: [] }));
      await writeFile(join(fingerprint, "dep-lib-model"), "fixed depinfo");
      await writeFile(join(fingerprint, "invoked.timestamp"), "fixed marker");
      await writeFile(output, "synthetic compiler output");
    }
    const pkg = (await f.metadata(ctx)).packages[0];
    options.onMessage({ reason: "compiler-artifact", package_id: pkg.id, target: pkg.targets[0],
      filenames: [output], features: [], fresh });
    seen.push({ args, environment, invocation: options.invocation, fresh });
    options.onExit(0, null);
    return completion(context, options.lease, { invocation: options.invocation });
  }
  const options = { key: f.request().namespace, env, platform: "win32", read: f.readPolicy, proofInputs: f, execute };
  return { ...f, ctx, env, options, seen, execute,
    run: (input, overrides = {}) => measureSession(ctx, input, { ...options, ...overrides }),
    saved: async () => JSON.parse(await readFile(join(ctx.cache, f.request().namespace, "managed/candidate.json"), "utf8")) };
}
