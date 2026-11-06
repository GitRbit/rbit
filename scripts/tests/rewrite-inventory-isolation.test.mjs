import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { INVENTORY_TEST, inventoryCompileViolations } from "../rewrite/inventory-isolation.mjs";

function workflow(command = INVENTORY_TEST, after = "") {
  return `jobs:
  rewrite-contracts:
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest]
    runs-on: \${{ matrix.os }}
    steps:
      - run: ${command}
${after}  lint:
    runs-on: ubuntu-latest
    steps:
      - run: cargo clippy --workspace
`;
}

test("the real workflow compiles and tests the tool alone on both existing platforms", async () => {
  const source = await readFile(new URL("../../.github/workflows/ci.yml", import.meta.url), "utf8");
  assert.deepEqual(inventoryCompileViolations(source), []);
  assert.deepEqual(inventoryCompileViolations(workflow()), []);
  assert.deepEqual(inventoryCompileViolations(workflow().replaceAll("\n", "\r\n")), []);
});

test("workspace tests, graph inspection and widened package features cannot stand in for isolated compilation", () => {
  for (const command of [
    "cargo tree --package rbit-credential-inventory --locked",
    "node scripts/workspace.mjs test --workspace --locked",
    `${INVENTORY_TEST} --package rbit-services`,
    `${INVENTORY_TEST} --features rbit-identity/password-work`,
    INVENTORY_TEST.replace(" --no-default-features", ""),
    INVENTORY_TEST.replace(" --locked", ""),
    `echo ${INVENTORY_TEST}`,
  ]) assert.match(inventoryCompileViolations(workflow(command)).join("\n"), /standalone locked package test/);
});

test("commented, skipped, failure-tolerant and platform-restricted steps fail the gate", () => {
  for (const source of [
    workflow().replace("      - run:", "      # - run:"),
    workflow(INVENTORY_TEST, "        if: false\n"),
    workflow(INVENTORY_TEST, "        continue-on-error: true\n"),
    workflow(INVENTORY_TEST, "        working-directory: other\n"),
    workflow(INVENTORY_TEST, "        env:\n          RUSTFLAGS: unexpected\n"),
    workflow().replace("[ubuntu-latest, windows-latest]", "[ubuntu-latest]"),
    workflow().replace("    strategy:", "    if: false\n    strategy:"),
    workflow().replace("  rewrite-contracts:", "  other-job:"),
  ]) assert.ok(inventoryCompileViolations(source).length > 0);
});
