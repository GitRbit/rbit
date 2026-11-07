/** Track new product scope without pretending it existed in the pinned baseline. */
import { lstatSync } from "node:fs";
import { isAbsolute, join } from "node:path";

export function checkPlannedAdditions(root, additions, baselineIds) {
  if (additions === undefined) return [];
  if (!Array.isArray(additions)) return ["Planned additions: expected an array"];
  const violations = [], ids = new Set(baselineIds);
  const evidence = path => {
    if (typeof path !== "string" || !path || isAbsolute(path) || /^[A-Za-z]:/.test(path)
      || path.split(/[\\/]/).includes("..")) return false;
    try { return lstatSync(join(root, path)).isFile(); } catch { return false; }
  };
  for (const feature of additions) {
    if (!feature || typeof feature !== "object") {
      violations.push("Planned additions: expected a feature record"); continue;
    }
    const id = feature.id;
    if (typeof id !== "string" || !/^[a-z][a-z0-9.-]+$/.test(id)) violations.push("Planned additions: invalid feature id");
    if (ids.has(id)) violations.push(`Feature ledger: duplicate ${id}`);
    ids.add(id);
    if (feature.baselinePresence !== "absent") violations.push(`${id}: new feature must remain absent from baseline`);
    if (!["planned", "in-progress", "implemented"].includes(feature.status)) violations.push(`${id}: unknown planned status`);
    if (typeof feature.implementation !== "string" || !feature.implementation.trim()) violations.push(`${id}: missing implementation progress`);
    if (typeof feature.target !== "string" || !/^rbit-[a-z0-9-]+$/.test(feature.target)) violations.push(`${id}: missing target crate`);
    for (const key of ["plan", "acceptancePlan"]) {
      if (!evidence(feature[key])) violations.push(`${id}: missing ${key} file`);
    }
    const delivery = feature.delivery;
    const phases = Array.isArray(delivery) && delivery.length > 0 && delivery.every(phase => typeof phase === "string" && phase.trim())
      && new Set(delivery).size === delivery.length;
    if (!phases) violations.push(`${id}: delivery phases must be nonempty and unique`);
    if (!Array.isArray(feature.acceptanceEvidence) || !feature.acceptanceEvidence.every(evidence)) {
      violations.push(`${id}: acceptance evidence must name existing files`);
    }
    if (feature.status !== "implemented") {
      if (feature.implementation === "complete") violations.push(`${id}: complete implementation needs implemented status and acceptance`);
      continue;
    }
    if (feature.implementation !== "complete") violations.push(`${id}: implemented feature needs complete implementation`);
    if (!Array.isArray(feature.replacement) || !feature.replacement.length || !feature.replacement.every(evidence)
      || !feature.replacement.some(path => path.startsWith(`crates/${feature.target}/`) && path.endsWith(".rs"))) {
      violations.push(`${id}: implemented feature needs target crate source replacements`);
    }
    if (!feature.acceptanceEvidence?.length) violations.push(`${id}: implemented feature needs acceptance results`);
    if (phases) {
      for (const phase of delivery) {
        if (!feature.phaseEvidence || !Object.hasOwn(feature.phaseEvidence, phase) || !evidence(feature.phaseEvidence[phase])) {
          violations.push(`${id}: missing acceptance result for ${phase}`);
        }
      }
    }
  }
  return violations;
}
