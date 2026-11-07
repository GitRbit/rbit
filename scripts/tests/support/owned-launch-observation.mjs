// Test-only observation: never changes launcher ownership/proof or accepts private diagnostics.
import { stopTree, verifyTreeStopped } from "../../workspace/process-tree.mjs";
import { StringDecoder } from "node:string_decoder";

function failure(code) {
  return Object.assign(new Error(code), { code });
}

export function observeOwnedLaunch(child, {
  stop = stopTree, verify = verifyTreeStopped, progress = async () => ({}),
  schedule = setTimeout, cancel = clearTimeout, cleanupMs = 15_000, maxOutputBytes = 65_536,
} = {}) {
  let spawned = false, exited = false, closed = false, errored = false, streamError = false, forcedStopFailed = false;
  let exitCode = null, signalSeen = false, stdoutEnded = false, stderrEnded = false;
  let stdoutBytes = 0, stderrBytes = 0, output = "", cleanup;
  let cleanupPhase = "not-started", cleanupFailurePhase = "none", resolveClose, resolveError;
  const decoder = new StringDecoder("utf8");
  const close = new Promise((resolve) => { resolveClose = resolve; });
  const spawnFailure = new Promise((resolve) => { resolveError = resolve; });
  child.once("spawn", () => { spawned = true; });
  child.once("error", () => { errored = true; resolveError(); });
  child.once("exit", (code, signal) => {
    exited = true; exitCode = Number.isInteger(code) ? code : null; signalSeen = Boolean(signal);
  });
  child.once("close", (code, signal) => {
    closed = true; exitCode = Number.isInteger(code) ? code : null; signalSeen ||= Boolean(signal);
    resolveClose({ code, signal });
  });
  child.stdout?.on("data", (chunk) => {
    stdoutBytes += Buffer.byteLength(chunk);
    if (stdoutBytes <= maxOutputBytes) output += decoder.write(Buffer.from(chunk));
  });
  child.stderr?.on("data", (chunk) => { stderrBytes += Buffer.byteLength(chunk); });
  for (const stream of [child.stdout, child.stderr]) {
    stream?.once("error", () => { streamError = true; resolveError(); });
  }
  child.stdout?.once("end", () => { stdoutEnded = true; output += decoder.end(); });
  child.stderr?.once("end", () => { stderrEnded = true; });

  async function bounded(promise, milliseconds, code) {
    let timer;
    try {
      return await Promise.race([promise, new Promise((_, reject) => {
        timer = schedule(() => reject(failure(code)), milliseconds);
      })]);
    } finally { cancel(timer); }
  }

  return {
    async wait(signal) {
      let abort;
      try {
        const result = await Promise.race([close, spawnFailure.then(() => {
          throw failure(errored ? "OWNED_LAUNCH_SPAWN_ERROR" : "OWNED_LAUNCH_IO_ERROR");
        }), new Promise((_, reject) => {
          abort = () => reject(failure("OWNED_LAUNCH_DEADLINE"));
          if (signal?.aborted) abort();
          else signal?.addEventListener("abort", abort, { once: true });
        })]);
        if (errored) throw failure("OWNED_LAUNCH_SPAWN_ERROR");
        if (streamError) throw failure("OWNED_LAUNCH_IO_ERROR");
        if (stdoutBytes > maxOutputBytes) throw failure("OWNED_LAUNCH_OUTPUT_LIMIT");
        return result;
      } finally { signal?.removeEventListener("abort", abort); }
    },
    output() { return output; },
    async diagnostics() {
      let state;
      try { state = await progress(); } catch { state = { unreadable: true }; }
      // Allowlist scalars only; never spread argv, stderr, env, paths or raw exceptions.
      return {
        category: "OWNED_LAUNCH_OBSERVATION", spawned, exited, closed, errored, streamError, forcedStopFailed, cleanupPhase, cleanupFailurePhase,
        exitCode, signalSeen, stdoutBytes, stderrBytes, stdoutEnded, stderrEnded,
        argumentsPending: state.argumentsPending === true,
        targetStarted: state.targetStarted === true,
        targetOutputReturned: state.targetOutputReturned === true,
        proof: ["verified", "missing", "unreadable"].includes(state.proof) ? state.proof : "unchecked",
        observationUnreadable: state.unreadable === true,
      };
    },
    cleanup() {
      cleanup ??= (async () => {
        const phase = (value) => { if (cleanupPhase !== "unproved") cleanupPhase = value; };
        try {
          await bounded((async () => {
            // One total cleanup budget. Root exit alone is not pipe closure.
            phase("stopping");
            if (!closed) await stop(child, { awaitMarker: false });
            phase("draining");
            await close;
            phase("proving");
            if (!await verify(child)) throw failure("OWNED_LAUNCH_UNPROVED");
            phase("verified");
          })(), cleanupMs, "OWNED_LAUNCH_CLEANUP_TIMEOUT");
        } catch {
          cleanupFailurePhase = cleanupPhase;
          cleanupPhase = "unproved";
          // Only the spawned owned launcher is killed, never a global PID/tree scan.
          // KILL_ON_JOB_CLOSE still applies on Windows; abrupt death is NOT a proof.
          if (!exited && child.exitCode === null && child.signalCode === null) {
            try { forcedStopFailed = !child.kill("SIGKILL"); } catch { forcedStopFailed = true; }
          }
          child.stdout?.destroy(); child.stderr?.destroy();
          throw failure("OWNED_LAUNCH_CLEANUP_UNPROVED");
        }
      })();
      return cleanup;
    },
  };
}

/** Preserve the original assertion/deadline when bounded cleanup also fails. */
export async function preservingLaunchFailure(operation, cleanup) {
  let primary;
  try { return await operation(); }
  catch (error) { primary = error; throw error; }
  finally {
    try { await cleanup(); }
    catch (error) {
      if (!primary) throw error;
      primary.cleanupUnproved = true;
    }
  }
}
