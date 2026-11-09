# Localization contracts

`rbit-localization` is a pure presentation foundation with no I/O, HTTP, account,
database, HTML, async runtime or production dependency. It supplies typed compiled
messages in English (the complete source), Spanish, and the locales prepared for
translators: Simplified Chinese (`zh-Hans`), Brazilian Portuguese (`pt-BR`), Japanese
(`ja`), German (`de`), Russian (`ru`), French (`fr`) and Korean (`ko`). Catalogs are
never parsed per request. Untranslated messages and feature regions explicitly retain
English, marked `lang="en"`. `en-XA` and `ar-XB` remain opt-in expansion/RTL
diagnostics, not translations or Arabic grammar. See [account activation](account-locale-activation.md).

## Negotiation

Construct `LocaleResolver::default()` for English-default production behavior negotiating
every language of `Locale::LANGUAGES`, or
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
`en-Latn-US` fall back to `en`, `fr-CA` to `fr`, while `it` finds nothing. Two
languages have their own rule. Portuguese (`pt`, `pt-PT`) is served the Brazilian
catalog: rbit has one Portuguese translation, Brazilian Portuguese has by far the most
readers and is the variant most software ships first, and a Portuguese reader is better
served by it than by English; a separate `pt-PT` can be added later as its own locale.
Chinese is decided by script, then region: `zh-Hans…`, `zh-CN`, `zh-SG`, `zh-MY` and a bare
`zh` select `zh-Hans`; `zh-Hant…`, `zh-TW`, `zh-HK`, `zh-MO` and other regions never do, so
Traditional Chinese readers get their next acceptable language or English rather than
Simplified characters. This is
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
English integer count nouns retain their existing behavior. Spanish count nouns use
singular for 1 (absolute 1 for signed stars), otherwise plural. These particular nouns
share the plural form across CLDR Spanish many/other categories. Pseudo catalogs
retain the English source grammar.

Every other translation chooses its noun forms with `Plural::of(locale, count)` (or
`text.plural(count)` on a writer), which returns the CLDR 46 category of a whole number:
`en`/`de` one (1) and other; `es` one (1), many (whole millions) and other; `fr`/`pt-BR`
one (0 and 1), many (whole millions) and other; `ru` one (1, 21, …; not 11), few (2–4,
22–24, …; not 12–14) and many (the rest); `zh-Hans`/`ja`/`ko` other only.
`Plural::categories(locale)` lists them. Counts, times, dates and sizes follow the page's
language as [localized times and numbers](localized-time.md) describes.

`LocalizedText` is plain Unicode text with ordinary Display behavior. Arguments are
copied as text, never parsed as catalog syntax or HTML. Render through ordinary
Askama interpolation, which escapes exactly once in both text and attribute
contexts. Do not apply `safe`, write to `innerHTML`, or add a trusted-HTML trait.
Use textContent for DOM updates. The crate's integration test renders real Askama
templates with malicious text and attribute arguments. Plain-text email/CLI consumers
may use the text directly under their own terminal/header control-character rules.

Static English/Spanish labels borrow their compiled catalog literals through private
`Cow<'static, str>` storage. Interpolated/count messages and diagnostic transformations
own their resulting text. This keeps ordinary labels from adding heap allocations
relative to the original static strings; representation tests enforce the distinction
without exposing an allocation policy through the public presentation API. English
literals have one authoritative match table used by both normal and pseudo rendering.

Expansion accents and lengthens only literal message segments; arguments retain their
identifiers. RTL diagnostics set document `dir=rtl` and wrap textual arguments in
Unicode FSI/PDI. This tests layout and mixed-direction identifiers; it is neither an
Arabic translation nor a guarantee against all misleading Unicode identifiers.

## Untranslated messages and English fallback

