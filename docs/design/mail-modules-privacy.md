# Mail rendering boundaries and diagnostic privacy

`rbit-mail` keeps its existing public `messages::*`, `Mail`, `MailMessage` and
transport paths. The message module now separates the rendered envelope, ordered
thread/list headers, account notices, individual thread activity and repository
notification digests. Askama template files and their paths are unchanged.

The pure extraction preserves 31 original production items and 13 existing test
or helper items as Rust tokens, excluding comments, imports, whitespace and
formatting-only trailing commas. The original rendering, escaping, subject,
link-extraction and header-order assertions remain in the rendering tests.
Transport/MIME tests remain alongside the original delivery implementation.

## Debug boundary

`Mail`, `MailHeaders`, password-reset/verification/address-change notices, thread
notifications and every digest nesting level have opaque Debug implementations.
They do not print recipient addresses, private subjects or bodies, usernames,
repository names, message IDs, reset URLs or unsubscribe tokens. Sender, SMTP and
memory transport Debug also avoid mailbox, relay, credentials and outbox contents.
Typed `MailError` Debug reports its variant without forwarding private field or
upstream error details. Both ordinary and pretty Debug use this policy.

Public fields, cloning, equality, rendering, delivery, error variants, error
Display and Error::source retain their original contracts. The private SMTP relay
string previously existed only for Debug; removing it also removes its setup-only
formatting allocation. No delivery or request path gains an allocation.

This is deliberately not a claim that all logging is redacted. `LogMailer` is an
explicit development transport that still logs the full recipient, subject, text,
headers and links. Sender Display, error Display/source, explicit outbox access
and serialized MIME also retain data. The foreign lettre Message Debug API is
outside this crate's opaque carrier contract; MIME values are not logged here.

Seven new Rust acceptance tests exercise actual reset/verification/address-change,
thread/digest rendering and MIME construction with private sentinels. They also
check header tokens, nested payloads, transport Debug, error Display/source and
an explicitly populated memory outbox. None sends SMTP or invokes the log
transport with the private fixtures. MIME assertions retain real content while
our Debug and pretty Debug assertions forbid it.

## Strict source gate and remaining validation

The shared strict selector now includes the whole mail crate. Historical physical
file exceptions no longer apply, and the normal native structural checker must
check all logical types and implementations in the crate. Every authored mail
file is at most 378 lines at this checkpoint. A focused policy fixture rejects a
401-line legacy mail file and selects the complete crate for cumulative checking.

Source bounds, formatting and focused Node policy tests pass. The new Rust tests,
all existing mail tests, strict all-target Clippy and the actual cumulative AST
gate remain pending under the shared owned compiler lane. Source preservation
and prepared tests do not substitute for those results; no SMTP/network,
throughput or latency result is claimed.
