#!/usr/bin/env node
// Release rehearsal: proves an installation of the previous release upgrades to a
// candidate build and rolls back safely. See docs/upgrade-rehearsal.md.
//
//   node scripts/release-rehearsal.mjs --candidate target/debug/rbit            # fast mode
//   node scripts/release-rehearsal.mjs --candidate NEW --baseline OLD \
//        [--baseline-runner OLD-RUNNER]                                        # full mode
//
// Options: --work DIR (default: a new temporary directory), --keep (leave it afterwards),
// --target N (the candidate's newest migration; default: the newest file in the tree).
import { mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import { Report } from "./release-rehearsal/report.mjs";
import { fast } from "./release-rehearsal/fast.mjs";
import { full } from "./release-rehearsal/full.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

export function parseArgs(argv) {
  const options = { keep: false };
  const valued = { "--candidate": "candidate", "--baseline": "baseline", "--baseline-runner": "runner", "--work": "work", "--target": "target" };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--keep") options.keep = true;
    else if (valued[arg] && index + 1 < argv.length) options[valued[arg]] = argv[++index];
    else throw new Error(`unknown or incomplete argument: ${arg}`);
  }
  if (!options.candidate) throw new Error("--candidate PATH is required");
  if (options.runner && !options.baseline) throw new Error("--baseline-runner needs --baseline");
  options.mode = options.baseline ? "full" : "fast";
  return options;
}

export async function newestMigration(dir = join(root, "crates/rbit-storage/migrations")) {
  return Math.max(...(await readdir(dir)).filter((name) => /^\d{4}_/.test(name)).map((name) => Number(name.slice(0, 4))));
}

const version = (binary) => {
  try { return execFileSync(binary, ["--version"], { encoding: "utf8" }).trim(); } catch (error) { return `unavailable: ${error.message}`; }
};

export async function rehearse(options) {
  const work = options.work ? resolve(options.work) : await mkdtemp(join(tmpdir(), "rbit-rehearsal-"));
  const candidate = resolve(options.candidate);
  const target = options.target ? Number(options.target) : await newestMigration();
  const meta = { mode: options.mode, candidate: version(candidate), target, platform: `${process.platform}-${process.arch}`, git: execFileSync("git", ["--version"], { encoding: "utf8" }).trim(), work };
  if (options.baseline) meta.baseline = version(resolve(options.baseline));
  const report = new Report(meta);
  try {
    if (options.mode === "full") {
      await full(report, { work, candidate, target, baseline: resolve(options.baseline), runner: options.runner && resolve(options.runner) });
    } else {
      await fast(report, { root, work, candidate, target });
    }
  } finally {
    await report.write(join(work, "report.json"), join(work, "report.md"));
    process.stdout.write(`\n${report.markdown()}\nReport: ${join(work, "report.md")}\n`);
    if (!options.keep && report.ok) await rm(work, { recursive: true, force: true, maxRetries: 3 });
  }
  return report;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const report = await rehearse(parseArgs(process.argv.slice(2)));
    process.exitCode = report.ok ? 0 : 1;
  } catch (error) {
    console.error(error.message);
    process.exitCode = 2;
  }
}
