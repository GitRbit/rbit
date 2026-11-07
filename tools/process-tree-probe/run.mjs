#!/usr/bin/env node
// Build one fixed development tool and run its exact copied artifact, never the server.
import { writeFile } from "node:fs/promises";
import { cargo } from "../../scripts/workspace/build.mjs";
import { context } from "../../scripts/workspace/paths.mjs";
import { runBinary } from "../../scripts/workspace/runtime.mjs";

const [report, ...extra] = process.argv.slice(2);
if (!report || extra.length) throw new Error("Usage: node tools/process-tree-probe/run.mjs REPORT.json");
const command = process.platform === "linux" ? "verify-unix" : process.platform === "win32" ? "verify" : null;
if (!command) throw new Error("Native process proof requires Windows or Linux");
const ctx = await context();
const build = await cargo(ctx, "build", ["--package", "rbit-process-tree-probe", "--locked"]);
const executable = build.executables.find((entry) => entry.name === "rbit-process-tree-probe");
if (!executable) throw new Error("Cargo did not report the exact native proof executable");
const child = await runBinary(ctx, executable.path, [command, report]);
const code = child.exitCode !== null || child.signalCode !== null
  ? child.exitCode ?? 1 : await new Promise((done) => child.once("exit", done));
await child.runtimeCompletion;
await writeFile(`${report}.build.json`, JSON.stringify({ namespace: build.namespace, exit_code: code, platform: process.platform }));
if (code !== 0) throw new Error(`Native proof exited ${code}; completion evidence is absent or failed`);