The original message families (`Message`, `ChromeMessage`, `ActionsMessage`,
`ActionsOverviewMessage`, `OverviewControlMessage`, `ActionsJobMessage`, `ThemeMessage`
and `LanguageMessage`) are complete in English and Spanish: their Spanish match tables are
exhaustive, so a new variant does not compile without a Spanish entry. Every other locale
gets their English text without new arms (`Locale::has_original_catalog` is `false`), and
pages mark those regions English: the header, the footer's chrome and the theme picker
carry `lang="en"`, and `Page::with_locale` (the constructor of the Explore, Actions and
language-settings pages built on these families) keeps `<main lang="en">` in such a
locale. Newer families implement `MessageFamily` instead, where English is the only
required catalog:

- `id` returns the stable key and `english` writes the source text through a
  `MessageWriter` (`literal` for wording, `argument` for names, environments and
  lists, `number` for counts). Plurals are chosen in the arm, as elsewhere.
- `translate(locale, writer)` writes a translation and returns `true`. Its default
  returns `false` for every locale, so a new key or a whole new family compiles with
  English alone (`tests/fallback.rs` proves this with a test-only family).
- `Catalog::localized(message)` returns a `Localized`: the text plus whether it fell
  back. Without a translation it is the English text with `fallback() == true`.
  English and the `en-XA`/`ar-XB` diagnostics derive from English and never fall back.
  `Localized::english_only` wraps text that only exists in English, such as a service's
  error message, with the same rule.

Templates render a `Localized` through the `ui::localized` macro in `macros.html`.
It marks fallback text `<span lang="en">` so assistive technology reads it as
English. Translated text gets no wrapper in English, Spanish and the diagnostics; in
the other locales it is wrapped `<span lang="…">` in its own language
(`Localized::marked_lang`), because a page built on the original families may surround
it with English. Neither sets `dir`: the text follows the
page direction, so it aligns with its translated neighbours in a right-to-left page.
Do not hard-code `lang="en"` or `dir="ltr"` around untranslated prose. Keep explicit
`dir="ltr"` only for machine values (code, paths, shas, identifiers).

`Page::text(message)` looks a family message up in the page's language;
`{% call ui::text(page, Family::Key) %}` renders it through `ui::localized`, and an
attribute uses `{{ page.text(Family::Key) }}`. A page whose text all comes from the
catalog calls `Page::with_catalog_content`, so its `<main>` carries the page's
language. The pages around the repositories (sign-in, recovery, the front page and
dashboards, the inbox, error pages and the shared page macros) use the families in
`rbit_localization::shell`. The `catalog-guard` node test fails on words hard-coded in
a template, in every template: there is no exemption list.

Sentences that embed markup (a link, a time, a branch, a count in its own element)
are `RichFamily` messages, used by the issue, pull request and timeline pages
(`CollabSentence`, `TimelineEventMessage`). Their writer adds numbered slots besides
literals, arguments and numbers; `rich(locale)` returns a `Rich` of text parts and
slots plus the same fallback flag. The template renders each slot's markup and the
text between them, and puts `lang="en"` on the sentence's own element when it fell
back, so a translation may reorder the pieces without splitting the sentence. Each
variant documents its slots; timeline events always start with the actor in slot 0.
Plain families look themselves up with `localized(locale)` (for example
`IssueMessage::NewIssue.localized(page.locale)`), so templates need no catalog helper.

To add a locale, add its `Locale` variant, tag, catalog key, native name, direction and
`Plural` rules, a marker and a table directory in `catalog/translations`, its account
value (`AccountLocale` and a migration widening the stored values) and, when its script
needs it, a font stack in `rbit-web/styles/scripts.css`. Then translators fill its table
as described in "Translating a locale" below. `SettingsNavMessage` keeps a hand-written
Spanish arm that reuses the shell's Spanish labels before it consults the table.

The account settings, administration, organization, access, branch protection and
webhook pages use English-only families in the `settings` module (`settings::keys`,
`settings::admin` and so on). The `labels!` macro there declares a family of fixed
labels from one `Variant = "key" => "English",` line each, and `messages!` one with
arguments. Every family offers `text(&locale)`, which templates call with the page
locale. Sentences that wrap a link or a machine value in markup are split at that
markup into separate keys. The macros generate each family's `translate`, which
consults the translation tables like every other family.

