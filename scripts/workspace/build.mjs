// Shared Cargo execution and exact, generation-specific binary handoff.
import { spawn, execFileSync } from "node:child_process";
import { childEnvironment } from "./child-environment.mjs";
import { createHash } from "node:crypto";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { guarded, managedDirectory } from "./paths.mjs";
import { leased, UncertainProcessTreeError } from "./lease.mjs";
import { cacheBudget, enforceHighWater } from "./storage.mjs";
import { cargoTreeMode, ownedCargo, verifiedCargoCompletion, failedCargoCompletion } from "./cargo-tree.mjs";
import { managedRetentionMode, beginRetention, finishRetention } from "./retention/observe.mjs";
import { candidateHeadroom, createCandidate, startCandidate, candidateExecutionStarting, candidateExecuted, completeCandidate, quarantineCandidate } from "./retention/candidate.mjs";
import { copyExecutables } from "./artifacts.mjs";
import { sourceProof } from "./source-proof/guard.mjs";

function reportCacheOverage(report) {
  if (report.overageBytes > 0) process.stderr.write(`Build cache remains ${report.overageBytes} bytes over its ${report.maxBytes}-byte soft limit (${report.bytes} bytes retained); current or managed namespaces are protected.\n`);
}

export function compatibility(env = process.env, rustc = execFileSync("rustc", ["-vV"], { encoding: "utf8", windowsHide: true })) {
  const flags = Object.fromEntries(Object.entries(env).filter(([key]) =>
    /^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|LIBSQLITE3_FLAGS|CC|CXX|CFLAGS|CARGO_BUILD_TARGET|CARGO_TARGET_.*_RUSTFLAGS)$/.test(key)).sort(([a], [b]) => a.localeCompare(b)));
  return createHash("sha256").update(JSON.stringify({ rustc, platform: process.platform, arch: process.arch, flags })).digest("hex").slice(0, 20);
}

export function validateCargoArgs(args) {
  // Output location flags would bypass the lane; custom config can override build-dir.
  if (!Array.isArray(args) || args.some((arg) => typeof arg !== "string" || arg.includes("\0"))) throw new Error("Cargo arguments must be strings");
  if (args.some((arg) => /^(--target-dir|--build-dir|--artifact-dir|--out-dir|--lockfile-path|--config|--cargo-config|--manifest-path|--message-format)(=|$)/.test(arg) || /^-C/.test(arg))) {
    throw new Error("Output/config overrides are not supported by the shared build helper");
  }
}

export function cargoProcess(ctx, args, env, onArtifact = () => {}, options = {}) {
  if (cargoTreeMode(env) === "windows-job") return ownedCargo(ctx, args, env, onArtifact, options, consumeCargo);
  return consumeCargo(ctx, args, env, onArtifact, options);
}

function consumeCargo(ctx, args, env, onArtifact = () => {}, { spawnProcess = spawn, output = process.stdout, diagnostics = process.stderr, onMessage = () => {}, captureStderr = false, verifyCompletion, onExit = () => {} } = {}) {
  return new Promise((done, reject) => {
    const child = spawnProcess("cargo", args, { cwd: ctx.checkout, env: childEnvironment(env), windowsHide: true, stdio: ["inherit", "pipe", captureStderr ? "pipe" : "inherit"] });
    let parseError, spawnError;
    if (captureStderr) child.stderr?.on("data", chunk => { try { diagnostics.write(chunk); } catch (error) { parseError = error; } });
    const lines = createInterface({ input: child.stdout });
    const drained = new Promise(resolve => lines.once("close", resolve));
    child.stdout.on("error", error => { parseError ??= error; });
    child.stderr?.on("error", error => { parseError ??= error; });
    lines.on("error", error => { parseError ??= error; lines.close(); });
    lines.on("line", line => {
      try {
        let message;
        try { message = JSON.parse(line); } catch { output.write(`${line}\n`); return; }
        if (!message || typeof message !== "object" || !message.reason) { output.write(`${line}\n`); return; }
        onMessage(message);
        if (env.RBIT_WORKSPACE_JSON === "1") output.write(`${line}\n`);
        if (message.reason === "compiler-artifact" && message.executable) onArtifact(message);
        else if (env.RBIT_WORKSPACE_JSON !== "1" && message.reason === "compiler-message" && message.message.rendered) diagnostics.write(message.message.rendered);
      } catch (error) { parseError ??= error; }
    });
    child.on("error", error => { spawnError = error; });
    child.on("close", (code, signal) => {
      void (async () => {
        // close follows native stdio closure. Also drain the readline decoder before
        // resolving, so the final JSON line/callback failure cannot race publication.
        await drained;
        const receipt = verifyCompletion ? await verifyCompletion(child) : undefined;
        if (!receipt && (signal || (!spawnError && code !== 0 && code !== 101) || (spawnError && child.pid))) {
          throw new UncertainProcessTreeError(`Cargo terminated abnormally (${signal ?? code}); compiler descendants may still be writing`);
        }
        try { onExit(code, signal); }
        catch (error) {
          if (!(parseError instanceof UncertainProcessTreeError)) parseError = error instanceof UncertainProcessTreeError ? error : parseError ?? error;
        }
        if (spawnError || parseError) throw parseError instanceof UncertainProcessTreeError ? parseError : spawnError ?? parseError;
        if (code !== 0) throw new Error(`Cargo ${signal ?? `exited ${code}`}`);
        return receipt;
      })().then(done, reject);
    });
  });
}

