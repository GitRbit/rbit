# Wiki domain contracts: W0 source preparation

This slice prepares pure `rbit-wikis` models and command policy. It does not implement
or validate a wiki persistence adapter, mutation transaction, HTTP/API route, renderer,
UI, cache, search, media, worker or production in-memory backend. W0 and the overall
wiki feature remain incomplete until their contract/acceptance gates pass.

## Dependencies and ownership

The crate depends on shared IDs/timestamps, pure RepoAccess and the already locked
unicode-normalization 0.1.25 release. Its published source/API was checked locally:
NFC and is_combining_mark are used; license MIT/Apache-2.0; its only normal package
edge is existing tinyvec with alloc. No new registry package, runtime, crypto, HTTP,
Git or SQL dependency is introduced. Actual full-feature Cargo trees, compilation,
binary/build footprint and strict AST/Clippy acceptance are pending registration.

The module owners are stable identity/generation, fixed-size Git revision identity,
logical path/title validation, finalized page projection and command policy. There is
no unused I/O trait: introduce a named WikiCatalog/WikiRevisionStore at the first
real adapter boundary rather than guessing a generic repository framework here.

## Name policy version 1

A page route is a logical, slash-separated, NFC-normalized, CASE-SENSITIVE UTF-8 name.
Guide and guide are distinct; canonically equivalent Cafe + combining acute and the
composed form share one key. No NFKC, transliteration, implicit lowercasing or whole
Unicode confusable protection is claimed. The future SQLite namespace must use the
same NFC bytes with binary uniqueness, never its default ASCII NOCASE repository-name
predicate. Stable page IDs, not title/path strings, identify records and disk objects.

| Input | W0 contract |
| --- | --- |
| Raw path | At most 1024 UTF-8 bytes, checked before copying/normalization |
| Canonical path | At most 512 UTF-8 bytes; at most 8 slash-separated components |
| Component | At most 64 UTF-8 bytes; starts with an alphanumeric scalar; letters/numbers/combining marks, hyphen/underscore and interior dot |
| Path rejection | Empty/leading/trailing/repeated separators, backslash/colon/query/fragment/percent/whitespace, leading/trailing dots, double dots, .git/.lock suffixes, Windows devices including extensions/superscript forms, known direction/invisible controls and variation selectors |
| Raw title | At most 1024 UTF-8 bytes, before normalization; controls/directional overrides rejected before trimming |
| Canonical title | Trim surrounding Unicode whitespace, normalize NFC, at most 200 Unicode scalars; punctuation, markup and script/emoji joiners remain plain text |

