/**
 * Retired presentation paths must not return; see docs/design/rewrite/
 * repositories-documents-acceptance.md, presentation-navigation-acceptance.md and
 * presentation-themes-pwa-acceptance.md.
 */

// Markdown rendering and sanitising, document tab selection, the partial-navigation cut,
// the theme catalog and the installable-app documents are decided in rbit-presentation.
// rbit-web only adapts them.
const OWNER = "crates/rbit-presentation/";
const WEB = "crates/rbit-web/";
const CATALOG = "crates/rbit-web/src/themes/catalog.rs";
// [pattern, message, root the rule applies below]
const RULES = [
  [/\bammonia::/u, "HTML sanitising outside rbit-presentation; use rbit_presentation::markdown", "crates/"],
  [/\bpush_html\b|\bpulldown_cmark::(?:html\b|\{[^}]*\bhtml\b)/u, "Markdown rendered to HTML outside rbit-presentation; use rbit_presentation::markdown", "crates/"],
  [/\benum\s+(?:LinkKind|ResolvedLink)\b/u, "document link types redefined; use rbit_presentation::markdown::{LinkKind, ResolvedLink}", "crates/"],
  [/\bfn\s+(?:unique_slug|with_heading_ids|resolve_srcset|prefixed_fragment|raw_depth|filter_attribute|configured_builder)\b/u,
    "retired Markdown renderer function redefined in rbit-web", WEB],
  [/\bfn\s+(?:text_stem|readme_priority|license_name|sort_and_bound)\b|\bconst\s+MAX_DOCUMENTS\b/u,
    "document tab selection redefined; use rbit_presentation::documents", WEB],
  [/\b(?:const|static)\s+\w+\s*:[^=;]*=\s*"<!--\/?partial:/u, "partial-navigation marker redefined; use rbit_presentation::navigation", WEB],
  [/\bfn\s+(?:slice|between)\s*\(/u, "partial-navigation cut redefined; use rbit_presentation::navigation::slice", "crates/rbit-web/src/partial.rs"],
  [/\bstruct\s+TintedTheme\b|\benum\s+Theme\b|\bstatic\s+THEMES\b|\bfn\s+toggled\b/u, "theme catalog or selection redefined; use rbit_presentation::themes", WEB],
  [/"start_url"\s*:|\.replace\(\s*"__VERSION__"|\.btn-primary\{/u, "manifest, worker version or branding sheet built outside rbit_presentation::pwa", WEB],
];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkPresentationRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (path === CATALOG) violations.push(`${path}: the theme catalog is generated into rbit-presentation`);
    if (!path.endsWith(".rs") || path.startsWith(OWNER)) continue;
    const text = code(source);
    for (const [pattern, message, root] of RULES) {
      if (path.startsWith(root) && pattern.test(text)) violations.push(`${path}: ${message}`);
    }
  }
  return violations;
}
