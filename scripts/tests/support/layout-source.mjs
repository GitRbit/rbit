// The page layout as one source text. `templates/base.html` renders its fixed parts as
// templates of their own (`templates/layout/*.html`, compiled once; see views/layout.rs)
// and keeps only the blocks pages override. Tests that check the layout's markup read
// this: base.html with each `{{ page.layout_*()|safe }}` replaced by that part's file.
import { readFileSync } from "node:fs";

const templates = new URL("../../../crates/rbit-web/templates/", import.meta.url);
const read = (path) => readFileSync(new URL(path, templates), "utf8").replace(/\r\n/g, "\n");

export const LAYOUT_PARTS = ["head", "top", "menus", "foot"];

export function layoutSource() {
  let source = read("base.html");
  for (const part of LAYOUT_PARTS) {
    const call = `{{ page.layout_${part}()|safe }}`;
    if (source.split(call).length !== 2) throw new Error(`base.html renders layout/${part}.html once`);
    source = source.replace(call, () => read(`layout/${part}.html`).replace(/\n$/, ""));
  }
  return source;
}