## Translating a locale

Every `MessageFamily` and `RichFamily` family (78 families, about 2,400 messages) consults
a per-locale table from its `translate` method. Tables live in
`crates/rbit-localization/src/catalog/translations/<locale>/`, one directory per locale
(`es`, `zh_hans`, `pt_br`, `ja`, `de`, `ru`, `fr`, `ko`), mirroring the source modules:
the entries for `catalog/collab/review.rs` are in `translations/<locale>/collab/review.rs`,
those for `catalog/wiki.rs` in `translations/<locale>/wiki.rs`. A translator edits only
their own locale's directory, so translators of different languages never touch the
same file. Each file already lists every family of its source file with an empty table:

```rust
entries! { De:
    ReviewMessage<'a> => MessageWriter |text| {}
}
```

To translate a message, add a match arm on the family inside the braces. End each arm
with a comma unless it is a block. Write the text the way the English arm in the source
file does: `text.literal(..)` for wording, `text.argument(..)` for a name or other
interpolated value, `text.number(..)` for a count.

```rust
use crate::plural::Plural;

entries! { De:
    ReviewMessage<'a> => MessageWriter |text| {
        ReviewMessage::Approve => text.literal("Genehmigen"),
        ReviewMessage::CommentOnLine { line } => {
            text.literal("Kommentar zu Zeile ");
            text.number(line);
        }
        ReviewMessage::RemoveRequestFor { name } => {
            text.literal("Prüfanfrage an ");
            text.argument(name);
            text.literal(" zurückziehen");
        }
        ReviewMessage::OutdatedComments { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " veralteter Kommentar steht in der Unterhaltung",
                _ => " veraltete Kommentare stehen in der Unterhaltung",
            });
        }
    }
}
```

A message without an arm keeps its English text, marked `lang="en"`; the arm makes it
translated, in its own language. Add arms in any order, a few at a time.

- **Plurals.** Match on `text.plural(count)` (a `Plural`: `One`, `Few`, `Many`, `Other`,
  `Zero`, `Two`), and cover the categories your language has, listed under "Text and
  grammar boundary"; end with `_` for the rest. Russian needs `One`, `Few` and `Many`;
  Chinese, Japanese and Korean have only `Other`, so they write one form without a match.
  `Many` in Spanish, French and Portuguese is only whole millions: a `_` arm sharing
  the `Other` form is right there.
- **Rich sentences** (`CollabSentence`, `TimelineEventMessage`, written to a
  `RichWriter`) also have `text.slot(n)`: the markup the template inserts (a link, a time,
  a branch). Each variant's documentation says what each slot holds. Keep every slot the
  English arm writes, exactly once, and move it wherever your grammar needs it.
- **Arguments and slots are not optional.** Every `argument`, `number` and `slot` of the
  English arm must appear in the translation; never replace one with literal text.
- **No HTML.** Messages are plain text, escaped by the templates: never write tags,
  entities or Markdown. Use the Unicode character itself (`\u{a0}`, `«`, `„`).
- **Split fragments.** Some families split a sentence at markup into `…Lead` and `…Tail`
  keys (the markup sits between them in the template). Translate them so they read as one
  sentence around that markup.
- **Native typography.** French puts a no-break space (`\u{a0}`, or the narrow `\u{202f}`)
  before `:` `;` `?` `!` and inside `« »`. Use the language's quotation marks: `«…»`
  (French, Russian, Spanish), `„…“` (German), `“…”` (Portuguese, Chinese), `「…」`
  (Japanese), `“…”` or `「…」` (Korean). Chinese and Japanese use full-width punctuation
  (`，。：；？！（）`), with no space between Han or Kana and the punctuation. Keep the
  English ellipsis style `…` (one character).
- **Machine values stay as they are.** Never translate argument values, keys, theme
  names, route names, protocol fields, Git terms that are commands (`git push`) or product
  names (`rbit`, `GitHub`, `Git LFS`).
