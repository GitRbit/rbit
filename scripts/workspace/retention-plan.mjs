// Read-only stdin policy tool. Does not resolve or inspect the repository/cache.
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { normalizeGraph } from "./retention/graph.mjs";
import { planRetention } from "./retention/planner.mjs";
export function retentionPlan(input) {
  const requested = normalizeGraph(input.request);
  const observation = input.observation ?? {};
  const current = observation.current && { ...observation.current, graph: normalizeGraph(observation.current.request) };
  const candidate = observation.candidate && { ...observation.candidate, graph: normalizeGraph(observation.candidate.request) };
  return { graph: { id: requested.id, policyId: requested.policyId, confidence: requested.confidence, reasons: requested.reasons },
    decision: planRetention({ ...observation, current, candidate, requested }) };
}
async function main() {
  if (process.argv.length !== 2) throw new Error("Expected one JSON document on stdin and no arguments");
  let length = 0; const chunks = [];
  for await (const chunk of process.stdin) {
    length += chunk.length;
    if (length > 65536) throw new Error("Policy input exceeds 64 KiB");
    chunks.push(chunk);
  }
  let output;
  try { output = retentionPlan(JSON.parse(Buffer.concat(chunks).toString("utf8"))); }
  catch { throw new Error("Invalid policy input"); }
  process.stdout.write(`${JSON.stringify(output, null, 2)}\n`);
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
}
