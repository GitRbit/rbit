# Dependency maintenance for 0.2.2

The resolved registry lock graph updates serde-saphyr to 2.1.1 (granit-parser 2.1.0),
unicode-normalization to 0.1.26, russh to 0.65.1 and redb to 4.5.0. Rustix 1.1.6
was already included in the preceding hardening work. No new redb concurrency
mode is enabled: its default exclusive writer remains the ownership model.
The generator, generic-array, iana-time-zone and tempfile lockfile edge changes
come from the actual russh Dependabot patch; no additional package updates were
introduced while resolving the format adapter.

## Persistent wiki normalization

Wiki catalog names have a **Unicode 17 NFC** format contract. Constructors and
checked decoders use the same normalizer and combining-mark table. They preserve
the exact bytes of current paths, title metadata, history, aliases, tombstones and
accepted mutation intents. A dependency update does not rename or rekey them.

Registry unicode-normalization 0.1.26 uses Unicode 18. For example, the previously
valid title `a\u{1ADE}\u{05AE}b` changes to `a\u{05AE}\u{1ADE}b` under that version;
silently adopting it would either reject valid catalog rows or change identities.
The wiki format therefore uses the explicit `unicode-normalization-v17` adapter
dependency. Wiki normalization intentionally does **not** gain Unicode 18 semantics.
The lock graph retains registry 0.1.26 through stringprep and optional SQLx
Postgres dependencies. `cargo tree` finds no active Unicode 18 consumer in the
default server or workspace graph, including `--target all`; the production
wiki and gitoxide paths share the pinned Unicode 17 implementation.

The compatibility source is the official upstream repository
[unicode-rs/unicode-normalization](https://github.com/unicode-rs/unicode-normalization),
pinned to the full published 0.1.25 revision
[`5a69b3bafb625caccdec7871a42bed0d9a6604d1`](https://github.com/unicode-rs/unicode-normalization/commit/5a69b3bafb625caccdec7871a42bed0d9a6604d1).
That revision matches the crate's `.cargo_vcs_info.json`; all 12 Rust source files,
licenses, copyright, README and original manifest were compared byte for byte.
The manifest checks version 0.1.25, and the adapter asserts Unicode version 17.
There is no custom Unicode algorithm or copied generated table in rbit.

## Git precomposition compatibility

Gitoxide's `core.precomposeUnicode` behavior also uses normalization tables. The
valid packed refs `α\u{1ADE}\u{0345}` and `ᾳ\u{1ADE}` must remain distinct: Unicode 18
would compose across the newly assigned mark and resolve the first to the second.
Disabling precomposition would change existing macOS loose-ref spelling behavior,
and gitoxide reads this setting before API config overrides are applied.

A scoped `[patch.crates-io]` redirects only gix-utils 0.5.2 to its published source
under `vendor/rust/gix-utils`, with its normalization dependency pointing to the
same exact Git17 source. All 13 Rust source files and both licenses are unchanged;
original hashes, original manifest and upstream VCS identity are retained there.
No Unicode algorithm or gitoxide precomposition algorithm was rewritten.
The third-party package is excluded from first-party workspace membership; source
proof snapshots still hash it as a local dependency, and cargo-deny checks it.

This adds a second resolved source identity, a pinned Git fetch on clean builds
and about 65 kB of upstream compatibility package source and provenance. Wiki calls still
perform the same normalization work as before. In an optimized, stripped macOS
standalone normalization probe, retaining
both implementations added about 129 KiB; that is an illustration of dual-version
linking, not the current production graph or a full-server binary measurement.
Calling Unicode 17 alone versus selecting it in the dual-version
probe took about 298–299 ms for 100,000 decomposed 360-byte strings (warm
runs); the adapter adds no normalization pass. Ordinary registry aliases cannot
resolve exact 0.1.25 and 0.1.26 together; the separate official source permits the current graph and stable format to coexist.
Future implementation fixes still require advisory review and byte-identity tests.
Changing the wiki format itself requires an explicit versioned migration with
collision checks across all retained identities and replay records.

## Validation and source policy

`deny.toml` permits only this exact Git repository, requires revision pins and
rejects other Git sources. It adds no organization-wide or host-wide exception.
Run `cargo deny check sources licenses advisories`; the existing CI invocation
selects only licenses and advisories, so the source check must also be run explicitly.
No workflow changes accompany this update.

Compatibility coverage includes workflow parser/conformance tests, SSH exchange
and shutdown tests, redb session/epoch/cache ownership and reopen tests, and real
wiki publication/history/delete/restore plus exact accepted-intent replay checks.
Actual ordinary/pinned RepositoryReader tests cover distinct packed identities,
NFC/NFD loose and packed refs, native Git agreement and existing macOS
precomposition/decomposed-query controls.
