// Copies third-party browser scripts from node_modules into the embedded asset folder.
// Vendored files are committed so `cargo build` works without Node installed.
import { copyFileSync, mkdirSync } from "node:fs";

const target = "crates/rustit-web/assets/vendor";
mkdirSync(target, { recursive: true });
copyFileSync("node_modules/htmx.org/dist/htmx.min.js", `${target}/htmx.min.js`);
console.log(`vendored htmx.min.js -> ${target}`);