The device-name/extension/superscript defense follows [Microsoft's naming rules](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file).
COM0/LPT0 are additionally refused as a conservative application rule, not a claim
that Microsoft lists them as reserved. None of this turns a logical path into a safe
OS pathname or removes the need for the actual adapter's containment checks.

The bounds are initial explicit name contracts, not measured performance SLOs or a
claim of grapheme counting. Their complete tests must pass before these bytes become
a durable format; later persisted changes require a reviewed format/migration policy.

WikiPath accepts ALREADY-DECODED input and rejects residual percent. It does not decode
URLs or prove filesystem containment. The future HTTP boundary must inspect raw URL
encoding, reject encoded slash/backslash and malformed escapes, perform exactly one
strict UTF-8 decoding step, then construct the domain path. No middleware/route may
reinterpret it a second time. Paths may never be joined directly into filesystem
storage: use the repository/page IDs and a checked adapter-owned root. Symlink/reparse
protection belongs to that real storage adapter. Validation does not make a title/path
safe HTML; Askama escaping and reviewed Markdown/media policy remain mandatory.

## Identity and optimistic mutation facts

WikiPageId is a positive signed SQLite identity. Allocation, durable nonreuse and
repository ownership remain catalog obligations, not constructor guarantees. Page
rename/title changes retain that identity. WikiRevisionId holds full nonzero lowercase
SHA-1/SHA-256 object bytes; it accepts neither abbreviated IDs nor Git expressions or
options. Syntax is not proof of object type, reachability, existence or authorization.

WikiHead pairs a positive repository-wide generation with the visible whole-wiki Git
commit ID. This is not an independently mutable page ref: any accepted edit, rename,
restore/tombstone or catalog change advances the visible generation. WikiPage is a
finalized projection at that head and preserves its original expected edit identity.
There is no helper to re-read a new head and silently restamp stale user input.

Create carries an exact expected optional head; None means genuinely NEVER INITIALIZED,
not any current head. The target path/alias slot must still be empty. Change carries
repository, stable page ID and exact expected head. After initialization, catalog
retains generation/head across whole-wiki tombstone/reinitialization: resetting to
None or reusing page IDs would create ABA and is prohibited before an adapter ships.
A deleted/recreated repository also needs an explicit identity/incarnation contract
at the persistence boundary. These are required SQL/recovery gates, not implemented
by pure types. The prospective next generation refuses i64 exhaustion instead of
wrapping/resetting; every returned value is provisional until durable publication.

Current trusted repository facts drive policy in a fixed order:

1. Disabled/unreadable or foreign-repository wiki is NotFound for reads, including
   conditional-response/cache use.
2. Content mutation requires an authenticated actor and current Write/Admin access.
3. Archive rejects all content mutation, including administrator restore/rename/delete.
4. Expected/current repository scopes must match the authorized repository.
5. A pending mutation blocks new publication.
6. Exact expected head/generation/OID, page identity and creation-slot emptiness match.
7. Allocate only the prospective bounded next generation, with explicit exhaustion.

Page create/edit/rename/restore/tombstone share that writer rule. Whole-wiki settings
are separate admin commands; this content policy does not expose an enable/delete-all
shortcut. Anonymous public reads remain supported, and archived reads remain allowed.
Errors are finite variants for localized presentation and do not echo page content.

These are data checks, NOT opaque one-use admission, fresh authentication, durable
CAS or a store-instance/realm proof. A real command must obtain current ACL/archive
facts and enforce corresponding conditions, namespace uniqueness and SQL/Git CAS in
the publication protocol. Calling the pure function, storing its output or checking
an old cached DTO is insufficient. There is no new COMMIT-cancellation guarantee.

## Prepared acceptance and next concrete runtime gap

Fourteen Rust fixtures prepare NFC/case/script support, percent/double-decoding input,
traversal/separators/Windows devices/invisible controls, title markup-as-text, byte/
scalar/depth limits, full SHA algorithms, stable identity, current read/write/archive
precedence, repository/page/head mismatches, pending work and exhaustion behavior.
Rust tests, strict Clippy, AST and unified dependency graphs are UNRUN in this source
checkout; parent composition owns workspace/lock registration and the shared lane.

Registration needs member crates/rbit-wikis plus its workspace path alias. Cargo.lock
adds rbit-wikis 0.1.5 with rbit-access, rbit-contracts and the existing exact
unicode-normalization 0.1.25 package. The architecture policy permits only those two
internal edges; no global service/core dependency is introduced.

The next genuine W1 slice must select and implement SQLite catalog allocation/binary
namespace/visible-head/tombstone authority and a native Git revision reader behind
actual static ports, then produce an authorized finalized-page read through real
Askama/HTTP. Migration 65 is reserved for review after the credential 63/64 sequence;
this slice adds no migration or schema. Do not fabricate pages in a production memory
store or expose a write form before the fixed-intent/CAS/crash protocol is proven.

Before W1 ships, complete finite content/media/search/cache/admission budgets,
raw-URL decoding/HTTP cache/XSS fixtures, current ACL/archive races, page and wiki
incarnation/nonreuse, real catalog/Git consistency/recovery, bootstrap/disabled flow
and shared build/graph measurements. The comprehensive W0-W5 plan remains authority;
this source preparation neither completes W0 nor makes the local wiki usable yet.
