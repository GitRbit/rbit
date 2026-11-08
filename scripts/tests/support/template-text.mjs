// Finds hard-coded natural-language text in an Askama template: text nodes and the
// accessible or visible attributes (aria-label, title, placeholder, alt) that remain
// after every `{{ }}` expression, `{% %}` tag and `{# #}` comment is removed. Catalog
// text reaches the page only through those expressions and `{% call ui::localized %}`.

/// Attributes whose value a reader sees or hears.
export const TEXT_ATTRIBUTES = ["aria-label", "title", "placeholder", "alt"];

// Elements whose content is code or machine text, never prose: skipped entirely.
const MACHINE_ELEMENTS = new Set(["script", "style", "pre", "code", "kbd", "samp", "svg"]);
const VOID_ELEMENTS = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"]);

// Reviewed words that read the same in every language: product and protocol names, code
// tokens, file formats and units. Matched as whole tokens, case-sensitively.
const ALLOWED_TOKENS = new Set([
  "rbit", "Git", "GitHub", "Gitea", "LFS", "SSH", "HTTPS", "HTTP", "OpenSSH", "OIDC", "OpenID", "TOTP", "WebAuthn",
  "SHA-256", "SHA-1", "sha256", "tar.gz", "zip", "README", "README.md", "Markdown", "JSON", "API", "REST",
  "KB", "KiB", "MB", "MiB", "GB", "GiB", "ms", "px",
]);

// Askama syntax, in the order it must be stripped: comments may contain tags.
const ASKAMA = [/\{#[\s\S]*?#\}/g, /\{%[\s\S]*?%\}/g, /\{\{[\s\S]*?\}\}/g];
const TAG = /<(\/?)([a-zA-Z][a-zA-Z0-9-]*)((?:[^<>"']|"[^"]*"|'[^']*')*)>/g;
const ATTRIBUTE = /([a-zA-Z_:][-a-zA-Z0-9_:.]*)\s*=\s*(?:"([^"]*)"|'([^']*)')/g;

/// The template with its Askama syntax replaced by spaces, so that `Hello {{ name }}`
/// keeps `Hello` while a whole expression vanishes.
export function stripAskama(source) {
  const markup = source.replace(/<!--[\s\S]*?-->/g, " ").replace(/<![a-zA-Z][^>]*>/g, " ");
  return ASKAMA.reduce((text, pattern) => text.replace(pattern, " "), markup);
}

/// The words of `text` a translator would need to translate, or "" when none remain.
export function untranslatable(text) {
  const words = decodeEntities(text).split(/\s+/)
    .map((word) => word.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ""))
    .filter((word) => word && !ALLOWED_TOKENS.has(word) && /\p{L}{2}/u.test(word));
  return words.join(" ");
}

function decodeEntities(text) {
  return text.replace(/&(?:[a-zA-Z]+|#\d+|#x[0-9a-fA-F]+);/g, " ");
}

/// Every hard-coded text finding in `source`: `{ kind, text }` with kind `text` or the
/// attribute name. `snippets` are reviewed literal fragments this template may contain.
export function findHardCodedText(source, snippets = []) {
  let html = stripAskama(source);
  for (const snippet of snippets) html = html.split(snippet).join(" ");
  const findings = [];
  const report = (kind, value) => {
    const words = untranslatable(value);
    if (words) findings.push({ kind, text: value.replace(/\s+/g, " ").trim() });
  };
  let cursor = 0;
  let skip = null; // { name, depth } while inside machine text
  for (const match of html.matchAll(TAG)) {
    const [whole, closing, rawName, attributes] = match;
    const name = rawName.toLowerCase();
    if (!skip) report("text", html.slice(cursor, match.index));
    cursor = match.index + whole.length;
    if (skip) {
      if (name === skip.name) skip.depth += closing ? -1 : (whole.endsWith("/>") ? 0 : 1);
      if (skip.depth === 0) skip = null;
      continue;
    }
    if (closing) continue;
    for (const [, attribute, double, single] of attributes.matchAll(ATTRIBUTE)) {
      if (TEXT_ATTRIBUTES.includes(attribute.toLowerCase())) report(attribute.toLowerCase(), double ?? single);
    }
    const machine = MACHINE_ELEMENTS.has(name) || /\b(?:dir="ltr"|translate="no")/.test(attributes);
    if (machine && !VOID_ELEMENTS.has(name) && !whole.endsWith("/>")) skip = { name, depth: 1 };
  }
  if (!skip) report("text", html.slice(cursor));
  return findings;
}