- **Large families.** A file that would exceed 400 lines (the configuration schema,
  `ConfigText`, matched on its `section`/`key` strings) may move arms into a sibling
  module: the arm calls a function there, e.g.
  `ConfigText::Label { section, key } => return fields::label(section, key, text),`
  with `fn label(..) -> bool` returning `false` for keys it does not have.
- **New families.** A developer wiring a new family into the tables gives it a
  `translate` method calling `crate::catalog::translations::write`, then adds one empty
  line for it to every locale's file for that source module (the compiler names each
  missing one). A family without that method keeps compiling with English alone.

`node scripts/i18n-coverage.mjs` prints, per locale, how many messages are translated
and how many fall back; `node scripts/i18n-coverage.mjs de fr` adds per-family counts for
those locales (directory names). It is a progress report, never a gate. The original
exhaustive families are not table-backed and not counted: in the new locales they stay
English until they are converted to `MessageFamily`.

`cargo test -p rbit-localization` checks a test-only table (`catalog/translations/tests.rs`):
an entry is used and not a fallback, a missing entry falls back, and plural arms follow
the locale.

## First-slice integration and future catalogs

The HTTP adapter should pass Catalog/Locale to presentation; set both HTML `lang` and
`dir`, use locale as a render/cache dimension, keep private responses private, and
vary header-negotiated responses appropriately. Explicit locale changes must refresh
the relevant full/fragment/history state. Domain errors and protocol fields remain
stable codes with arguments. Do not persist only localized output as event history.

