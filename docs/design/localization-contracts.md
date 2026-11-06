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

Static English labels borrow their compiled catalog literals through private
`Cow<'static, str>` storage. Interpolated/count messages and diagnostic transformations
own their resulting text. This keeps ordinary labels from adding heap allocations
relative to the original static strings; representation tests enforce the distinction
without exposing an allocation policy through the public presentation API. English
literals have one authoritative match table used by both normal and pseudo rendering.

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

## Web adapter status

`rbit_web::localization::RequestLocale` resolves and memoizes one locale in request
extensions. Its production default enables English only. Tests/embedded composition
can inject a `LocaleResolver` explicitly; incoming locale headers cannot enable
diagnostic catalogs. Trusted `LocaleChoices` extensions provide explicit/account
preferences. No persisted account-locale schema or locale-setting endpoint exists
yet; the identity/preference migration slice owns those integrations. The adapter
accepts one bounded `rbit-locale` cookie value and one Accept-Language header;
duplicate headers/cookie choices are disregarded rather than selected ambiguously.
`Page` defaults to English and offers an explicit `with_locale` builder. Only Explore
currently assigns its negotiated request locale and language/cache response headers.
The generic Page extractor does not negotiate: unrelated pages such as login stay
English/LTR even when diagnostic catalogs are injected into a host. This avoids
partially changing document direction without a matching response/cache contract.
This foundation does not claim that existing platform chrome or other routes have
translated messages.

## Explore production slice

Explore continues to use the existing public-only repository search service and stable
query values/URLs for search, sort, archive filters and paging. Feature-owned typed
messages cover its title, description, controls, result count, empty states, archive
badge, star label and pagination. Repository names/descriptions remain user content.
Existing core relative timestamps and platform navigation chrome are still English;
locale-aware date/relative-time formatting remains a later presentation contract.

Full documents include `home/explore_results.html`. A live search with exactly one
`HX-Request: true` and `HX-Target: explore-results` renders that same component
directly, preceded by its updated document title. It does not render the base layout
and cut HTML afterward. Requests carrying boosted/history-restore headers always get
the full document, as do ordinary GETs and ambiguous/repeated shape headers. The
form remains an ordinary GET form without JavaScript and retains HTMX push-URL and
select/swap behavior. Both shapes report Content-Language and vary on language,
cookies and response-shape headers; existing private revalidation remains in force.

There is no locale mutation UI in this slice. Future locale-setting flows must reset
locale-sensitive HTMX history/navigation caches before enabling account language
switching; diagnostic resolver injection does not constitute that completed feature.
HTTP regressions cover component equality, locale/direction consistency, full history
fallback, private repositories even for their signed-in owner, malicious interpolation,
shape-header ambiguity and existing URL/filter/pagination behavior.


## Deferred Explore text

`Catalog::display(Message)` returns plain borrowed `LocalizedDisplay`, with the same
ordinary template escaping boundary as `LocalizedText`. English ExploreSubtitle,
PageOf and Stars format directly into the caller's writer from shared catalog
literals. Diagnostic locales and other messages delegate to the existing owned
catalog path; punctuation, plural wording and RTL isolation remain byte-compatible.
No HTML-safety trait or raw string slot is introduced. `Catalog::text` stays available
with unchanged semantics.

Explore constructs the subtitle only while rendering a full-page header, using a
borrow of Page.site_name rather than a self-referential view. PageOf is requested
inside the existing multi-page template conditional; star labels write on demand.
CountForQuery retains its previous owned formatting, which also existed in the main
listing implementation. This removes specific source-level formatting/String work;
no allocator-count, end-to-end latency or renderer-winner claim follows without a
controlled measurement. The default renderer and shared-shell experiment use the
same methods and exact full/fragment owned-catalog equivalence fixtures.


Actions run rerun/summary labels use the same deferred plain-Display contract via
`Catalog::actions_display` and borrowed `ActionsDisplay`. English dynamic labels write
shared catalog literals and existing job names directly to the destination. Diagnostic
locales delegate to the existing owned Actions catalog without changing its output.
RunPage methods return borrowed labels only at render time; templates introduce no
English literals or HTML-safety bypass. This removes introduced per-label owned
formatting buffers at the source level; no allocator-count or latency claim is made.
