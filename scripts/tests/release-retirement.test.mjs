import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { RELEASE_WORKFLOW, checkReleaseWorkflow, jobs } from "../rewrite/release-retirement.mjs";

const source = readFileSync(new URL(`../../${RELEASE_WORKFLOW}`, import.meta.url), "utf8").replaceAll("\r\n", "\n");

/** Applies one textual change, which must exist, and checks the result. */
function broken(from, to, last = false) {
  const at = last ? source.lastIndexOf(from) : source.indexOf(from);
  assert.ok(at >= 0, `fixture text missing: ${from}`);
  return checkReleaseWorkflow(source.slice(0, at) + to + source.slice(at + from.length)).join("\n");
}

test("the current release workflow keeps every delivery guarantee", () => {
  assert.deepEqual(checkReleaseWorkflow(source), []);
  assert.deepEqual([...jobs(source).keys()], ["prepare", "assets", "binaries", "release", "image", "runner-image"]);
});

test("losing a platform, the paired runner, checksums or provenance fails", () => {
  const cases = [
    ["          - target: x86_64-apple-darwin\n", "", /exactly x86_64-unknown-linux-gnu/u],
    ["--package rbit-server --package rbit-runner --target", "--package rbit-server --target", /builds rbit and rbit-runner/u],
    ["for exe in rbit rbit-runner; do", "for exe in rbit; do", /pairs rbit with rbit-runner/u],
    ["node scripts/release-rehearsal.mjs --candidate", "echo skipped", /upgrade rehearsal/u],
    ["sha256sum -- * > SHA256SUMS", "true", /SHA256SUMS/u],
    ["            --verify-tag \\\n", "", /verified tag/u],
    ["          provenance: mode=max\n          sbom: true\n", "          provenance: false\n", /publishes with provenance: mode=max/u],
    ["          file: Dockerfile.runner\n", "", /Dockerfile.runner/u],
    ["            type=semver,pattern={{major}}.{{minor}}\n", "", /same tags/u, true],
    ["            latest=${{ needs.prepare.outputs.prerelease == 'false' }}\n", "            latest=true\n", /same tags/u, true],
    ["-f branch=main -f event=push -f status=success", "-f status=success", /successful main CI/u],
  ];
  for (const [from, to, message, last] of cases) assert.match(broken(from, to, last), message, from);
});

test("untrusted triggers, caches, broad tokens and unpinned actions fail", () => {
  const cases = [
    ['    tags: ["v*"]\n', '    tags: ["v*"]\n  workflow_dispatch:\n', /pushed v\* tags/u],
    ["on:\n  push:\n", "on:\n  pull_request:\n  push:\n", /untrusted trigger/u],
    ["      - uses: ./.github/actions/prepare-build\n", "      - uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0\n", /compiler cache/u],
    ["permissions:\n  contents: read\n\n# Never", "permissions:\n  contents: write\n\n# Never", /read-only/u],
    ["    runs-on: ${{ matrix.os }}\n", "    runs-on: ${{ matrix.os }}\n    permissions:\n      contents: write\n", /job binaries may not hold contents: write/u],
    ["    permissions:\n      contents: write\n", "    permissions:\n      contents: write\n      packages: write\n", /job release may not hold/u],
    ["actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1", "actions/checkout@v7", /not pinned/u],
    ["cancel-in-progress: false", "cancel-in-progress: true", /never cancelled/u],
  ];
  for (const [from, to, message] of cases) assert.match(broken(from, to), message, from);
});
