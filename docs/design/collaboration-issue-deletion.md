# Issue deletion application

Prepared source, pending Rust acceptance. IssueDeletion owns the existing admin-only,
issue-kind-only gate, archive precedence, conditional-delete result and audit/event
ordering. The service supplies borrowed Database/AuditService/EventBus/repository/actor
handles. It constructs no store until the admitted deletion operation is polled.
DeletionEffects is included in the qualified legacy-component AST policy.

The original issue_permissions(Some(actor), access, issue).can_delete is exactly
access.can_admin() && issue.kind == IssueKind::Issue: the actor is present and Admin
implies Read. Forbidden still precedes Archived, and the original SQL independently
excludes pull requests. There is no new lookup, throttle, clock call or transaction.

After SQL returns its snapshot, existing audit target/detail formatting is retained.
The event copies repo/id/number/kind and boxes the single moved snapshot. The former
full Issue clone (including title/body allocations) is removed; a policy fixture checks
the body allocation is moved unchanged. This is a concrete allocation reduction, not
a measured latency claim.

Deletion remains one committed SQL statement, followed by best-effort audit and a
transient event. Audit failure still logs and allows publication. Cancellation during
that audit can leave a deleted row without the event, as before. No outbox, rollback
or durable cross-store guarantee is introduced.

Prepared tests cover denied/unpolled/archive/pull paths, missing/store errors, exact
moved allocation and postdelete cancellation; actual stores cover 0-query rejected
paths, 2 acquisitions/statements on success (delete+audit), 1 on a stale repeat,
racing SQL winner publication and forced audit failure. The source guard passes.
The cumulative AST check, Rust/Clippy, whole issue suites and HTTP deletion
acceptance remain pending:

```text
cargo test -p rbit-collaboration --features applications --test issue_deletion
cargo test -p rbit-services --test integration issues::deletion_port
cargo test -p rbit-services --test integration issues::only_admins_delete_issues_and_never_pull_requests
```

This prepares the bounded deletion authority. Its acceptance gates, the collaboration
feature group and remaining IssueService background/controller ownership stay open.
