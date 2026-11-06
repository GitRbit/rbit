// Explicit development experiment; importing modules never starts a compiler.
import { open, rm } from "node:fs/promises";
import { resolve, join, basename } from "node:path";
import { randomUUID } from "node:crypto";
import { cpus, totalmem, release } from "node:os";
import { context, managedDirectory, guarded } from "../workspace/paths.mjs";
import { compatibility } from "../workspace/build.mjs";
import { options, measurementEnv } from "../render-compile/options.mjs";
import { sourceIdentity, flagIdentity, toolIdentity, cargoConfigIdentity, sha } from "../render-compile/identity.mjs";
import { measuredJob } from "../render-compile/run.mjs";
import { sequence } from "../render-compile/sequence.mjs";
import { sampleCounts } from "../render-compile/summary.mjs";

async function main() {
  const config = options(process.argv.slice(2));
  const ctx = await context();
  const env = measurementEnv(process.env);
  const output = await open(resolve(config.output), "wx", 0o600);
  const write = async (record) => { await output.write(`${JSON.stringify(record)}\n`); await output.sync(); };
  let control, complete = false, retainControl = false;
  const summary = sampleCounts();
  try {
    control = await managedDirectory(ctx, join(ctx.state, "render-compile", randomUUID()));
    const baseline = await sourceIdentity(ctx);
    const { tools, packages } = toolIdentity(ctx, env);
    config.compatibilityKey = compatibility({ ...env, LIBSQLITE3_FLAGS: [env.LIBSQLITE3_FLAGS ?? "", "-DSQLITE_ENABLE_QPSG"].join(" ").trim() }, tools.rustc);
    await write({ kind: "identity", schema: 1, source: baseline, tools, flags: flagIdentity(env), cargoConfigs: await cargoConfigIdentity(ctx, env),
      config: { ...config, output: undefined }, runtime: process.version,
      machine: { platform: process.platform, arch: process.arch, release: release(), logicalCpus: cpus().length,
        cpuModel: cpus()[0]?.model, totalMemoryBytes: totalmem() },
      policy: "serial; package-only cold work after dependency warmup; memory is sampled, not a lifetime peak" });
    await sequence(config, async (job, phase) => {
      await measuredJob(ctx, config, job, baseline, control, async (record) => {
        await write(record);
        summary.observe(record);
      }, env, packages, phase);
    });
    complete = true;
  } catch (error) {
    retainControl = error.retainControl === true;
    await write({ kind: "failure", category: error.name, diagnosticSha256: sha(String(error.message)), observer: error.observerReport ?? null, retainControl });
    process.exitCode = 1;
  } finally {
    await write({ kind: "summary", complete, ...summary.counts, controlRetained: retainControl, controlRunId: retainControl ? basename(control) : null, aggregatesComputed: false });
    await output.close();
    if (control && !retainControl) { await guarded(ctx.root, control); await rm(control, { recursive: true }); }
    if (summary.counts.compileInvalid || (config.memory !== "none" && summary.counts.memoryInvalid)) process.exitCode = 1;
  }
}

main().catch(() => { process.stderr.write("Compiler measurement setup failed; check explicit arguments and output location.\n"); process.exitCode = 1; });
