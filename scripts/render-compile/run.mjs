// Only the selected probe's library is compiled. Package cleaning and measurements
// execute inside cargo()'s existing lane lease, never through an unrestricted shell.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { performance } from "node:perf_hooks";
import { cargo, cargoProcess } from "../workspace/build.mjs";
import { artifactIdentity, flagIdentity, sampleValidity, sourceIdentity, sha } from "./identity.mjs";
import { cargoArgs, cleanArgs } from "./options.mjs";
import { sampler } from "./sampler.mjs";

function digestSink() {
  const hash = createHash("sha256"); let bytes = 0;
  return { write(chunk) { const value = Buffer.from(chunk); bytes += value.length; hash.update(value); }, result() { return { bytes, sha256: hash.digest("hex") }; } };
}

export async function measuredJob(ctx, config, job, baseline, control, write, env, packages, phase = "sample") {
  let recorded = false;
  try {
    return await cargo(ctx, config.command, cargoArgs(job, config), { env, key: config.compatibilityKey, execute: async (_ctx, args, effectiveEnv, onArtifact) => {
      const cleanOutput = digestSink(); let clean = null;
      if (phase === "sample") {
        let cleanExit;
        try {
          await cargoProcess(_ctx, cleanArgs(config), effectiveEnv, () => {}, {
            output: cleanOutput, diagnostics: cleanOutput, captureStderr: true,
            spawnProcess: (command, argv, options) => {
              const child = spawn(command, argv, options); child.on("close", (code, signal) => { cleanExit = { code, signal }; }); return child;
            },
          });
        } catch (error) {
          await write({ kind: "sample", phase: "package-clean", job, exit: cleanExit ?? null, valid: false, output: cleanOutput.result() });
          recorded = true; throw error;
        }
        clean = { exit: cleanExit, output: cleanOutput.result(), argv: cleanArgs(config) };
      }
      const before = await sourceIdentity(ctx);
      const observer = await sampler(control, phase === "sample" ? config : { ...config, memory: "none" });
      const artifacts = [], output = digestSink();
      let exit, failure;
      const started = performance.now();
      try {
        await cargoProcess(_ctx, args, effectiveEnv, onArtifact, {
          output, diagnostics: output, captureStderr: true,
          onMessage: (message) => { const artifact = artifactIdentity(message, packages); if (artifact) artifacts.push(artifact); },
          spawnProcess: (command, argv, options) => {
            const beforeMs = Date.now(); const child = spawn(command, argv, options);
            observer.attach(child, beforeMs, Date.now());
            child.on("close", (code, signal) => { exit = { code, signal }; }); return child;
          },
        });
      } catch (error) { failure = error; }
      const wallMs = performance.now() - started;
      const memory = await observer.finish();
      const after = await sourceIdentity(ctx);
      const validity = phase === "sample" ? sampleValidity(artifacts, job, exit, before, after, memory)
        : { compileComparable: false, memoryComparable: false, reasons: ["warmup-excluded"], warmupSucceeded: exit?.code === 0 };
      if (before.fingerprint !== baseline.fingerprint) { validity.compileComparable = false; validity.memoryComparable = false; validity.reasons.push("baseline-source-mismatch"); }
      await write({ kind: phase, job, command: config.command, profile: config.profile, jobs: config.jobs,
        argv: args, wallMs, clean, exit: exit ?? null, artifacts, memory, validity, output: output.result(), sourceFingerprint: before.fingerprint, effectiveFlags: flagIdentity(effectiveEnv),
        outputNamespace: effectiveEnv.CARGO_TARGET_DIR.split(/[\\/]/).at(-3), outputDirectoriesSha256: sha(JSON.stringify([effectiveEnv.CARGO_TARGET_DIR, effectiveEnv.CARGO_BUILD_BUILD_DIR])) });
      recorded = true;
      if (failure) throw failure;
      if (before.fingerprint !== baseline.fingerprint || after.fingerprint !== baseline.fingerprint) throw new Error("Measurement source changed");
    } });
  } catch (error) {
    if (!recorded) await write({ kind: phase, job, valid: false, reason: "helper-or-preparation-failed", errorType: error.name });
    throw error;
  }
}
