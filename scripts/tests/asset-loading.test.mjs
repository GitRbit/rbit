import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import test from "node:test";
import vm from "node:vm";
import { build } from "esbuild";

const root = fileURLToPath(new URL("../../", import.meta.url));

test("committed browser bundle includes all dependencies without further module requests", async () => {
  const result = await build({
    absWorkingDir: root, entryPoints: ["crates/rbit-web/assets/app.js"],
    bundle: true, minify: true, format: "esm", target: "es2022",
    outfile: "crates/rbit-web/assets/app.bundle.js", write: false, metafile: true,
  });
  assert.equal(Object.values(result.metafile.outputs)[0].imports.length, 0);
  for (const module of ['repo.js', 'contributions.js', 'branch-graph.js', 'mirror-progress.js']) {
    assert.ok(Object.keys(result.metafile.inputs).some(path => path.endsWith(`/assets/${module}`)), `${module} is bundled`);
  }
  const committed = await readFile(new URL("../../crates/rbit-web/assets/app.bundle.js", import.meta.url), "utf8");
  assert.equal(committed.replaceAll("\r\n", "\n"), result.outputFiles[0].text);
});

test("committed Mermaid renderer matches its source and needs no remote modules", async () => {
  const result = await build({
    absWorkingDir: root, entryPoints: ["scripts/mermaid-frame.js"],
    bundle: true, minify: true, format: "iife", target: "es2022", legalComments: "eof",
    outfile: "crates/rbit-web/assets/vendor/mermaid-frame.js", write: false, metafile: true,
  });
  assert.equal(Object.values(result.metafile.outputs)[0].imports.length, 0);
  const committed = await readFile(new URL("../../crates/rbit-web/assets/vendor/mermaid-frame.js", import.meta.url), "utf8");
  assert.equal(committed.replaceAll("\r\n", "\n"), result.outputFiles[0].text);
});

test("theme switching reuses inline theme CSS and retains it until a replacement loads", async () => {
  const source = await readFile(new URL("../../crates/rbit-web/assets/theme-picker.js", import.meta.url), "utf8");
  const ensure = source.slice(source.indexOf("function ensureStylesheet("), source.indexOf("\nfunction apply("));
  let removed = false;
  const original = { dataset: { theme: "base24-berlin" }, getAttribute: () => null, remove: () => { removed = true; } };
  const appended = [];
  const context = vm.createContext({
    BUILT_IN: { system: null, light: "light", dark: "dark" },
    document: {
      head: { querySelectorAll: () => [original], append: (sheet) => appended.push(sheet) },
      createElement: () => ({ dataset: {}, addEventListener(event, callback) { this.loaded = callback; } }),
    },
  });
  vm.runInContext(ensure, context);
  context.ensureStylesheet("base24-berlin");
  assert.equal(appended.length, 0);
  context.ensureStylesheet("base16-gruvbox-dark-hard");
  assert.equal(appended.length, 1);
  assert.equal(removed, false);
  appended[0].loaded();
  assert.equal(removed, true);
  removed = false;
  context.ensureStylesheet("system");
  assert.equal(removed, true);
});
