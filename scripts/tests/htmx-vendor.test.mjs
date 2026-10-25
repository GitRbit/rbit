import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { patchHtmx } from "../vendor.mjs";

const root = fileURLToPath(new URL("../../", import.meta.url));

async function* files(dir, extensions) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== "vendor" && entry.name !== "themes") yield* files(path, extensions);
    } else if (extensions.some(extension => entry.name.endsWith(extension))) {
      yield path;
    }
  }
}

test("nothing rbit renders or scripts uses hx-on, whose scan the vendored htmx skips", async () => {
  const offenders = [];
  for (const [dir, extensions] of [
    ["crates/rbit-web/templates", [".html"]],
    ["crates/rbit-web/assets", [".js", ".html"]],
    ["crates/rbit-web/src", [".rs"]],
  ]) {
    for await (const path of files(join(root, dir), extensions)) {
      if (/(?:data-)?hx-on[:-]/.test(await readFile(path, "utf8"))) offenders.push(path);
    }
  }
  assert.deepEqual(offenders, []);
});

test("vendored htmx is the release with its hx-on collector returning no elements", async () => {
  const upstream = await readFile(join(root, "node_modules/htmx.org/dist/htmx.min.js"), "utf8");
  const committed = await readFile(join(root, "crates/rbit-web/assets/vendor/htmx.min.js"), "utf8");
  assert.equal(committed.replaceAll("\r\n", "\n"), patchHtmx(upstream));
  assert.notEqual(committed, upstream);
  const collector = /function (\w+)\((\w+)\)\{return\[\]\}function \w+\(\w+\)\{if\(\w+\.querySelectorAll\)/;
  assert.match(committed, collector);
  const name = committed.match(collector)[1];
  assert.match(committed, new RegExp(`\\(${name}\\(\\w+\\),\\w+\\)`), "the patched collector is still the one processNode calls");
});

test("the htmx patch fails loudly when the collector is missing", () => {
  assert.throws(() => patchHtmx("function a(b){return b}"), /expected one hx-on collector, found 0/);
});
