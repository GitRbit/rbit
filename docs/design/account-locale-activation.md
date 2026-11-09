# Account language activation

English remains the default. Spanish is the first real additional compiled catalog,
covering the shared shell, language settings and the existing localized Explore and
Actions presentation. This does not claim a complete Spanish translation of every
feature. Legacy page bodies, settings navigation, protocol/event labels and relative
dates retain explicit English `lang`/LTR boundaries inside the Spanish document.
Diagnostic expanded/RTL catalogs remain trusted opt-ins for selected feature routes.

## Preference and authenticated request work

After the credential writer protocol in migration 0061, migration
`0062_user_locale.sql` adds a nullable, checked `en`/`es` column to `user_logins`. Null means browser/default. This existing per-user row is already read
by both session projections; an integer `CASE` maps its tag to a closed model enum
without allocating a tag string, adding a join or adding a preference query on each
authenticated request. This choice avoids a new preferences-table B-tree lookup.
Theme remains in `user_preferences`. Language updates change no password, epoch,
pending-revocation or theme field. Login PHC/epoch snapshot predicates stay unchanged.

The preference store and service immediately serve the native signed-in
`/settings/language` form. Only canonical saved `en`/`es` values or empty browser/default
are accepted. Saved choice wins over anonymous cookie and browser negotiation. A
reset clears the old anonymous preference cookie as well as the account selection.
The form performs a fixed full navigation; it accepts no redirect destination.
Ordinary CSRF, form-size, authentication and password-change restrictions still apply.

## Visitors and the header's language dialog

Visitors who are not signed in use the same form. `GET /settings/language` renders it
for them without the settings navigation, and `POST /settings/language`, with the same
Origin check, stores their choice in the `rbit-locale` cookie instead of an account:
the canonical tag only (`ja`, `pt-BR`), `Path=/`, `HttpOnly`, `SameSite=Lax`, `Secure`
over HTTPS, for one year. It identifies nobody, is never readable by scripts and is
never sent to other sites. Negotiation reads it after the account choice and before
Accept-Language; an unsupported, repeated or oversized value falls through. Choosing
the browser default removes it, and a signed-in save removes it too, so the account
choice alone decides.

The header's globe button sits before the theme buttons in reading order (so it
mirrors with them right to left). Without scripts it is a link to that page; with
scripts it opens a modal dialog fetched from `GET /settings/language/dialog`, listing
every language by its native name with its own `lang`, the current one marked
`aria-current`. A row posts the page's form (the redirect is not followed), then the
page reloads once. The reloaded document carries the new catalog key, which the
mismatch logic below records without reloading again. A refused save opens the page
itself, where the form reports the problem.

Upgrade/constraint/roundtrip tests use actual SQLite. Session tests count one real
acquisition and one statement; EXPLAIN tests additionally require the existing login
primary-key lookup with no new preference join. These are source-level regression
tests until run in the shared compiler lane, not measured latency claims.

## HTML, partials and browser history

The session adapter supplies account preference before one memoized request locale
resolution. The shared Page extractor selects the document locale while defaulting
legacy body content to English. Localized feature constructors explicitly select
their content locale too. The HTML pipeline annotates successful pages and generic HTML errors before partial
slicing and validator generation. Error prose retains the legacy English content
boundary; a diagnostic feature request that becomes a default HTML error falls back
to the real document catalog rather than enabling pseudo error rendering. Raw/API/static, sandboxed user content,
downloads and language-neutral immutable commit-pinned fragments are excluded.

HTML reports `Content-Language` and a bounded versioned `Rbit-Catalog-Key`. Validators
hash that key and the existing body segments excluding render-time text; locale is
therefore a validator dimension even when a legacy English fragment has equal bytes.
Language, cookie and HTMX response shape remain Vary dimensions, with
`Rbit-Document-Locale` identifying the client's current catalog. That request header
never overrides preference selection. Existing private/no-cache and bounded preload
freshness remain in force. No Clear-Site-Data or asset-cache purge is introduced.

A successful ordinary HTMX HTML GET whose catalog differs from the requesting
document gets a full-refresh instruction and transitional no-store. This instruction
does not apply to preload, raw/API/error or same-catalog responses. Client safeguards
also prevent a cross-catalog HTML swap. Native navigation creates fresh in-memory
state. The bounded repository HTML response cache includes catalog identity and
retains language metadata. Only HTMX's own HTML history cache is removed on a changed
catalog namespace; unrelated local storage, HTTP caches and service workers remain
untouched. Body-history and BFCache mismatch checks reload the current URL once.

## Catalog and acceptance boundaries

There is no runtime message parser or new dependency. Static English/Spanish labels
borrow compiled literals. Common dynamic labels hold borrowed arguments and format
directly into the caller's writer, preserving ordinary Askama escaping. Typed enum
matches require a catalog entry for every currently supported message variant.
Owned/display parity fixtures cover hostile Unicode and HTML arguments.

Spanish integer noun messages use singular for one and plural otherwise (absolute
one for signed star counts). The nouns used here share plural spelling across CLDR's
Spanish many/other categories; see the [CLDR Spanish plural rules](https://www.unicode.org/cldr/charts/49/supplemental/language_plural_rules.html#es).
This is no general grammatical selector or date/number localization engine. Existing
machine query values, protocol values and user content are never translated.

Before broader catalog rollout, measure startup/resident memory, binary growth,
formatting/allocations and concurrent access against the planned startup-loaded
Fluent adapter using the same typed contract. That comparison is not yet performed.
Spanish linguistic review, real HTTP/304/private-cache tests, and isolated browser
history/back/HTMX/no-JavaScript acceptance remain required validation; Node fixtures
alone cannot establish actual browser behavior or performance.
