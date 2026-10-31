# Language pack format gate

Schema 1 selects the pinned Fluent Rust parser and concurrent bundle (see the
workspace dependency versions). The accepted subset supports plain text, typed
text/finite numeric variables, exact numeric variants, named select variants and
locale cardinal plurals. Fluent's bidi isolation remains enabled. Text is never
trusted HTML: the host escapes the complete formatted result and creates markup
itself. No functions (including `NUMBER`/`DATETIME`), terms, message references,
attributes or nested inline placeables are accepted. Dates/times must be
host-formatted typed text arguments; full localized number/date formatting is
not delivered by this gate. Native Fluent number rendering must not be presented
as a complete CLDR number/date formatter. This restriction makes resolution a
bounded tree, with no reference graph, executable hooks or arbitrary functions.

The prototype tests interpolation, numeric zero/one/many, RTL, fallback,
replacement, concurrent readers and hostile catalogs. Parser/bundle APIs and the
concurrent memoizer are documented by the [Fluent project](https://docs.rs/fluent-bundle/0.16.0/fluent_bundle/)
and [AST](https://docs.rs/fluent-syntax/0.12.0/fluent_syntax/ast/).
Both Fluent crates are licensed Apache-2.0 OR MIT. Plural support is explicitly
checked against [`intl_pluralrules`](https://docs.rs/intl_pluralrules/7.0.2/intl_pluralrules/).

## Package

`.rbit-language` is a single-volume ZIP32 archive with no ZIP comment, extra
fields, directory entries, prepended stub or trailing data. Stored/deflated
regular non-executable files are the only supported entries. UTF-8 contents,
ASCII relative path components, consistent local/directory headers and
nonoverlapping entries are required. Links, duplicates, encrypted data,
unlisted data, executable modes and unsupported compression are rejected.
Nothing is extracted to disk by this crate. License contents are required UTF-8.

```toml
schema = 1
id = "rbit.language.fr"
version = "1.0.0"
locale = "fr"
direction = "ltr"
message_schema = "^1.0"
publisher = "Reviewed language publisher"
license = "MIT"
license_file = "LICENSE"
# parent = "fr" # Example: a fr-CA pack may explicitly declare fr.

[catalogs]
core = "catalogs/core.ftl"

[checksums]
"LICENSE" = "<lowercase SHA-256 of LICENSE>"
"catalogs/core.ftl" = "<lowercase SHA-256 of catalog>"
```

Publisher metadata and checksums establish neither publisher authenticity nor
approval. The host authorizes all installation/replacement of core catalogs as a
trusted administrator operation. Schema 1 only accepts host-known namespaces,
message IDs and argument names; translation installation cannot extend the
message schema. The `en` locale is reserved for host English; `en-XA` and `ar-XB`
may be installed as pseudolocales. `locale`/`parent` use canonical
`unic-langid::LanguageIdentifier` syntax (language, script, region and variants;
no Unicode extensions or private-use tags). Unknown plural-rule languages are
rejected. Direction is validated against the locale's script or language.

## Bounds and publication

The fixed limits are 2 MiB archive bytes, 8 MiB expanded data, 128 entries,
512 KiB per entry, 16 KiB manifest, 200:1 expansion plus 1 KiB slack per entry,
4,096 messages per pack, 128 AST nodes per message, eight levels of nesting,
16 select variants, 1,024 bytes per text argument and 8,192 output bytes.
Static output budgeting includes every select branch and worst-case arguments,
so it intentionally rejects some patterns whose individual branches would fit.
A set permits 64 packs, 64 MiB combined expanded data and 65,536 messages.

Locale/namespace conflicts and duplicate package IDs are rejected: installation
order never chooses a winner. Replacement supplies a new explicit set without
the replaced version. Catalog fallback is exact locale, explicitly declared
same-language parent (maximum eight links), then host English. Missing parents
and cycles are errors. Snapshot construction never implicitly adds a parent.
Preference negotiation separately performs bounded language lookup across
account preference, cookie, browser header (1,024 bytes/16 entries) and instance
default. English remains available without any installed pack.

`PackSet` and request `LocaleSnapshot`s are immutable and share validated catalogs
through `Arc`; reads perform no disk IO, SQL or HTTP. The host atomically
publishes a replacement and increments its revision. Previously issued snapshots
remain coherent until their owning requests finish; disabling the provider
selects English for new requests. Storage and host lifecycle policy are outside
this pure engine.

## Warm-format benchmark

Run `cargo test --release -p rbit-i18n benchmark_warm_100_message_page -- --ignored --nocapture`.
The ignored test warms one immutable snapshot, then formats 1,000 pages of 100
interpolated messages without disk access. It prints elapsed time and average
microseconds per page; no hardware-dependent latency threshold can fail CI.
A local unoptimized development run measured about 201 microseconds per
100-message page. This is an engine baseline, not an end-to-end rendering or
production latency guarantee; use the release command on deployment hardware.

The ZIP profile also rejects data descriptors, per-entry comments and orphan
local records: every listed local entry is contiguous from offset zero to the
central directory, whose records end exactly at EOCD. The parser guard follows
quoted expression literals and select-pattern text, rejects calls before Fluent
can recurse into arguments, and bounds nested placeables before AST allocation.
Browser q-values use the bounded RFC decimal grammar; q=0 exclusions survive
wildcard/default selection. Host English remains the final safe fallback when
no supported representation is acceptable.

The host reserves `core` message keys beginning with `browser-` for its browser
export channel. These patterns accept text and inline variables/string/number
literals, but reject all Fluent selects. Export formats a template with opaque
argument sentinels before JavaScript supplies real values; selecting a branch
at export time would silently choose grammar using those sentinels. Browser
plurals instead use the host's category-suffixed message keys selected by
`Intl.PluralRules` after the numeric value is available. Server-rendered messages
retain the bounded Fluent select/plural support described above.
