// Strict compiler ownership, default on Windows. No PID/normal-exit proof fallback.
import { randomUUID } from "node:crypto";
import { execFileSync } from "node:child_process";
import { lstat, realpath, readdir, rm } from "node:fs/promises";
import { isAbsolute, join } from "node:path";
import { guarded, managedDirectory } from "./paths.mjs";
import { requireLease } from "./storage.mjs";
import { UncertainProcessTreeError } from "./lease.mjs";
import { launchTree, verifyTreeStopped } from "./process-tree.mjs";

const receipts = new WeakMap();
const failedReceipts = new WeakMap();
export function cargoTreeMode(env, platform = process.platform) {
  const mode = env.RBIT_WORKSPACE_CARGO_TREE;
  if (mode === undefined) return platform === "win32" ? "windows-job" : "off";
  if (mode === "off") return "off";
  if (mode !== "windows-job") throw new Error("RBIT_WORKSPACE_CARGO_TREE must be off or windows-job");
  if (platform !== "win32") throw new Error("Owned Cargo windows-job mode is unsupported on this platform");
  return mode;
}
/** A serialized object or a receipt from another lease is never completion authority. */
export function verifiedCargoCompletion(receipt, lease, invocation) {
  return receipt !== null && typeof receipt === "object" && receipts.has(receipt)
    && typeof lease?.token === "string" && lease.token.length > 0 && receipts.get(receipt).token === lease.token
    && (invocation === undefined || receipts.get(receipt).invocation === invocation);
}
/** Failure status is separate from positive native tree-completion provenance. */
export function failedCargoCompletion(error, lease) {
  const receipt = error && typeof error === "object" ? failedReceipts.get(error) : undefined;
  return verifiedCargoCompletion(receipt, lease) ? receipt : undefined;
}
async function cargoExecutable(ctx, env) {
  // Resolve the same operator PATH tool, then pass an absolute module name to CreateProcess.
  const systemRoot = process.env.SystemRoot;
  if (!systemRoot || !isAbsolute(systemRoot)) throw new Error("Windows system executable root is unavailable");
  const candidates = execFileSync(join(systemRoot, "System32", "where.exe"), ["cargo.exe"], {
    cwd: ctx.checkout, env, encoding: "utf8", windowsHide: true, maxBuffer: 65536,
    stdio: ["ignore", "pipe", "pipe"],
  }).trim().split(/\r?\n/);
  const executable = candidates[0];
  if (!isAbsolute(executable) || !/\.exe$/i.test(executable)) throw new Error("Cargo executable resolution failed");
  const canonical = await realpath(executable);
  if (!(await lstat(canonical)).isFile()) throw new Error("Cargo executable is not a regular file");
  // Preserve the proxy invocation name (e.g. a cargo.exe symlink to rustup.exe).
  return executable;
}
export async function ownedCargo(ctx, args, env, artifact, options, consume) {
  const { lease, invocation, native = {}, ...streamOptions } = options;
  if (!native.launch && process.platform !== "win32") throw new Error("Owned Cargo requires Windows job support");
  await requireLease(ctx, lease);
  const control = await managedDirectory(ctx, join(ctx.state, "cargo", randomUUID()));
  const token = randomUUID();
  const stopFile = join(control, "stop");
  const proofFile = join(control, "stopped");
  const launch = native.launch ?? launchTree;
  const verify = native.verify ?? verifyTreeStopped;
  let uncertain = false, failure, child, proved = false, receipt;
  try {
    const executable = await (native.resolve ?? cargoExecutable)(ctx, env);
    const result = await consume(ctx, args, env, artifact, {
      ...streamOptions,
      spawnProcess: (_name, actual, spawnOptions) => {
        try { child = launch(executable, actual, { ...spawnOptions, stopFile, proofFile, token }); return child; }
        catch (cause) { throw new UncertainProcessTreeError("Owned Cargo launch outcome is unverified", { cause }); }
      },
      verifyCompletion: async child => {
        try {
          await requireLease(ctx, lease);
          if (!await verify(child)) throw new Error("Native completion proof is absent");
          proved = true;
          receipt = Object.freeze({ method: "windows-job", treeStopped: true });
          receipts.set(receipt, { token: lease.token, invocation });
          return receipt;
        } catch (cause) {
          throw new UncertainProcessTreeError("Owned Cargo descendants are unverified; control evidence retained", { cause });
        }
      },
    });
    return result;
  } catch (error) {
    failure = child && !proved && !(error instanceof UncertainProcessTreeError)
      ? new UncertainProcessTreeError("Owned Cargo completion was interrupted before proof", { cause: error }) : error;
    if (receipt && failure && typeof failure === "object") failedReceipts.set(failure, receipt);
    uncertain = failure instanceof UncertainProcessTreeError;
    if (uncertain) { try { failure.control = control; } catch {} }
    throw failure;
  } finally {
    if (!uncertain) {
      try {
        await requireLease(ctx, lease);
        await guarded(ctx.root, control);
        // Fixed per-run control contains only our three files; reject links/unknown files.
        for (const name of await readdir(control)) {
          if (!["stop", "stopped", "stop.args.json"].includes(name)) throw new Error("Unknown owned Cargo control file");
          const path = await guarded(ctx.root, join(control, name));
          if (!(await lstat(path)).isFile()) throw new Error("Invalid owned Cargo control file");
        }
        await rm(control, { recursive: true });
      } catch (cleanupError) {
        if (failure) { try { failure.cleanupError = cleanupError; } catch {} }
        else {
          if (receipt && cleanupError && typeof cleanupError === "object") failedReceipts.set(cleanupError, receipt);
          throw cleanupError;
        }
      }
    }
  }
}
