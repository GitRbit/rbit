//! # rbit-presentation
//!
//! The presentation decisions of the rbit web interface that do not depend on HTTP,
//! storage or the embedded asset bundle:
//!
//! * [`markdown`] — untrusted Markdown to sanitised HTML: heading anchors, structural
//!   bounds, the one shared ammonia policy, id prefixing and relative-link rewriting
//!   through the [`markdown::LinkResolver`] port.
//! * [`documents`] — the document tabs of a tree page: which conventional documents, in
//!   which order, under which label.
//! * [`navigation`] — the partial-page cut that boosted navigation inside one repository
//!   receives: the layout markers, the same-repository check and the `Vary` value.
//! * [`themes`] — the built-in and tinted colour themes, their slugs, variants, launch
//!   colours, light/dark toggle and stylesheet paths.
//! * [`pwa`] — the installable app: the web manifest, the service worker version and the
//!   branding stylesheet with its cache policy.
//!
//! Allowed dependencies: `rbit-contracts` and `rbit-localization` (architecture policy),
//! plus pulldown-cmark, ammonia, `serde_json` and sha2. No HTTP types, I/O, async, storage
//! or account state cross this boundary; `rbit-web` adapts requests, cookies, response
//! headers, hashed asset URLs and repository URLs to these functions.
//!
//! State ownership: the process-wide sanitiser (configured once) and the per-thread
//! document a sanitising call resolves links against. Nothing else is stored.
//!
//! Invariants: sanitising is always the last step of rendering; every rendered `id` carries
//! [`markdown::ID_PREFIX`]; a partial page is a subset of the full page plus one
//! `hx-swap-oob` attribute; the service worker version changes with every asset build and
//! every change of the site's identity.
//!
//! Resource costs: rendering is linear in the document's size (see [`markdown::bounds`]);
//! the partial cut scans the page once; the theme catalog is static data searched by
//! binary search.

pub mod documents;
pub mod markdown;
