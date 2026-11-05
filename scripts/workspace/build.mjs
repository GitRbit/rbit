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
  if (!Array.isArray(args) || args.some((arg) => typeof arg !== "string" || arg.includes("\0"))) throw new Error("Cargo arguments must be strings");
  if (args.some((arg) => /^(--target-dir|--build-dir|--artifact-dir|--out-dir|--lockfile-path|--config|--cargo-config|--manifest-path|--message-format)(=|$)/.test(arg) || /^-C/.test(arg))) {
    throw new Error("Output/config overrides are not supported by the shared build helper");
  }
}

export function cargoProcess(ctx, args, env, onArtifact = () => {}, { spawnProcess = spawn, output = process.stdout, diagnostics = process.stderr } = {}) {
  return new Promise((done, reject) => {
    const child = spawnProcess("cargo", args, { cwd: ctx.checkout, env, windowsHide: true, stdio: ["inherit", "pipe", "inherit"] });
    const lines = createInterface({ input: child.stdout });
    let parseError, spawnError;
    lines.on("line", (line) => {
      let message;
      try { message = JSON.parse(line); }
      catch { output.write(`${line}\n`); return; }
      try {
        if (!message || typeof message !== "object" || !message.reason) { output.write(`${line}\n`); return; }
        if (env.RBIT_WORKSPACE_JSON === "1") output.write(`${line}\n`);
        if (message.reason === "compiler-artifact" && message.executable) onArtifact(message);
        else if (env.RBIT_WORKSPACE_JSON !== "1" && message.reason === "compiler-message" && message.message.rendered) diagnostics.write(message.message.rendered);
      } catch (error) {
        parseError = error;
      }
    });
    // Default interruption deliberately leaves the lease behind. This prevents another
    // build from reusing output while orphaned compiler descendants may still be alive.
    // Even spawn errors settle on close, after stdio handles/children have closed.
    child.on("error", (error) => { spawnError = error; });
    child.on("close", (code, signal) => {
      if (spawnError || parseError) reject(spawnError ?? parseError);
      else if (code !== 0) reject(new Error(`Cargo ${signal ?? `exited ${code}`}`));
      else done();
    });
  });
}

/** Only helpers acquire this lease. Direct Cargo/IDE builds must not use this lane. */
export async function cargo(ctx, command, args = [], { execute = cargoProcess, env = process.env, key } = {}) {
  if (!["build", "check", "clippy", "test", "nextest", "generate-lockfile"].includes(command)) throw new Error("Unsupported Cargo command");
  validateCargoArgs(args);
  if (command === "nextest" && !["run", "list"].includes(args[0])) throw new Error("Use nextest run or list");
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
    buildEnv.CARGO_BUILD_TARGET_DIR = buildEnv.CARGO_TARGET_DIR;
    buildEnv.CARGO_BUILD_BUILD_DIR = join(lane, "intermediates");
    const artifacts = [];
    const json = ["build", "check", "clippy", "test"].includes(command);
    const format = buildEnv.RBIT_WORKSPACE_JSON === "1" ? "json" : "json-render-diagnostics";
    await execute(ctx, [command, ...(json ? [`--message-format=${format}`] : []), ...args], buildEnv, (item) => artifacts.push(item));
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
