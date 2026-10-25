// Copies third-party browser scripts from node_modules into the embedded asset folder.
// Vendored files are committed so `cargo build` works without Node installed.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { build } from "esbuild";

const target = "crates/rbit-web/assets/vendor";

// htmx looks for `hx-on:*` attributes by running an XPath over every attribute of every
// element, at startup and again for each swapped subtree. rbit never uses `hx-on` (the
// strict CSP and `allowEval: false` would block its handlers anyway), so the collector
// (`findHxOnWildcardElements` in htmx.js) is replaced with one returning no elements.
// The minified names change between releases; the shape is matched instead, and the
// build fails if it is not found exactly once.
const HX_ON_COLLECTOR = /function (\w+)\((\w+)\)\{const (\w+)=\[\];if\(\2 instanceof DocumentFragment\)\{for\(const (\w+) of \2\.childNodes\)\{\w+\(\4,\3\)\}\}else\{\w+\(\2,\3\)\}return \3\}/g;

export function patchHtmx(source) {
  const matches = source.match(HX_ON_COLLECTOR) ?? [];
  if (matches.length !== 1) {
    throw new Error(`htmx patch: expected one hx-on collector, found ${matches.length}; update scripts/vendor.mjs for this htmx release`);
  }
  return source.replace(HX_ON_COLLECTOR, "function $1($2){return[]}");
}

const files = [
  ["node_modules/htmx.org/dist/htmx.min.js", "htmx.min.js", patchHtmx],
];

async function main() {
  mkdirSync(target, { recursive: true });
  await build({ entryPoints: ['scripts/mermaid-frame.js'], bundle: true, minify: true,
    format: 'iife', target: 'es2022', outfile: `${target}/mermaid-frame.js`, legalComments: 'eof' });
  for (const [source, name, patch] of files) {
    writeFileSync(`${target}/${name}`, patch(readFileSync(source, "utf8")));
    console.log(`vendored ${name} -> ${target}`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