/** Only helpers acquire this lease. Direct Cargo/IDE builds must not use this lane. */
export async function cargo(ctx, command, args = [], { execute = cargoProcess, env = process.env, key, reportMaintenance = reportCacheOverage, readRetentionInput,
  provenance = sourceProof, proofInputs } = {}) {
  if (!["build", "check", "clippy", "test", "nextest", "generate-lockfile", "fetch"].includes(command)) throw new Error("Unsupported Cargo command");
  validateCargoArgs(args);
  cargoTreeMode(env);
  if (command === "fetch" && env.RBIT_BUILD_MANAGED_RETENTION && env.RBIT_BUILD_MANAGED_RETENTION !== "off") {
    throw new Error("fetch does not support managed retention observation or candidate modes");
  }
  const retentionMode = managedRetentionMode(env);
  const requiredHeadroom = retentionMode === "measure-candidate" ? candidateHeadroom(env) : null;
  if (command === "nextest" && !["run", "list"].includes(args[0])) throw new Error("Use nextest run or list");
  const maxCacheBytes = cacheBudget(env);
  const buildEnv = { ...env, LIBSQLITE3_FLAGS: [env.LIBSQLITE3_FLAGS ?? "", "-DSQLITE_ENABLE_QPSG"].join(" ").trim() };
  const namespace = key ?? compatibility(buildEnv, execFileSync("rustc", ["-vV"], {
    cwd: ctx.checkout, env: buildEnv, encoding: "utf8", windowsHide: true,
  }));
  if (!/^[a-f0-9]{20}$/.test(namespace)) throw new Error("Invalid compatibility namespace");
  const cache = await managedDirectory(ctx, ctx.cache);
  // A repository-wide lease also serializes prune and caps active developer build lanes
  // at one. Namespaces separate compiler identities, not worktrees or branches.
  return leased(join(cache, ".lease"), async (lease) => {
    const candidate = retentionMode === "measure-candidate"
      ? await createCandidate(ctx, namespace, requiredHeadroom, lease) : null;
    const lane = candidate?.path ?? await managedDirectory(ctx, join(cache, namespace, "shared"));
    let completion, proof, executionAttempted = false;
    try {
      buildEnv.CARGO_TARGET_DIR = join(lane, "target");
      buildEnv.CARGO_BUILD_TARGET_DIR = buildEnv.CARGO_TARGET_DIR;
      buildEnv.CARGO_BUILD_BUILD_DIR = join(lane, "intermediates");
      const observation = retentionMode === "observe"
        ? await beginRetention(ctx, { namespace, command, args, env: buildEnv }, lease, { read: readRetentionInput }) : null;
      if (candidate) await startCandidate(candidate.handle, command, args, buildEnv, lease, { read: readRetentionInput });
      // Noncompiler writers bootstrap dependency/lock state without offline metadata
      // admission. They never authorize any cached compilation unit.
      if (!["generate-lockfile", "fetch"].includes(command)) proof = await provenance.begin(ctx, { lane, namespace, command, args, env: buildEnv }, lease, proofInputs);
      const artifacts = [], copies = observation || candidate ? [] : null;
      const json = ["build", "check", "clippy", "test"].includes(command);
      const format = buildEnv.RBIT_WORKSPACE_JSON === "1" ? "json" : "json-render-diagnostics";
      if (candidate) candidateExecutionStarting(candidate.handle, lease);
      executionAttempted = true;
      completion = await execute(ctx, [command, ...(json ? [`--message-format=${format}`] : []), ...args], buildEnv, (item) => artifacts.push(item),
        { lease, invocation: proof?.invocation, onMessage: proof?.observe });
      if (candidate) candidateExecuted(candidate.handle, lease);
      const result = { executables: [], environment: buildEnv, namespace };
      if (proof) {
        result.sourceProof = await provenance.finish(proof.handle, completion, lease);
        if (!result.sourceProof.trusted) process.stderr.write("Shared local-unit reuse remains untrusted: owned native completion is required.\n");
      }
      if (verifiedCargoCompletion(completion, lease)) result.completion = completion;
      if ((observation || candidate) && !result.completion) throw new UncertainProcessTreeError("Managed execution requires verified native Cargo completion");
      if (command === "build") {
        const output = await copyExecutables(ctx, lane, namespace, artifacts);
        result.executables = output.executables;
        copies?.push(...output.copies);
      }
      await writeFile(join(cache, namespace, "last-used"), String(Date.now()));
      // Keep hot IDE/check loops cheap: scan after builds and completed test runs, never
      // during compilation or before exact executables have been copied out of the lane.
      if (!candidate && (["build", "test"].includes(command) || (command === "nextest" && args[0] === "run"))) {
        result.cache = await enforceHighWater(ctx, maxCacheBytes, namespace, lease);
        await reportMaintenance(result.cache);
      }
      if (observation) result.retention = await finishRetention(observation, completion, copies, maxCacheBytes, lease);
      if (candidate) result.retention = await completeCandidate(candidate.handle, completion, copies, lease);
      return result;
    } catch (error) {
      if (proof) {
        try { await provenance.fail(proof.handle, lease); }
        catch { /* Failed provenance publication cannot replace process uncertainty. */ }
      }
      const failure = candidate && executionAttempted && !verifiedCargoCompletion(completion, lease)
        && !failedCargoCompletion(error, lease) && !(error instanceof UncertainProcessTreeError)
        ? new UncertainProcessTreeError("Candidate command completion is unverified", { cause: error }) : error;
      if (candidate) {
        try { await quarantineCandidate(candidate.handle, failure, completion, lease); }
        catch (diagnostic) {
          // Diagnostic attachment is best-effort even for frozen caller errors;
          // replacing an uncertainty error could incorrectly release the lease.
          try { if (failure && typeof failure === "object") failure.candidateRecordError = diagnostic; } catch {}
        }
      }
      throw failure;
    }
  });
}
