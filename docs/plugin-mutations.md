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
`RepositoryAdmin`. `RepositorySignedIn` is an explicit policy floor for domain-
authorized operations such as deployment reviews by designated readers; it still
requires signed-in repository read access and the callback must enforce the
operation-specific domain rule. It does not imply write or administrator access.
Public audiences cannot register ordinary mutations.
The sealed locale-preference capability below is the explicit guest exception.
The host resolves the actor and repository permissions before admitting a handler.

A GET page explicitly declares the mutation paths it needs using
`PageDescriptor::with_forms(Vec<FormReference>)`. Use `compiled_form_page` to
receive `FormActions` alongside the ordinary page context. The host supplies an
action only for a currently permitted mutation with the same plugin owner,
generation and scope. A compiled template renders its `FormAction::action()` as
the form target and `FormAction::csrf()` in a hidden `_csrf` input. Use normal
Askama escaping; do not mark action, proof or user input as safe HTML. Neither the
session token nor its digest is passed to plugin callbacks.

For example, the matching compiled GET view can render the issued action directly.
When authorization does not permit the mutation, its action is absent and the view
omits the form. Native submissions use the ordinary host 303/422 behavior.
For validation rerenders, `MutationContext::forms()` contains the currently
authorized same-owner, same-scope, same-generation sibling actions, bounded to
32 entries. It grants no session or proof-issuing capability.

```rust,ignore
use rbit_services::plugin_pages::{
    compiled_form_page, CompiledPage, FormAction, FormReference,
    PageAudience, PageDescriptor, PageScope,
};
use std::time::Duration;

#[derive(askama::Template)]
#[template(source = r#"
{% if let Some(form) = form %}
<form method="post" action="{{ form.action() }}" hx-boost="false">
  <input type="hidden" name="_csrf" value="{{ form.csrf() }}">
  <label>Title <input name="title" required maxlength="200"></label>
  <button type="submit">Save</button>
</form>
{% endif %}
"#, ext = "html")]
struct Editor { form: Option<FormAction> }

let page = compiled_form_page(
    PageDescriptor::new(
        "edit", PageScope::Repository, "edit", PageAudience::RepositoryReader,
        Duration::from_secs(1),
    )?.with_forms(vec![FormReference::new("save", "save")?])?,
    |_, actions| async move {
        CompiledPage::new("Edit", Editor { form: actions.get("save").cloned() })
    },
);
// Stage and publish this page under the same owner/scope/generation as its mutation.
```

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
page with status 422. `Success(CompiledPage<T>)` renders a successful response with
status 200, for example a one-time runner registration token. Do not place such
secrets in redirect query strings or shared caches. `Redirect(PluginRedirect)` accepts only a validated literal
path in the same plugin and scope; it cannot redirect to an external origin or a
core administration route. `PluginRedirect::with_query(&[("run", "12")])` adds
bounded encoded query values to that same validated target; it does not allow a
new origin, namespace or arbitrary redirect header.
`MutationDescriptor::with_document()` uses the same reviewed full-document
presentation as compiled GET pages, without duplicating host chrome. Templates
remain compiled Rust types, with the same
bounded output and escaping responsibilities as GET pages.

`MutationDescriptor::with_body_limit(bytes)` independently sets the compiled
response output limit, defaulting to 256 KiB and capped at 2 MiB. This is distinct
from the submitted form's encoded-body limit. Responses declared above 256 KiB
also consume one of eight large-document permits in the shared mutation registry,
in addition to an ordinary operation permit. The GET and mutation registries have
separate eight-permit pools. A mutation retains its permits through actual native
work completion and the final guarded response lifetime; callback completion alone
does not release capacity for an unread successful or validation document.

## Host validation and CSRF

For ordinary forms, the adapter accepts only uncompressed
`application/x-www-form-urlencoded` input,
with an optional UTF-8 charset. Encoded bodies are bounded to 32 KiB before
parsing. Schemas have at most 32 domain fields; text fields have at most 4096
bytes. Unknown fields, duplicate decoded scalar names, malformed escapes, invalid UTF-8,
reserved domain field names and values outside the declared schema are rejected.
Boolean values must be the literal `true` or `false`; checkboxes should explicitly
use `value="true"`. An absent optional field remains absent. Text limits count
UTF-8 bytes, so browser character limits supplement server validation.
The host `_csrf` field is removed before typed callback input is constructed.
Form values and proof-bearing structures redact secrets from debug output.

