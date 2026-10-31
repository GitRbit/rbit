# Native plugin forms and mutations

Trusted embedded plugins can pair compiled Askama GET pages with bounded POST
handlers. This builds on [native plugin pages](plugin-routes.md); executable code
remains part of the server build. Uploaded language packs and declarative UI
contributions cannot register executable handlers or provide template source.

## Registration and forms

Use `Services::mutation_registry()` (types are reexported as
`rbit_services::plugin_mutations`) from trusted startup composition. Stage a
`MutationRegistration` under the plugin's current scope and generation, then
publish after startup succeeds. The registry is shared across service clones.
Replacement validates the entire candidate before publishing it. Every batch gets
a fresh version, including replacement within the same generation. Registry
publication, replacement and removal change the host's plugin revision.

Mutations use POST at the same reserved global or repository namespace as pages:
`/-/plugins/{plugin}/{path}` or
`/{owner}/{repo}/-/plugins/{plugin}/{path}`. Paths are validated literal suffixes,
not patterns or arbitrary router mounts. Global mutations require `SignedIn` or
`SiteAdmin`; repository mutations require `RepositoryWriter` or
`RepositoryAdmin`. Public and read-only audiences cannot register mutations.
The host resolves the actor and repository permissions before admitting a handler.

A GET page explicitly declares the mutation paths it needs using
`PageDescriptor::with_forms(Vec<FormReference>)`. Use `compiled_form_page` to
receive `FormActions` alongside the ordinary page context. The host supplies an
action only for a currently permitted mutation with the same plugin owner,
generation and scope. A compiled template renders its `FormAction::action()` as
the form target and `FormAction::csrf()` in a hidden `_csrf` input. Use normal
Askama escaping; do not mark action, proof or user input as safe HTML. Neither the
session token nor its digest is passed to plugin callbacks.

For example, a schema and typed callback can be built as follows. The surrounding
startup controller supplies the scope and generation and owns publication.

```rust,ignore
use rbit_services::plugin_mutations::{
    compiled_mutation, FormField, FormFieldKind, FormSchema, MutationAudience,
    MutationDescriptor, MutationOutcome, PageScope, PluginRedirect,
};
use std::time::Duration;

#[derive(askama::Template)]
#[template(source = "<p>{{ message }}</p>", ext = "html")]
struct ValidationError { message: String }

let schema = FormSchema::new(vec![FormField {
    name: "title".into(),
    kind: FormFieldKind::Text { max_bytes: 200, multiline: false },
    required: true,
}])?;
let descriptor = MutationDescriptor::new(
    "save", PageScope::Repository, "save", MutationAudience::RepositoryWriter,
    Duration::from_secs(1), schema,
)?;
let registration = compiled_mutation(descriptor, |context, values| async move {
    let title = values.text("title").expect("required validated field");
    // Await the authorized transactional domain operation here. Use
    // context.repository and context.actor as host-resolved identity facts.
    let _ = (context, title);
    Ok(MutationOutcome::<ValidationError>::Redirect(PluginRedirect::new("edit")?))
});
let batch = services.mutation_registry().stage(plugin_id, &scope, generation, vec![registration])?;
batch.publish()?;
```

`MutationOutcome::Invalid(CompiledPage<T>)` renders a bounded compiled validation
page with status 422. `Redirect(PluginRedirect)` accepts only a validated literal
path in the same plugin and scope; it cannot redirect to an external origin or a
core administration route. Templates remain compiled Rust types, with the same
bounded output and escaping responsibilities as GET pages.

## Host validation and CSRF

The adapter accepts only uncompressed `application/x-www-form-urlencoded` input,
with an optional UTF-8 charset. Encoded bodies are bounded to 32 KiB before
parsing. Schemas have at most 32 domain fields; text fields have at most 4096
bytes. Unknown fields, duplicate decoded names, malformed escapes, invalid UTF-8,
reserved domain field names and values outside the declared schema are rejected.
The host `_csrf` field is removed before typed callback input is constructed.
Form values and proof-bearing structures redact secrets from debug output.

The host issues an opaque HMAC proof with a random boot key and nonce. It binds
the session digest, actor ID, repository ID when applicable, exact canonical
POST action, plugin generation and registration version, plus issuance time.
Proofs expire after 30 minutes, with at most 60 seconds of future clock leeway.
The server derives expected claims from the authenticated request and current
route metadata; browser-supplied generations or versions are not trusted.
Replacement, restart, session changes and re-enablement invalidate old forms.
A form left open across those transitions must be reloaded.

Origin and Fetch Metadata are checked independently: a same-origin Fetch header
cannot override an invalid Origin. The scheme, host and effective port must match
the configured public URL. Missing browser headers do not replace proof
verification. Existing session, forced-password-change, request limits and host
security headers remain in force. Plugin POST handlers receive no raw headers,
cookies, credentials, request extensions or arbitrary body stream.

These proofs prevent cross-site request forgery. They are not one-use receipts or
idempotency keys. A valid form may be submitted more than once before expiry;
plugins must use transactional constraints or explicit domain idempotency where
repeated effects would be harmful.

## Cancellation and commit lifetime

Mutation admission is limited to 64 concurrent operations across the host and
128 registered routes, with at most 32 routes per registration. Admission includes
input reading, callback execution and the guarded response lifetime. Uploads have
an absolute one-second deadline; drip-fed frames do not extend it. After reading
and verifying a proof, the host refreshes session and repository authorization
before entering native code. Callback
HTTP waiting is bounded by a declared deadline of at most one second. Request
cancellation, deadline expiry, replacement and plugin withdrawal signal an
advisory cancellation token.

Once callback execution starts, the host tracks it in an owned task. Ending the
HTTP waiter does not drop a possible database commit halfway through. The
operation lease and native captures remain owned until the callback actually
finishes. A queued callback canceled before dispatch does not enter native code.
Native handlers must await their domain commits, avoid detached untracked writes,
and use explicit transaction boundaries. Cancellation cannot undo committed
writes. An unavailable or timed-out response can therefore represent an unknown
outcome; clients must not automatically retry it.

`Services::prepare_shutdown()` synchronously closes mutation admission together
with other plugin entry points. Cloned registries and stale registrations cannot
stage, publish or replace after terminal close. `Services::shutdown()` then awaits
`MutationRegistry::wait_for_idle()` before shutting down built-in features or core
services and stores. This barrier has no false-success timeout: a noncooperative
trusted native handler can delay shutdown until its actual operation completes.
The HTTP server's request deadline alone is not evidence that a commit drained.

This API controls validation, lifecycle and host authority. It does not sandbox
trusted native Rust, unload machine code, promise rollback, or provide a generic
exactly-once commit ledger.
