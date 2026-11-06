# Offline credential parameter inventory

This optional operator tool observes metadata from an explicitly supplied **offline,
trusted SQLite copy**. It changes no password policy, verifies no passwords, allocates
no Argon working memory, and never opens application configuration, redb, migrations,
or sealing keys. It is evidence for a later reviewed compatibility policy, not enforcement.

After building through the repository's shared compiler lane, run the binary:

```text
rbit-credential-inventory --snapshot OFFLINE_COPY.sqlite
```

There is no default, discovery, environment override, live-database mode, or output
path option. Keep the source server stopped while obtaining a copy, or use an existing
consistent SQLite backup mechanism. Move the finished copy to an operator-controlled
location and keep it unchanged and inaccessible to concurrent writers throughout the
scan. Do not point this tool at an active installation, an uncheckpointed file copied
from one, or an untrusted uploaded SQLite database. It cannot establish that an
operator's copy was originally consistent or defend against hostile concurrent changes.

The scanner uses the pinned SQLx0.9 `SqliteConnectOptions` filename API with
`read_only(true)`, `immutable(true)` and `create_if_missing(false)`. Immutable mode
intentionally disables SQLite locking/change detection; read-only mode alone is
insufficient to avoid WAL/SHM creation. No journal-mode changes or optimization on
close are enabled. Query-only/trusted-schema-off, a 2 MiB page-cache target, no mmap,
one-row fetch buffer, and no sorting bound the intended metadata query. Only a real
`user_logins` table is accepted; a view is refused. The snapshot must use UTF-8
(`PRAGMA encoding`); UTF-16le/be are refused with the fixed schema error and no
report. SQLite BLOB casts and octet_length use the database encoding, so treating
UTF-16 bytes as UTF-8 could incorrectly classify a valid PHC. The scanner does not
convert or rewrite the input database. This tool-input restriction changes no
application credential acceptance or stored hash policy.

The tool rejects any `-wal`, `-shm`, or `-journal` sidecar, including empty files/links,
before and after scanning. Input/ancestor links (including Windows reparse points)
are refused. It compares full-file SHA256 and size/timestamps before and after,
with checks during each bounded 64 KiB streaming hash; fingerprints and paths never
enter the report. Files over 8 GiB are refused. These checks detect observed drift;
they are not a lock, an atomic cross-file snapshot, or proof against a writer changing
and restoring data between observations. Reads may update filesystem access times;
the tool performs no snapshot or sidecar writes.

The fixed production limits are 1,000,000 counted rows plus one lookahead row,
1024 credential bytes per parsed field, and 256 distinct parameter groups. The pure
aggregator supports only validated bounded test/operator limits, never unbounded
sizes. SQLite 3.51.3's `octet_length(column)` reads record length without loading a
large TEXT/BLOB; CASE returns no credential bytes at all for oversized/non-TEXT
fields. At most one bounded credential value is transferred per streamed row; Rust
copies and file-hash buffers are zeroized, but this is not a guarantee that SQLite,
its page cache, PHC parser stack values or the operating system erase all copies.

JSON contains only schema/completeness flags, tool limits, row/classification counts,
PHC byte-length ranges, decoded salt/digest length ranges, and recognized Argon
algorithm/version/effective m/t/p group counts. It contains no PHC strings, salts,
digests, account IDs/names/emails, database fingerprints, SQL errors or paths.
Even these aggregate counts may be sensitive operational metadata: review locally
and do not automatically upload or publish them. Standard error uses fixed codes.

Classifications are parameters-observed, incomplete (missing salt/digest or truncated
adapter input), malformed, unsupported algorithm/version, oversized and invalid
storage. The pinned PHC/Argon **parameter parsers only** determine recognized metadata;
no hash/verify API is called. Supported costs include defaults when parameters are
omitted; absent version stays null (the pinned verifier defaults to 19). Accepted
keyid/data parameters are parsed but their contents are never reported or grouped.
Parameters-observed is not proof that any password verifies or that its resource cost
is safe. Oversized/unsupported input cannot justify a policy cap or deletion.

`complete` means every row fit the row/group inventory bounds, not that every PHC
was valid or supported. Exit 0 emits a complete inventory; exit 3 emits an explicitly
incomplete inventory after a row/group limit; exit 1 emits a fixed refusal code and
no report. Review all classification counts, not only parameter groups. Group omission
counts count affected rows, not estimated unseen distinct groups. Increasing approved
inventory bounds requires an explicit source/configuration review; no hash-cost cap
or compatibility change is introduced here.

## Build and CI feature scope

Build or test this optional tool as an explicitly selected package through the
shared compiler helper, for example:

```text
node scripts/workspace.mjs test --package rbit-credential-inventory --no-default-features --locked
node scripts/workspace.mjs build --package rbit-credential-inventory --no-default-features --release --locked
```

Its isolated normal graph requests identity's `credentials` feature, not
`password-work` or `sessions`; the standalone package graph excludes the kernel
and application stores/services. The existing read-only Cargo-tree guard checks
that graph. Tree inspection is not compilation: CI also compiles and tests the
tool in its own package invocation on Linux and Windows, separately from the
workspace application feature union. The source guard requires that explicit
locked invocation without additional application packages/features or skip rules.

A full workspace test or metadata invocation can unify identity's password-work
and session features through other selected application packages. Its normal
closure then includes `rbit-credential-inventory -> rbit-identity -> rbit-kernel`.
This is expected Cargo feature
unification, not evidence that the tool constructs a password-work owner or starts
application services. The scanner's source uses only identity inventory types and
its explicitly supplied immutable SQLite connection. Do not claim a kernel-free
full-workspace closure from the isolated check, or use workspace-only compilation
as proof that accidental application-only imports would fail in the standalone
release-package scope. The isolated package test prevents that masking gap.

Acceptance (through the shared build helper/lane; never create per-worktree targets):

```text
cargo test -p rbit-identity --no-default-features --features credentials --test credential_inventory
cargo test -p rbit-credential-inventory
cargo clippy -p rbit-identity -p rbit-credential-inventory --all-targets -- -D warnings
```

Also run the existing models-only/token-only graph and feature fixtures. The new
identity module/test are credentials-gated; storage remains models-only. Tests use
closed synthetic fixtures only: exact published legacy vector, extreme numeric costs
without work, malformed/unsupported/incomplete/oversized input, aggregate secrecy,
row/group bounds, non-TEXT storage, absent-file/view refusal, pre/post sidecars,
content drift, CLI secrecy and unchanged directory contents/database bytes/mtime.
No real database scan or runtime acceptance is claimed by source preparation.

Primary implementation references:
[SQLite immutable URI](https://www.sqlite.org/uri.html#uriimmutable),
[SQLite octet_length](https://www.sqlite.org/lang_corefunc.html#octet_length),
[pinned Argon Params parser](https://github.com/RustCrypto/password-hashes/blob/argon2-v0.6.0/argon2/src/params.rs).
