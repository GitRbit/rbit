import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

function checkoutPolicy(source, expected) {
  const steps = [...source.matchAll(/^      - uses: actions\/checkout@([a-f0-9]{40})[^\n]*\n([\s\S]*?)(?=^      - |^  [a-z-]+:|$(?![\s\S]))/gm)];
  assert.equal(steps.length, expected, "all source checkouts remain covered");
  for (const [, revision, body] of steps) {
    assert.equal(revision, "3d3c42e5aac5ba805825da76410c181273ba90b1");
    assert.match(body, /^          persist-credentials: false$/m);
    assert.doesNotMatch(body, /^          (?:token|ssh-key|allow-unsafe-pr-checkout):/m);
  }
}
const workflows = [
  ["ci.yml", 8],
  ["actions-production-visual.yml", 2],
];
for (const [name, expected] of workflows) {
  const workflow = new URL(`../../.github/workflows/${name}`, import.meta.url);
  test(`${name} checkout credentials are not retained for compiler, Git or container steps`, async () => {
    checkoutPolicy((await readFile(workflow, "utf8")).replaceAll("\r\n", "\n"), expected);
  });

  test(`${name} rejects explicit or implicit credential persistence`, async () => {
    const source = (await readFile(workflow, "utf8")).replaceAll("\r\n", "\n");
    assert.throws(() => checkoutPolicy(source.replace("persist-credentials: false", "persist-credentials: true"), expected));
    assert.throws(() => checkoutPolicy(source.replace("        with:\n          persist-credentials: false\n", ""), expected));
    assert.throws(() => checkoutPolicy(source.replace("persist-credentials: false", "persist-credentials: false\n          token: unexpected"), expected));
  });
}