The compiled Spanish activation does not resolve the planned comparative catalog gate. Evaluate a Fluent-backed adapter behind the
same typed `Message` contract. [Fluent selectors](https://projectfluent.org/fluent/guide/selectors.html)
support grammatical forms beyond English. Load/validate catalogs once at startup;
require message/argument completeness and deterministic English fallback; benchmark
binary growth, resident catalog bytes, cold/warm formatting, allocations and concurrent
access against this compiled implementation. The initial choice adds zero production
dependencies and avoids speculative language-engine overhead; it does not claim to
have benchmarked Fluent. Complete catalog rollout additionally requires locale-aware dates/numbers, reviewed
grammar, fallback policy and RTL/accessibility acceptance beyond this partial activation.

## Web adapter status

`rbit_web::localization::RequestLocale` resolves and memoizes one locale in request
extensions after the existing session lookup. Production supports every language of
`Locale::LANGUAGES`, with an English default. Saved canonical account preference comes from the existing
login-row session projection. Native `/settings/language` reads that extension and
persists the choice through the preference service. Its picker lists every language by
its native name (`Locale::native_name`), each option marked with its own `lang`. The
account choice is stored in `user_logins.interface_locale` (migration 0072), which takes
every offered tag; `user_logins.locale` (0062, English and Spanish only) keeps mirroring
those two, so an older build still reads a valid choice. Diagnostics require an explicit
trusted resolver extension and remain selected-feature opt-ins.

The shared Page extractor assigns the document locale, while legacy content stays
explicitly English. Localized feature constructors select their own content locale.
The shared HTML response boundary adds language/catalog/cache dimensions before
partial slicing and ETags. Raw/download/sandbox/immutable-fragment exclusions,
native settings navigation and catalog-scoped browser history are specified in
[account activation](account-locale-activation.md). This is intentionally partial
translation coverage, not a claim that all feature text has been translated.

## Explore production slice

Explore continues to use the existing public-only repository search service and stable
query values/URLs for search, sort, archive filters and paging. Feature-owned typed
messages cover its title, description, controls, result count, empty states, archive
badge, star label and pagination. Repository names/descriptions remain user content.
Untranslated feature regions are still English; relative times follow the page's
language ([localized times and numbers](localized-time.md)).

Full documents include `home/explore_results.html`. A live search with exactly one
`HX-Request: true` and `HX-Target: explore-results` renders that same component
directly, preceded by its updated document title. It does not render the base layout
and cut HTML afterward. Requests carrying boosted/history-restore headers always get
the full document, as do ordinary GETs and ambiguous/repeated shape headers. The
form remains an ordinary GET form without JavaScript and retains HTMX push-URL and
select/swap behavior. Both shapes report Content-Language and vary on language,
cookies and response-shape headers; existing private revalidation remains in force.

The original Explore-only slice had no locale mutation UI. Public account activation
now supplies the native `/settings/language` form and catalog-scoped history/navigation
rules described above; diagnostic resolver injection remains a separate test seam.
HTTP regressions cover component equality, locale/direction consistency, full history
fallback, private repositories even for their signed-in owner, malicious interpolation,
shape-header ambiguity and existing URL/filter/pagination behavior.


## Deferred Explore text

`Catalog::display(Message)` returns plain borrowed `LocalizedDisplay`, with the same
ordinary template escaping boundary as `LocalizedText`. English and Spanish ExploreSubtitle,
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
`Catalog::actions_display` and borrowed `ActionsDisplay`. English/Spanish dynamic labels write
shared catalog literals and existing job names directly to the destination. Diagnostic
locales delegate to the existing owned Actions catalog without changing its output.
RunPage methods return borrowed labels only at render time, with ordinary template
escaping and no HTML-safety bypass. Existing trust/deployment prose remains English. This removes introduced per-label owned
formatting buffers at the source level; no allocator-count or latency claim is made.


### Borrowed Explore count labels

Explore now retains the integer total and borrows its existing query only while the
shared results template formats the count. English and Spanish `CountForQuery`
display writes directly to the destination, including the empty-query case; it no
longer builds an intermediate count String containing a second copy of the query.
The existing owned `Catalog::text` API remains the parity oracle and remains available.
Diagnostic catalogs retain that original transformation path. Ordinary Askama text
and attribute escaping applies to the entire borrowed display value.

This is a source-level removed copy/intermediate-buffer claim, not measured latency,
CPU, allocation-count or whole-request performance acceptance. Real Rust/HTTP parity
and current optimized paired page measurements remain necessary.
## Public response boundaries and queued candidates

The published checkpoint is `fb9f21dd`. Production `en`/`es` negotiation
and native account language saving/resetting are implemented. `en-XA`/`ar-XB` require
trusted diagnostic enablement and selected feature adapters; incoming arbitrary
query/header text cannot enable them globally. Supported full pages and selected
Actions roots report matching language/direction, with catalog identity applied
before private revalidation. Machine filter values, route names, refs, theme slugs,
log bytes, user content and stored protocol/error codes are never translated.

Generic HTML errors are not raw responses: they use the shared real-document locale
and catalog metadata. A pseudo-locale Actions request that falls through to a default
HTML 404 has English catalog metadata; ordinary Spanish negotiation can retain a
Spanish shell around the explicitly English legacy error body. Raw/API/static bodies,
sandboxed user content, downloads and language-neutral immutable fragments retain
their exclusions. Native redirects remain redirects and do not become localized HTML.
The successful-GET catalog-mismatch refresh does not apply to errors or raw responses.

Two candidates queued at that checkpoint are included in this integration source;
their Rust, HTTP and browser acceptance remains pending:

- `6dcba78f` plus browser candidate `00bf1acd` localize the shared theme picker in
  English/Spanish, including accessible search and Colorblind controls and dynamic
  counts. Theme identities and machine filter values stay unchanged. At the checkpoint
  picker content was still English; Appearance settings and preview example code are not
  claimed fully translated. Real Rust/HTTP and isolated hosted browser gates remain.
- `162fee0a` changes Explore CountForQuery from its current owned count String to a
  borrowed template-boundary display for English/Spanish, with unchanged owned API
  and diagnostic fallback. It removes the intermediate copied query at source level;
  Rust/full-fragment escaping parity and current performance gates remain unrun for
  this candidate. No measured allocation-count or latency improvement is claimed.

Spanish linguistic review, remaining legacy labels/dates, current production browser
accessibility/history/no-JavaScript proof and matched performance measurements remain
separate acceptance work. Historical measurement records and prototype captures keep
their original source scope; neither publication nor this wording upgrades that evidence.
