/** The retired core Markdown bridge must not return; see docs/design/rewrite/collaboration-timeline-acceptance.md. */

// Markdown reference scanning is rbit_collaboration::references::{markdown_options, scan_markdown},
// behind the `applications` feature. Its options must match safe rendering's, so both see
// the same text events.
const OWNER = "crates/rbit-collaboration/";
const SCANNER = "crates/rbit-collaboration/src/references/markdown.rs";
// Safe rendering's options, wherever the renderer lives in this tree.
const RENDERERS = ["crates/rbit-presentation/src/markdown.rs", "crates/rbit-web/src/render/markdown.rs"];
const RULES = [
  [/\bfn\s+(?:scan_markdown|markdown_options)\b/u, "Markdown reference scanning redefined; use rbit_collaboration::references::scan_markdown", ""],
  [/\brbit_core::[^;]*?\breferences::(?:\{[^}]*)?\b(?:scan_markdown|markdown_options)\b/u,
    "retired rbit_core Markdown bridge path", ""],
  [/\bpulldown_cmark\b/u, "rbit-core parses Markdown again; the reference scan is collaboration's", "crates/rbit-core/"],
];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** The sorted `Options::ENABLE_*` flags a source names. */
function flags(source) {
  return [...new Set([...code(source).matchAll(/Options::(ENABLE_[A-Z_]+)/gu)].map((match) => match[1]))].sort();
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkCollaborationTimelineRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || !path.startsWith("crates/") || path.startsWith(OWNER)) continue;
    const text = code(source);
    for (const [pattern, message, root] of RULES) {
      if (path.startsWith(root || "crates/") && pattern.test(text)) violations.push(`${path}: ${message}`);
    }
  }
  const byPath = new Map(sources);
  const scanner = byPath.get(SCANNER);
  const renderer = RENDERERS.map((path) => [path, byPath.get(path)]).find(([, source]) => source !== undefined);
  if (scanner === undefined) violations.push(`${SCANNER}: the Markdown reference scan is missing`);
  else if (renderer) {
    const [scan, render] = [flags(scanner), flags(renderer[1])];
    if (scan.length === 0 || scan.join(",") !== render.join(",")) {
      violations.push(`${SCANNER}: Markdown options [${scan.join(", ")}] differ from ${renderer[0]} [${render.join(", ")}]`);
    }
  }
  return violations;
}
