import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { checkLedger } from "../rewrite/check.mjs";

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), "rbit-planned-feature-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, "crates/rbit-example/src"), { recursive: true });
  writeFileSync(join(root, "crates/rbit-example/src/lib.rs"), "pub struct Example;\n");
  for (const name of ["plan.md", "w0-results.md", "w1-results.md"]) writeFileSync(join(root, name), "Fixture evidence only.\n");
  const feature = { id: "example.documentation", baselinePresence: "absent", status: "planned", implementation: "not-started",
    target: "rbit-example", plan: "plan.md", acceptancePlan: "plan.md", delivery: ["W0 contracts", "W1 runtime"], acceptanceEvidence: [] };
  const ledger = { features: [], plannedAdditions: [feature] };
  return { root, feature, ledger, check: () => checkLedger(root, ledger) };
}

test("planned and partially prepared work stays distinct from implemented scope", t => {
  const { feature, check } = fixture(t);
  assert.deepEqual(check(), []);
  feature.implementation = "w0-contracts-prepared";
  assert.deepEqual(check(), []);
  feature.status = "in-progress";
  feature.acceptanceEvidence = ["w0-results.md"];
  assert.deepEqual(check(), []);
  feature.implementation = "complete";
  assert.match(check().join("\n"), /complete implementation needs implemented status/);
});

test("a plan alone cannot complete a newly requested product feature", t => {
  const { feature, check } = fixture(t);
  feature.status = "implemented";
  feature.implementation = "complete";
  feature.replacement = ["plan.md"];
  assert.match(check().join("\n"), /target crate source replacements/);
  assert.match(check().join("\n"), /needs acceptance results/);
  feature.replacement = ["crates/rbit-example/src/lib.rs"];
  feature.acceptanceEvidence = ["w0-results.md"];
  feature.phaseEvidence = { "W0 contracts": "w0-results.md" };
  assert.match(check().join("\n"), /missing acceptance result for W1 runtime/);
  feature.acceptanceEvidence.push("w1-results.md");
  feature.phaseEvidence["W1 runtime"] = "w1-results.md";
  assert.deepEqual(check(), []);
});

test("new scope cannot hide duplicates or claim baseline presence", t => {
  const { root, feature, ledger, check } = fixture(t);
  ledger.features = [{ id: feature.id, status: "baseline-retained", source: "plan.md", test: "plan.md" }];
  assert.match(check().join("\n"), /duplicate/);
  ledger.features = [];
  feature.baselinePresence = "present";
  assert.match(check().join("\n"), /must remain absent from baseline/);
  assert.match(checkLedger(root, { features: [], plannedAdditions: {} }).join("\n"), /expected an array/);
});

test("unknown states, missing phases and unsafe evidence fail the ledger check", t => {
  const { feature, check } = fixture(t);
  feature.status = "ported";
  assert.match(check().join("\n"), /unknown planned status/);
  feature.status = "planned";
  for (const path of ["../plan.md", "C:\\outside\\report.md", "/outside/report.md", "missing.md"]) {
    feature.acceptanceEvidence = [path];
    assert.match(check().join("\n"), /evidence must name existing files/);
  }
  feature.acceptanceEvidence = [];
  feature.delivery = ["W0 contracts", "W0 contracts"];
  assert.match(check().join("\n"), /phases must be nonempty and unique/);
});
