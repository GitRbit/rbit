#!/usr/bin/env node
// The only door an agent under evaluation has to the seeded candidate: one metered GET.
//
//   node scripts/agent-loop/eval-api.mjs --work DIR --case ID PATH
//
// PATH is an API path after `/api/v1` (e.g. `/repos/agent/calc/actions/runs?branch=x`).
// The response body is printed as it is; its size is appended to `<work>/meter/<ID>.jsonl`
// so the grader can charge each case for what it read. The token is read from
// `<work>/token` and never printed. Non-GET methods are refused: cases are read-only.
import { appendFileSync, mkdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

function parse(argv) {
  const options = { work: "", case: "", path: "" };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--work" || argv[i] === "--case") options[argv[i].slice(2)] = argv[++i] ?? "";
    else if (!options.path) options.path = argv[i];
    else throw new Error(`unexpected argument ${argv[i]}`);
  }
  if (!options.work || !/^[\w.-]+$/.test(options.case) || !options.path.startsWith("/")) {
    throw new Error("usage: eval-api.mjs --work DIR --case ID /api/path");
  }
  return options;
}

async function main() {
  const options = parse(process.argv.slice(2));
  const work = resolve(options.work);
  const { base } = JSON.parse(readFileSync(join(work, "cases.json"), "utf8"));
  const token = readFileSync(join(work, "token"), "utf8").trim();
  const url = new URL(`/api/v1${options.path}`, base);
  if (url.origin !== new URL(base).origin) throw new Error("the path must stay on the candidate");
  const response = await fetch(url, { headers: { Authorization: `token ${token}`, Accept: "application/json" }, redirect: "manual", signal: AbortSignal.timeout(30_000) });
  const body = Buffer.from(await response.arrayBuffer());
  mkdirSync(join(work, "meter"), { recursive: true });
  appendFileSync(join(work, "meter", `${options.case}.jsonl`), `${JSON.stringify({ path: options.path, status: response.status, bytes: body.length, at: Date.now() })}\n`);
  process.stdout.write(`HTTP ${response.status}\n`);
  process.stdout.write(body);
  if (body.length && body[body.length - 1] !== 10) process.stdout.write("\n");
}

main().catch((error) => {
  process.stderr.write(`${error.message}\n`);
  process.exitCode = 2;
});
