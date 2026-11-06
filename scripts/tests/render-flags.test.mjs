import assert from "node:assert/strict";
import test from "node:test";
import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { measurementEnv, cargoArgs } from "../render-compile/options.mjs";
import { flagIdentity, cargoConfigIdentity, sha } from "../render-compile/identity.mjs";
import { assertControlledFlags } from "../render-compile/flags.mjs";

test("plain, encoded, target/build and mixed-case incremental injection cannot enter child flags", () => {
  const hostile = {
    RUSTFLAGS: "-Cincremental=/outside/plain -Copt-level=0",
    CARGO_ENCODED_RUSTFLAGS: "-C\u001fincremental=/outside/encoded",
    CARGO_BUILD_RUSTFLAGS: "-C incremental=/outside/build",
    CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS: "-Cincremental=/outside/target",
    cargo_target_x86_64_unknown_linux_gnu_rustflags: "@/outside/response-file",
    cargo_encoded_rustflags: "-Cincremental=/outside/alias",
    CARGO_INCREMENTAL: "1", cargo_incremental: "1", CARGO_BUILD_INCREMENTAL: "true",
  };
  const env = measurementEnv({ ...process.env, ...hostile });
  assertControlledFlags(env);
  // Inspect a real child's received environment, without starting Cargo/rustc.
  const received = JSON.parse(execFileSync(process.execPath, ["-e", 'console.log(JSON.stringify(Object.fromEntries(Object.entries(process.env).filter(([key])=>/^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|CARGO_BUILD_RUSTFLAGS|CARGO_TARGET_.*_RUSTFLAGS|CARGO_INCREMENTAL|CARGO_BUILD_INCREMENTAL)$/i.test(key)))))'], { env, encoding: "utf8", windowsHide: true }));
  assert.deepEqual(received, { CARGO_ENCODED_RUSTFLAGS: "--cap-lints=allow", CARGO_INCREMENTAL: "0", CARGO_BUILD_INCREMENTAL: "false" });
  assert.equal(JSON.stringify(received).includes("/outside"), false);
});

test("controlled lint policy preserves selected profile and its codegen settings", () => {
  const env = measurementEnv({ CARGO_PROFILE_RELEASE_OPT_LEVEL: "3", CARGO_PROFILE_DEV_DEBUG: "2", CARGO_PROFILE_RELEASE_LTO: "thin", CARGO_PROFILE_DEV_INCREMENTAL: "true" });
  assert.equal(env.CARGO_PROFILE_RELEASE_OPT_LEVEL, "3"); assert.equal(env.CARGO_PROFILE_DEV_DEBUG, "2"); assert.equal(env.CARGO_PROFILE_RELEASE_LTO, "thin");
  // Cargo's explicit global incremental override wins over profile defaults.
  assert.equal(env.CARGO_INCREMENTAL, "0");
  assert.equal(cargoArgs({ mode: "blocks", count: 8 }, { profile: "release", jobs: 1 }).at(-3), "release");
  assert.deepEqual(flagIdentity(env).rustflagsPolicy, { name: "controlled-encoded-v1", source: "CARGO_ENCODED_RUSTFLAGS", flags: ["--cap-lints=allow"] });
  assert.equal(flagIdentity(env).incremental, false);
});

test("reports cannot claim controlled incremental policy after a flag override", () => {
  const safe = measurementEnv({});
  for (const patch of [{ CARGO_ENCODED_RUSTFLAGS: "-Cincremental=outside" }, { CARGO_INCREMENTAL: "1" }, { RUSTFLAGS: "-Cincremental=outside" }, { CARGO_TARGET_FOO_RUSTFLAGS: "@outside" }]) {
    const env = { ...safe, ...patch };
    assert.throws(() => assertControlledFlags(env));
    assert.equal(flagIdentity(env).incremental, "unverified");
    assert.deepEqual(flagIdentity(env).rustflagsPolicy, { name: "unverified" });
  }
});

test("build, target triple/cfg and home config rustflags stay untouched and hashed under encoded precedence", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "rbit-flags-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const checkout = join(root, "source"), home = join(root, "cargo-home");
  await mkdir(join(checkout, ".cargo"), { recursive: true }); await mkdir(home);
  const cases = [
    '[build]\nrustflags = ["-C", "incremental=/outside/build"]\n',
    '[target.x86_64-unknown-linux-gnu]\nrustflags = "-Cincremental=/outside/triple"\n',
    '[target.\'cfg(unix)\']\nrustflags = ["@/outside/response-file"]\n',
  ];
  const homeText = '[build]\nrustflags = ["-Cincremental=/outside/home"]\n';
  await writeFile(join(home, "config"), homeText);
  for (const text of cases) {
    await writeFile(join(checkout, ".cargo", "config.toml"), text);
    const env = measurementEnv({ CARGO_HOME: home });
    const identity = await cargoConfigIdentity({ checkout }, env);
    assert.ok(identity.some((entry) => entry.sha256 === sha(text)));
    assert.ok(identity.some((entry) => entry.sha256 === sha(homeText)));
    assert.equal(await readFile(join(checkout, ".cargo", "config.toml"), "utf8"), text);
    // Cargo's documented first mutually-exclusive source is nonempty encoded flags.
    assert.equal(env.CARGO_ENCODED_RUSTFLAGS, "--cap-lints=allow");
    assert.equal(JSON.stringify(identity).includes("/outside"), false);
    assert.equal(JSON.stringify(identity).includes(root), false);
  }
});
