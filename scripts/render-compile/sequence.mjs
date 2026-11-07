import { COUNTS, MODES, plan } from "./options.mjs";

/** Await every job: no overlapping compilers or pipelined cleans. */
export async function sequence(config, run) {
  for (const count of COUNTS) for (const mode of MODES) await run({ count, mode }, "warmup");
  for (const job of plan(config.rounds)) await run(job, "sample");
}
