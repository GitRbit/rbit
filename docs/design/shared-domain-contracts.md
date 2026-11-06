# Shared domain contract extraction

The account/session application boundary needs shared timestamp and name-error
contracts without importing legacy core. This preparatory change moves only those
proven cross-feature contracts and the existing slug grammar into `rbit-contracts`.
It moves no accounts, repository entities, settings, sessions, storage or application
workflows yet. Existing production callers use the new authority through exact core
reexports; they do not convert or duplicate values.

`Timestamp(pub i64)` preserves its public tuple constructor, signed Unix-second
representation, derives, methods and Display behavior. Root and module paths in
`rbit-core` reexport the same type. JSON remains an integer and postcard remains the
same signed-i64 newtype encoding, including negative values. `now` still reads the
UTC clock. ISO/date formatting and out-of-range epoch fallback remain unchanged.
English relative/until/Display helpers move intact solely for existing source
compatibility; this does not establish localized date formatting or injected clocks.
Arithmetic behavior is unchanged, including extreme-value limitations. Correcting
those semantics requires a separate reviewed change and acceptance tests.

`NameError` preserves all seven variants, field types, derives and display strings.
It remains shared by username, email, repository/team names and issue-number
validation. No new serialization representation is introduced for the error enum.
Both legacy core paths remain the same public error type.

`names::validate_slug` is the single existing grammar used by account and repository
names. It keeps byte-based length limits and exact error precedence: empty, length,
ASCII character set, alphanumeric boundaries, then reserved names/Git suffixes/
double dots. It does not trim or lowercase accepted values. The reserved-name comparison
retains its existing temporary lowercase allocation. Domain wrappers retain
their private fields and constructors; account/repository route reservations stay
with their owning feature. TeamName's distinct grammar and all email rules remain
unchanged. This function is not an authorization or uniqueness decision.

The contracts crate gains existing workspace `time` and `thiserror` dependencies;
postcard is test-only to check persisted byte compatibility. There is no new external
package version, async runtime, crypto dependency, task, cache or global registry.
This dependency change does not claim a build-time or runtime improvement.

Validation prepared here retains existing deterministic timestamp/name tests and
adds fixed signed wire vectors, integer JSON roundtrips, invalid-time fallback,
exact error/precedence cases and module/root TypeId/constructor compatibility.
The author ran source-body comparison, formatting and diff checks only. Compiler
execution waits for root lockfile registration and the shared build lane:

```sh
node scripts/workspace.mjs test --package rbit-contracts --package rbit-core --locked
```

Identity model relocation and account/session repository ports remain subsequent
work; this commit does not complete either feature or phase2.
