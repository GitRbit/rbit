# Localization contracts

`rbit-localization` is a pure presentation foundation with no I/O, HTTP, account,
database, HTML, async runtime or production dependency. It supplies typed compiled
English messages for the first repository-list slice. Catalogs are never parsed per
request. English is the only supported translated language; `en-XA` and `ar-XB` are
opt-in expansion/RTL diagnostics, not actual translations or Arabic grammar.

## Negotiation

Construct `LocaleResolver::default()` for English production behavior, or
`LocaleResolver::new(default, true)` to enable diagnostic catalogs. Configuration
rejects a pseudo default if diagnostics are disabled. Resolve borrowed
`LocalePreferences` in this order: explicit request, authenticated account,
anonymous cookie, Accept-Language, configured default. Invalid or unsupported values
fall through. An explicit preference must come from an adapter's locale-setting flow,
not from arbitrary query data supplied to unrelated operations. Cookie attributes,
CSRF, persistence and account preference loading belong to that adapter.

The allocation-free negotiator accepts bounded ASCII basic language ranges, matches
case-insensitively, and looks up progressively shorter subtags against the closed
allowlist. It removes dangling extension singletons during lookup. Thus `en-US` and
`en-Latn-US` fall back to `en`, while `fr-CA` does not invent a translation. This is
basic-range lookup, not a full IANA language-tag registry or a general BCP 47 parser.
Tags use hyphens; underscore aliases are rejected. Public output tags/direction are
static enum values, never reflected header strings.

Accept-Language work is limited to 4,096 bytes, 32 comma-delimited members and 63
bytes per range. Oversized lists, non-ASCII input and control-character injection
disregard the entire header. Malformed individual members are ignored. Quality is
integer thousandths, accepting RFC quality values 0 through 1 with at most three
decimal places; unsupported parameters, duplicate q parameters and invalid numbers
are ignored with their member. The most-specific range mapping to a locale governs
its quality; first occurrence wins at equal specificity, including conflicting
duplicates. Higher quality wins, then original member order. `q=0` excludes the
candidate from header negotiation. Wildcard considers only the configured default,
never opts an English deployment into a diagnostic catalog. If every candidate is
excluded/unavailable, the configured default is served, following the permitted
HTTP behavior of disregarding an unsatisfiable preference rather than returning 406.

See [HTTP language negotiation](https://www.rfc-editor.org/rfc/rfc9110.html#name-accept-language)
and [language lookup](https://www.rfc-editor.org/rfc/rfc4647.html#section-3.4).

## Text and grammar boundary

`Catalog::new(locale).text(Message::RepositoryCount { count })` formats a complete
typed message. Stable IDs such as `repositories.count` are separate from wording.
Counts are unsigned typed values; visibility is an exhaustive selector, not a string.
The current compiled catalog uses English cardinal singular only for integer 1 and
plural otherwise. Pseudo catalogs deliberately use the same English source grammar.
This is not a universal plural engine: no other translated language may be added by
copying that rule.

`LocalizedText` is plain Unicode text with ordinary Display behavior. Arguments are
copied as text, never parsed as catalog syntax or HTML. Render through ordinary
Askama interpolation, which escapes exactly once in both text and attribute
contexts. Do not apply `safe`, write to `innerHTML`, or add a trusted-HTML trait.
Use textContent for DOM updates. The crate's integration test renders real Askama
templates with malicious text and attribute arguments. Plain-text email/CLI consumers
may use the text directly under their own terminal/header control-character rules.

Expansion accents and lengthens only literal message segments; arguments retain their
identifiers. RTL diagnostics set document `dir=rtl` and wrap textual arguments in
Unicode FSI/PDI. This tests layout and mixed-direction identifiers; it is neither an
Arabic translation nor a guarantee against all misleading Unicode identifiers.

## First-slice integration and future catalogs

The HTTP adapter should pass Catalog/Locale to presentation; set both HTML `lang` and
`dir`, use locale as a render/cache dimension, keep private responses private, and
vary header-negotiated responses appropriately. Explicit locale changes must refresh
the relevant full/fragment/history state. Domain errors and protocol fields remain
stable codes with arguments. Do not persist only localized output as event history.

Before shipping a real second language, evaluate a Fluent-backed adapter behind the
same typed `Message` contract. [Fluent selectors](https://projectfluent.org/fluent/guide/selectors.html)
support grammatical forms beyond English. Load/validate catalogs once at startup;
require message/argument completeness and deterministic English fallback; benchmark
binary growth, resident catalog bytes, cold/warm formatting, allocations and concurrent
access against this compiled implementation. The initial choice adds zero production
dependencies and avoids speculative language-engine overhead; it does not claim to
have benchmarked Fluent. Real translations also require locale-aware dates/numbers,
reviewed grammar, fallback policy and RTL/accessibility acceptance before enablement.
