// Guard the existing CI job's explicit standalone compile/test invocation.
// This checks our narrow inline-run workflow convention, not arbitrary YAML.
import { readFileSync } from "node:fs";
import { join } from "node:path";

export const INVENTORY_TEST = "node scripts/workspace.mjs test --package rbit-credential-inventory --no-default-features --locked";

export function inventoryCompileViolations(workflow) {
  const lines = workflow.replaceAll("\r\n", "\n").split("\n");
  const start = lines.findIndex(line => line === "  rewrite-contracts:");
  if (start < 0) return ["credential inventory: missing standalone CI job evidence"];
  let end = lines.findIndex((line, index) => index > start && /^  [a-zA-Z][\w-]*:/.test(line));
  if (end < 0) end = lines.length;
  const job = lines.slice(start + 1, end);
  const violations = [];
  if (!job.some(line => /^        os: \[ubuntu-latest, windows-latest\]$/.test(line))
    || !job.some(line => line === "    runs-on: ${{ matrix.os }}")
    || job.some(line => /^    if:/.test(line))) {
    violations.push("credential inventory: isolated compile must run in the existing Linux and Windows matrix");
  }
  const starts = job.flatMap((line, index) => /^      - /.test(line) ? [index] : []);
  const candidates = starts.map((position, index) => job.slice(position, starts[index + 1] ?? job.length))
    .filter(step => step[0] === `      - run: ${INVENTORY_TEST}`);
  if (candidates.length !== 1 || candidates.some(step => step.slice(1).some(line => line.trim() && !line.trimStart().startsWith("#")))) {
    violations.push("credential inventory: requires one unconditional standalone locked package test, without application/workspace features or overrides");
  }
  return violations;
}

export function checkInventoryCompileIsolation(root) {
  return inventoryCompileViolations(readFileSync(join(root, ".github/workflows/ci.yml"), "utf8"));
}
