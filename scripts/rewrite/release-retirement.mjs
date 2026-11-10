/** The release workflow's delivery contract; see docs/design/rewrite/delivery-releases-acceptance.md. */
import { readFileSync } from "node:fs";
import { join } from "node:path";

export const RELEASE_WORKFLOW = ".github/workflows/release.yml";
// The five binary platforms every release archives, each with rbit and rbit-runner.
export const TARGETS = Object.freeze([
  "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc",
]);
// Only these jobs publish, and only with the one write scope each needs.
const PUBLISHERS = Object.freeze({ release: "contents: write", image: "packages: write", "runner-image": "packages: write" });

/** Top-level jobs as name -> body, from the workflow's own two-space indentation. */
export function jobs(source) {
  const start = source.search(/^jobs:\n/mu);
  if (start < 0) return new Map();
  const parts = source.slice(start).split(/^ {2}(?=[a-z][\w-]*:\s*$)/mu).slice(1);
  return new Map(parts.map((part) => [part.slice(0, part.indexOf(":")), part]));
}

/** The literal block scalar that follows `key: |` in `body`, without indentation. */
function block(body, key) {
  const match = body.match(new RegExp(`^( +)${key}: \\|\\n((?:\\1  .*\\n)+)`, "mu"));
  return match ? match[2].split("\n").map((line) => line.trim()).filter(Boolean).join("\n") : undefined;
}

/** Takes the release workflow's text; returns violations naming the broken guarantee. */
export function checkReleaseWorkflow(text, path = RELEASE_WORKFLOW) {
  const source = text.replaceAll("\r\n", "\n");
  const violations = [];
  const fail = (message) => violations.push(`${path}: ${message}`);
  const triggers = source.match(/^on:\n((?: .*\n|\n)*)/mu)?.[1].trim();
  if (triggers !== 'push:\n    tags: ["v*"]') fail("releases run only from pushed v* tags");
  if (/\bpull_request|\bworkflow_run\b|\bworkflow_dispatch\b|permissions: write-all|Swatinem\/rust-cache@|actions\/cache(?:\/restore)?@/u.test(source)) {
    fail("no untrusted trigger, blanket write token or compiler cache may reach publication");
  }
  if (!/^permissions:\n {2}contents: read\n/mu.test(source)) fail("the default token is read-only");
  if (!/cancel-in-progress: false/u.test(source)) fail("a running release is never cancelled half-way");
  for (const line of source.matchAll(/^\s*(?:- )?uses: (\S+)/gmu)) {
    if (!line[1].startsWith("./") && !/@[0-9a-f]{40}$/u.test(line[1])) fail(`${line[1]} is not pinned to a commit`);
  }
  const found = jobs(source);
  for (const [name, body] of found) {
    const writes = [...body.matchAll(/^\s+([a-z-]+): write$/gmu)].map((match) => `${match[1]}: write`);
    if (writes.length && (writes.length > 1 || writes[0] !== PUBLISHERS[name])) fail(`job ${name} may not hold ${writes.join(", ")}`);
  }
  for (const name of ["prepare", "assets", "binaries", ...Object.keys(PUBLISHERS)]) {
    if (!found.has(name)) fail(`job ${name} is missing`);
  }
  const prepare = found.get("prepare") ?? "";
  if (!prepare.includes('select(.name == "rbit-server")') || !prepare.includes("CHANGELOG.md")) {
    fail("the tag must match the workspace version and CHANGELOG.md must hold its notes");
  }
  const binaries = found.get("binaries") ?? "";
  const targets = [...binaries.matchAll(/^\s+- target: (\S+)$/gmu)].map((match) => match[1]);
  if (targets.length !== TARGETS.length || !TARGETS.every((target) => targets.includes(target))) {
    fail(`the binaries are built for exactly ${TARGETS.join(", ")}`);
  }
  if (!binaries.includes("cargo build --release --locked --package rbit-server --package rbit-runner --target")) {
    fail("every platform builds rbit and rbit-runner from the lockfile");
  }
  if (!binaries.includes("for exe in rbit rbit-runner; do") || !binaries.includes("deploy/rbit-runner.example.toml deploy/rbit-runner.service")) {
    fail("every archive pairs rbit with rbit-runner and their deployment files");
  }
  if (!binaries.includes("node scripts/release-rehearsal.mjs --candidate")) fail("Linux binaries run the upgrade rehearsal before packaging");
  const release = found.get("release") ?? "";
  if (!release.includes("sha256sum -- * > SHA256SUMS") || !/gh release create "\$TAG" dist\/\*[\s\S]*--verify-tag/u.test(release)) {
    fail("the GitHub release publishes every archive with SHA256SUMS for the verified tag");
  }
  const images = ["image", "runner-image"].map((name) => found.get(name) ?? "");
  for (const [index, body] of images.entries()) {
    const name = index ? "runner-image" : "image";
    for (const line of ["push: true", "platforms: linux/amd64,linux/arm64", "provenance: mode=max", "sbom: true", "build-args: BINARY=prebuilt"]) {
      if (!body.includes(line)) fail(`job ${name} publishes with ${line}`);
    }
    if (!/needs: \[prepare, binaries\]/u.test(body)) fail(`job ${name} packages only the built release binaries`);
  }
  if (!images[1].includes("file: Dockerfile.runner") || !images[1].includes("images: ${{ env.RUNNER_IMAGE }}")) {
    fail("the runner image is built from Dockerfile.runner as ghcr.io/gitrbit/rbit-runner");
  }
  const tags = images.map((body) => [block(body, "tags"), block(body, "flavor")].join("\n"));
  if (!tags[0] || tags[0] !== tags[1]) fail("the rbit and rbit-runner images carry the same tags");
  return violations;
}

/** Checks the release workflow below `root`. */
export function checkReleaseRetirement(root) {
  return checkReleaseWorkflow(readFileSync(join(root, RELEASE_WORKFLOW), "utf8"));
}