Larger text is an explicit schema choice: `FormSchema::large_text(fields)` allows
up to 64 KiB per declared text field and a 256 KiB encoded body. Ordinary schemas
retain the 4 KiB/32 KiB defaults. This preserves Actions secret and variable limits
without raising limits for every form. All body limits apply before decoding;
percent-encoded input consumes its actual wire size.

Declare repeated values using `FormFieldKind::TextList { max_items, max_bytes,
multiline }` and retrieve them with `FormValues::list`. Lists allow at most
128 items. `FormSchema::with_prefix("inputs[", "]", 25, 4096)` permits a bounded
map of workflow-defined input names; unknown names outside that declaration remain
invalid. A prefix permits at most 32 entries, and the whole schema permits at most
256 wire pairs. `FormValues::to_pairs()` preserves validated names and expands
only declared list fields. Duplicate scalar, dynamic-prefix and `_csrf` fields
remain errors. Render a boolean once, such as a select or an optional checkbox;
do not combine a hidden `false` with a duplicate checked `true` field.

Actions mutation paths are exact suffixes such as `cancel` or `rerun-job`.
Resource IDs travel as declared form fields and receive fresh domain ownership
and permission checks. The proof binds the action and generation; it does not make
an arbitrary submitted run, job or environment authorized. Secret values are not
returned in validation pages or debug output.

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
Repository IDs bind the observed host row, not a durable incarnation: SQLite can
reuse a deleted ID. These proofs therefore do not promise invalidation after
every delete-and-recreate at the same path. The host still checks current access;
plugins needing stale-object protection must enforce domain version preconditions.

## Explicit upload and guest preference capabilities

`compiled_upload` pairs a global `SiteAdmin` descriptor with
`MutationDescriptor::with_upload(max_bytes)`. The descriptor must have an empty
ordinary form schema and a positive limit no larger than 2 MiB. Factory/descriptor
pairing is checked during registration: changing an input-kind enum does not grant
another factory upload or anonymous privileges. The callback receives typed
`UploadData` bytes and authenticated `MutationContext`, without filenames, MIME
metadata, raw headers or a multipart stream.

The host accepts only uncompressed multipart input for that declared capability:
first one `_csrf` text field, then one `pack` binary field, with no further fields.
Proof fields are capped at 128 bytes; framing adds at most 8 KiB to the declared
archive limit. The complete wire body, including any MIME epilogue, is counted.
The entire upload has a ten-second deadline, and at most four upload operations
can be admitted across the host. Those permits supplement the ordinary 64-operation
mutation limit and remain held through the guarded response lifetime. Session and
administrator authorization are refreshed after reading and verifying the proof.
The archive installer must still validate its own format and transactional writes.

`MutationDescriptor::locale_preference` and `compiled_locale_preference` form a
separate, sealed global capability. Its callback receives `PreferenceContext`
with an optional authenticated actor and a typed `LocaleChoice`, and returns
`PreferenceOutcome`. It cannot make an ordinary public mutation or name arbitrary
cookie/header effects. `PreferenceOutcome::Selected { choice, redirect }` asks
the host to set or clear only its locale preference cookie; the host controls its
name, path, lifetime, HttpOnly, SameSite and Secure attributes. The language plugin
also validates the choice against currently available locales and persists the
preference for an authenticated account.

A guest GET form receives an opaque browser-binding cookie and a separate
browser-domain proof, bound to that browser, exact route, generation, registration
version, host boot and expiry. HTTPS uses a Secure `__Host-` binding cookie; local
HTTP has an explicitly separate fallback cookie. Authenticated forms use the
session proof instead. Neither proof domain can authorize the other, and only a
declared locale-preference route accepts the guest domain. Origin checks remain
mandatory when the browser supplies them. Callback code receives no browser
binding secret, session credentials, CookieJar or arbitrary request headers.

Languages uses these APIs at `/-/plugins/rbit.i18n/language` and the installed
`rbit.i18n-admin` pack-management routes. See [Languages](i18n.md) for lifecycle
ownership and explicit `prepare_plugin_routes` startup integration. These are
bounded native capabilities, not dynamically installed code or unrestricted HTTP
middleware.

## Cancellation and commit lifetime

Mutation admission is limited to 64 concurrent operations across the host and
128 registered routes, with at most 32 routes per registration. Admission includes
input reading, callback execution and the guarded response lifetime. Uploads have
an absolute one-second deadline for ordinary forms; declared binary uploads use
the separate ten-second bound. Drip-fed frames do not extend either deadline. After reading
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
