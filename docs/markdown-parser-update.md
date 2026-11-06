# Markdown parser security update

The minimum `pulldown-cmark` version is 0.13.6, and the lockfile selects that patched release. This resolves the dependency gate for [RUSTSEC-2026-0433](https://github.com/RustSec/advisory-db/blob/main/crates/pulldown-cmark/RUSTSEC-2026-0433.md), which concerns quadratic parsing of nested image wikilinks when the optional wikilink mode is enabled.

Rbit's renderer and reference scanner use explicit parser options and do not enable wikilinks or `Options::all()`. Their supported Markdown features remain unchanged. Raising the minimum also protects future changes that might enable additional parser modes.

The [upstream 0.13.6 release](https://github.com/pulldown-cmark/pulldown-cmark/releases/tag/v0.13.6) backports the parser fix. No advisory exception or unrelated dependency upgrade is part of this update. Existing reference, Markdown, sanitization and web regressions remain the compatibility checks.
