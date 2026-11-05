// Shared Cargo execution and exact, generation-specific binary handoff.
import { spawn, execFileSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { copyFile, writeFile } from "node:fs/promises";
import { basename, join } from "node:path";
import { createInterface } from "node:readline";
import { guarded, managedDirectory } from "./paths.mjs";
import { leased } from "./lease.mjs";

export function compatibility(env = process.env, rustc = execFileSync("rustc", ["-vV"], { encoding: "utf8", windowsHide: true })) {
  const flags = Object.fromEntries(Object.entries(env).filter(([key]) =>
    /^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|LIBSQLITE3_FLAGS|CC|CXX|CFLAGS|CARGO_BUILD_TARGET|CARGO_TARGET_.*_RUSTFLAGS)$/.test(key)));
  return createHash("sha256").update(JSON.stringify({ rustc, platform: process.platform, arch: process.arch, flags })).digest("hex").slice(0, 20);
}

export function validateCargoArgs(args) {
  // Output location flags would bypass the lane; custom config can override build-dir.
  if (args.some((arg) => /^(--target-dir|--config|--manifest-path|--message-format)(=|$)/.test(arg))) {
    throw new Error("Output/config overrides are not supported by the shared build helper");
  }
}

export function cargoProcess(ctx, args, env, onArtifact = () => {}) {
  return new Promise((done, reject) => {
    const child = spawn("cargo", args, { cwd: ctx.checkout, env, windowsHide: true, stdio: ["inherit", "pipe", "inherit"] });
    const lines = createInterface({ input: child.stdout });
    let parseError;
    lines.on("line", (line) => {
      try {
        const message = JSON.parse(line);
        if (env.RBIT_WORKSPACE_JSON === "1") process.stdout.write(`${line}\n`);
        if (message.reason === "compiler-artifact" && message.executable) onArtifact(message);
        else if (env.RBIT_WORKSPACE_JSON !== "1" && message.reason === "compiler-message" && message.message.rendered) process.stderr.write(message.message.rendered);
        else if (!message.reason) process.stdout.write(`${line}\n`);
      } catch (error) {
        // Non-JSON output from tests/nextest is normal; callback failures are fatal.
        if (error instanceof SyntaxError) process.stdout.write(`${line}\n`);
        else parseError = error;
      }
    });
    // Default interruption deliberately leaves the lease behind. This prevents another
    // build from reusing output while orphaned compiler descendants may still be alive.
    child.on("error", reject);
    child.on("close", (code, signal) => {
      if (parseError) reject(parseError);
      else if (code !== 0) reject(new Error(`Cargo ${signal ?? `exited ${code}`}`));
      else done();
    });
  });
}

/** Only helpers acquire this lease. Direct Cargo/IDE builds must not use this lane. */
export async function cargo(ctx, command, args = [], { execute = cargoProcess, env = process.env, key } = {}) {
  validateCargoArgs(args);
  const buildEnv = { ...env, LIBSQLITE3_FLAGS: [env.LIBSQLITE3_FLAGS ?? "", "-DSQLITE_ENABLE_QPSG"].join(" ").trim() };
  const namespace = key ?? compatibility(buildEnv, execFileSync("rustc", ["-vV"], {
    cwd: ctx.checkout, env: buildEnv, encoding: "utf8", windowsHide: true,
  }));
  if (!/^[a-f0-9]{20}$/.test(namespace)) throw new Error("Invalid compatibility namespace");
  const cache = await managedDirectory(ctx, ctx.cache);
  // A repository-wide lease also serializes prune and caps active developer build lanes
  // at one. Namespaces separate compiler identities, not worktrees or branches.
  return leased(join(cache, ".lease"), async () => {
    const lane = await managedDirectory(ctx, join(cache, namespace, "shared"));
    buildEnv.CARGO_TARGET_DIR = join(lane, "target");
    buildEnv.CARGO_BUILD_BUILD_DIR = join(lane, "intermediates");
    const artifacts = [];
    const json = ["build", "check", "clippy", "test"].includes(command);
    await execute(ctx, [command, ...(json ? ["--message-format=json-render-diagnostics"] : []), ...args], buildEnv, (item) => artifacts.push(item));
    const result = { executables: [], environment: buildEnv, namespace };
    if (command === "build") {
      const generation = await managedDirectory(ctx, join(ctx.state, "bin", randomUUID()));
      for (const item of artifacts.filter((entry) => entry.target.kind.includes("bin"))) {
        await guarded(lane, item.executable);
        const destination = join(generation, basename(item.executable));
        await copyFile(item.executable, destination);
        result.executables.push({ name: item.target.name, path: destination });
      }
      await writeFile(join(generation, "build.json"), JSON.stringify({ namespace, executables: result.executables }, null, 2));
      await writeFile(join(generation, "handoff.json"), JSON.stringify({ pid: process.pid, host: (await import("node:os")).hostname() }));
    }
    await writeFile(join(cache, namespace, "last-used"), String(Date.now()));
    return result;
  });
}
