// The Rewrite contracts job runs on Linux and Windows, but a feature-contract step only
// needs Windows when the crates it covers behave differently there. Steps limited to one
// platform are allowed only for crates without OS-specific code or dependencies; a crate
// with any is tested on both platforms within the job.
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8").replace(/\r\n/g, "\n");
const workflow = read(".github/workflows/ci.yml");
const contracts = workflow.slice(workflow.indexOf("\n  rewrite-contracts:"), workflow.indexOf("\n  lint:"));

/** Each `- run:` step of the job with the platform its `if:` limits it to (or both). */
function steps() {
  const lines = contracts.split("\n");
  const found = [];
  for (const [index, line] of lines.entries()) {
    const run = line.match(/^ {6}- run: (.+)$/)?.[1];
    if (!run) continue;
    const condition = lines[index + 1]?.match(/^ {8}if: (.+)$/)?.[1];
    const platforms = condition === undefined ? ["Linux", "Windows"]
      : condition === "runner.os == 'Linux'" ? ["Linux"]
        : condition === "runner.os == 'Windows'" ? ["Windows"] : null;
    assert.ok(platforms, `unexpected condition on "${run}": ${condition}`);
    found.push({ run, platforms });
  }
  return found;
}

function members() {
  const manifest = read("Cargo.toml");
  const list = manifest.split("\nmembers = [")[1].split("]")[0];
  const byName = new Map();
  for (const path of [...list.matchAll(/"([^"]+)"/g)].map((match) => match[1])) {
    const name = read(`${path}/Cargo.toml`).match(/^\[package\][\s\S]*?^name = "([^"]+)"/m)?.[1];
    if (name) byName.set(name, path);
  }
  return byName;
}

function rustFiles(directory) {
  let entries;
  try { entries = readdirSync(new URL(directory, root)); } catch { return []; }
  return entries.flatMap((entry) => {
    const path = `${directory}/${entry}`;
    return statSync(new URL(path, root)).isDirectory() ? rustFiles(path) : path.endsWith(".rs") ? [path] : [];
  });
}

/** Whether a crate's code or dependencies differ by operating system. */
function osSpecific(path) {
  if (/^\[target\./m.test(read(`${path}/Cargo.toml`))) return true;
  const condition = /\bcfg!?\s*\([^\n]*\b(?:windows|unix|target_os|target_family)\b/;
  return ["src", "tests", "benches", "examples"].flatMap((part) => rustFiles(`${path}/${part}`))
    .some((file) => condition.test(read(file)));
}

test("contract steps limited to one platform cover only crates that behave the same on both", () => {
  const crates = members();
  const coverage = new Map();
  for (const { run, platforms } of steps()) {
    if (platforms.length === 2) continue;
    assert.match(run, /^node scripts\/workspace\.mjs test /, `only workspace tests may be limited to one platform: ${run}`);
    for (const [, name] of run.matchAll(/--package (\S+)/g)) {
      const set = coverage.get(name) ?? new Set();
      for (const platform of platforms) set.add(platform);
      coverage.set(name, set);
    }
  }
  assert.ok(coverage.size > 0, "found the platform-limited steps");
  for (const { run, platforms } of steps()) {
    if (platforms.length !== 2) continue;
    for (const [, name] of run.matchAll(/--package (\S+)/g)) coverage.set(name, new Set(platforms));
  }
  for (const [name, platforms] of coverage) {
    const path = crates.get(name);
    assert.ok(path, `${name} is a workspace member`);
    if (osSpecific(path)) {
      assert.deepEqual([...platforms].sort(), ["Linux", "Windows"], `${name} has OS-specific code or dependencies: test it on both platforms`);
    }
  }
});

test("the checks this job exists to run on Windows are not limited to one platform", () => {
  const both = steps().filter(({ platforms }) => platforms.length === 2).map(({ run }) => run);
  for (const required of [
    "node scripts/workspace.mjs fetch --locked",
    "node scripts/rewrite/check.mjs",
    "node scripts/workspace.mjs test --package rbit-credential-inventory --no-default-features --locked",
    "node scripts/workspace.mjs clippy --package rbit-platform --all-targets --locked -- -D warnings",
    "node scripts/rewrite/check-types.mjs",
  ]) assert.ok(both.includes(required), `runs on both platforms: ${required}`);
  assert.ok(both.some((run) => run.startsWith("node --test ") && run.includes("scripts/tests/platform-boundary.test.mjs")));
});
