import assert from "node:assert/strict";
import test from "node:test";
import { readFile, readdir } from "node:fs/promises";
import { Script } from "node:vm";
const workflow = new URL("../../.github/workflows/ci.yml", import.meta.url);
const pinnedAction = "Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0";

function cacheSteps(source) {
  return [...source.matchAll(/^      - uses: (Swatinem\/rust-cache@[^\s]+)[^\n]*\n([\s\S]*?)(?=^      - |^  [a-z-]+:|$(?![\s\S]))/gm)]
    .map(([, action, body]) => ({ action, body }));
}
function saveExpression(body) {
  const value = body.match(/save-if: >-\n([\s\S]*?\}\})/)?.[1].trim();
  assert.ok(value?.startsWith("${{") && value.endsWith("}}"), "explicit folded save expression required");
  return value.slice(3, -2).trim();
}
function saves(expression, event, ref) {
  // Execute the actual workflow expression against event/ref fixtures. Only the
  // two context fields, fixed string comparisons and prefix/suffix calls are legal.
  assert.ok(expression.length <= 4096, "bounded cache-policy expression required");
  const tokens = expression.match(/github\.(?:event_name|ref)|startsWith|endsWith|'[^'\\]*'|\s+|&&|\|\||==|[(),]/g);
  assert.equal(tokens?.join(""), expression, "unsupported cache-policy expression");
  assert.ok(tokens.length <= 512, "bounded cache-policy token count required");
  return new Script(expression).runInNewContext({ github: { event_name: event, ref },
    startsWith: (value, prefix) => value.startsWith(prefix), endsWith: (value, suffix) => value.endsWith(suffix),
  }, { timeout: 1000 });
}
function dependencyPolicy(source) {
  const steps = cacheSteps(source);
  assert.equal(steps.length, 3, "lint, matrix tests and release-build keep their existing caches");
  for (const step of steps) {
    assert.equal(step.action, pinnedAction);
    assert.match(step.body, /workspaces: "\. -> \.cargo-build"/);
    for (const field of ["cache-workspace-crates", "cache-all-crates", "cache-bin", "cache-on-failure"]) {
      assert.match(step.body, new RegExp(`^          ${field}: false$`, "m"));
    }
    assert.doesNotMatch(step.body, /shared-key:|prefix-key:|cache-directories:|add-job-id-key:|add-rust-environment-hash-key:/);
    const expression = saveExpression(step.body);
    for (const [event, ref, expected] of [
      ["pull_request", "refs/pull/42/merge", true],
      ["pull_request", "refs/pull/91/merge", true], // Fork and same-repository runs use this server-owned ref shape.
      ["push", "refs/heads/main", true], ["workflow_dispatch", "refs/heads/main", true],
      ["push", "refs/heads/feature/example", false], ["push", "refs/tags/v1.0.0", false],
      ["workflow_dispatch", "refs/heads/feature/example", false],
      ["pull_request", "refs/heads/main", false], ["pull_request", "refs/pull/42/head", false],
      ["pull_request_target", "refs/heads/main", false], ["workflow_run", "refs/heads/main", false],
      ["issue_comment", "refs/heads/main", false], ["", "", false],
    ]) assert.equal(saves(expression, event, ref), expected, `${event} ${ref}`);
  }
  assert.equal(steps.filter(({ body }) => /^          key: release$/m.test(body)).length, 1);
  assert.match(source, /^permissions:\n  contents: read\n/m);
  assert.doesNotMatch(source, /cache-mode:|pull_request_target:|workflow_run:|actions: write|permissions: write-all/);
  return steps;
}

test("PR dependency saves use the merge-ref boundary without token escalation or key churn", async () => {
  dependencyPolicy((await readFile(workflow, "utf8")).replaceAll("\r\n", "\n"));
});

test("policy refuses workspace outputs, tools, failure saves and privileged-trigger save expansion", async () => {
  const source = (await readFile(workflow, "utf8")).replaceAll("\r\n", "\n");
  for (const field of ["cache-workspace-crates", "cache-all-crates", "cache-bin", "cache-on-failure"]) {
    assert.throws(() => dependencyPolicy(source.replace(`${field}: false`, `${field}: true`)));
  }
  assert.throws(() => dependencyPolicy(source.replace("github.event_name == 'pull_request' && startsWith", "github.event_name == 'workflow_run' && startsWith")));
  assert.throws(() => dependencyPolicy(source.replace("key: release", "shared-key: release")));
  assert.throws(() => dependencyPolicy(source.replace("contents: read", "contents: write")));
  const expression = saveExpression(cacheSteps(source)[0].body);
  assert.throws(() => dependencyPolicy(source.replace(expression, "(".repeat(2048) + expression + ")".repeat(2048))), /bounded cache-policy expression/);
  assert.throws(() => dependencyPolicy(source.replace(expression, "(".repeat(300) + expression + ")".repeat(300))), /bounded cache-policy token count/);
});

test("tag publication and privileged follow-up workflows cannot consume PR compiler caches", async () => {
  const directory = new URL("../../.github/workflows/", import.meta.url);
  for (const name of await readdir(directory)) {
    if (!/\.ya?ml$/.test(name)) continue;
    const source = (await readFile(new URL(name, directory), "utf8")).replaceAll("\r\n", "\n");
    if (name === "release.yml" || /^  workflow_run:/m.test(source)) {
      assert.doesNotMatch(source, /Swatinem\/rust-cache@|actions\/cache(?:\/restore)?@/);
    }
  }
});
