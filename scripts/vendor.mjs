// Copies third-party browser scripts from node_modules into the embedded asset folder.
// Vendored files are committed so `cargo build` works without Node installed.
import { copyFileSync, mkdirSync } from "node:fs";

const target = "crates/rbit-web/assets/vendor";
const files = [
  ["node_modules/htmx.org/dist/htmx.min.js", "htmx.min.js"],
];

mkdirSync(target, { recursive: true });
for (const [source, name] of files) {
  copyFileSync(source, `${target}/${name}`);
  console.log(`vendored ${name} -> ${target}`);
}
