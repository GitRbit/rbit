# Security hardening review

This review covers identity and authorization, browser content and history, and
network and repository object boundaries. It includes malformed inputs, protocol
interpretation differences, persistent credentials, and alternate object lookup.
It is a focused review, not a guarantee that the application has no vulnerabilities.

## Changes

* Import Git and LFS credentials follow the configured API or web **origin**:
  scheme, hostname and effective port must all match. An import response cannot
  authorize sending its token to a different service on the same hostname.
* Authenticated pages opt out of htmx's serialized DOM history. The marker travels
  with partial page updates. Navigation startup removes snapshots left by older
  clients; anonymous pages retain normal history caching.
* Signed OIDC tokens must have correctly typed audience and not-before claims.
  Clock-skew comparisons use widened arithmetic, including at integer extremes.
* Client pushes cannot create or change Git replacement refs. Readers and browser
  edits ignore existing replacements, including when repository configuration
  enables them. Authorized writers can delete replacement refs to clean them up.
* Markdown has an independent document-wide budget of 4096 raw HTML `<` bytes.
  A separate budget permits at most 64 possible raw active-formatting starts, so
  extra raw markup after formatting starts is limited to 4 KiB. Raw table/template
  contexts and raw formatting are never admitted together: later conflicting
  events become text. This prevents HTML5 foster parenting from copying even one
  large formatting attribute across generated paragraphs. Native Markdown tables,
  links and formatting keep working.
  Raw events beyond either budget are escaped. This bounds HTML parser work even
  when HTML5 error recovery disagrees with the inexpensive depth estimate. Normal
  Markdown stays structured; large raw HTML documents may show excess markup as
  text. A 500-row raw HTML table remains formatted.

Full-suite validation also exposed a wiki writer-lock lifetime race. A completed
owner now explicitly unlocks its file before closing it, so an unrelated duplicated
or briefly inherited descriptor cannot retain the lock and reject the next operation.
Active native work and retained wiki scopes still hold their original ownership.

## Validation

Regression coverage includes cross-origin import metadata, signed malformed OIDC
claims, actual gitoxide replacement behavior, pinned and reopened readers, edits
with substituted trees, rendered authenticated history markers, and hostile raw
HTML syntax. Browser history was also exercised with real htmx in headless Chrome.

Markdown checks cover repeated self-closing elements, unmatched end tags, comment
and attribute quirks, malformed delimiters, and the maximum retained raw budget
followed by 10,000 Markdown paragraphs. A local optimized harness measured 100
README renders at 29.56 ms before and 29.24 ms after, and 100 renders of a 500-row
raw table at 76.62 ms before and 79.54 ms after (best of three; illustrative, not a
cross-machine performance threshold). The retained-budget regression rendered in
approximately 113 ms in the local test build.

Validation runs against a private local rbit instance, including browser checks,
workspace Rust tests, clippy, an optimized server build and isolated readiness.
This PR changes no CI/CD configuration.

## Scope relative to PR #69

Scoped tokens, agent endpoints, merge queues, runner lifecycle fixes, and the
dependency advisory already addressed by PR #69 are excluded. No dependency or
lockfile changes are needed. The existing object-open options in `reader.rs` are
hardened independently of that PR's added reader path accessor; the hunks do not
overlap. CI/CD changes remain entirely with PR #69.

Reauthentication before adding a new authentication factor remains a separate
policy question, especially for accounts that sign in exclusively through an
external provider. This review did not establish an account-takeover path there.
