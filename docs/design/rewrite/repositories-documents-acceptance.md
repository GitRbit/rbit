# Repository documents acceptance

Acceptance evidence for the `repositories.documents` ledger row ("Safe Markdown/Mermaid,
relative links, document tabs and outline"), following the
[identity retirement](identity-retirement.md) pattern. It is one of three rows retired into
the new `rbit-presentation` crate in the same slice; the others are
[navigation](presentation-navigation-acceptance.md) and
[themes and the installable app](presentation-themes-pwa-acceptance.md).

## Why a new crate

The [rewrite plan](../ground-up-rewrite-plan.md#proposed-crate-responsibilities) assigns
"shared Askama components, view contracts and safe content rendering" to
`rbit-presentation`, and `scripts/rewrite/check.mjs` already declared its allowed edges
(`rbit-contracts`, `rbit-localization`) before this slice. The crate did not exist yet; the
collaboration gap record also names it as the owner of safe content rendering. It is
created here as a strict crate (400-line files and cumulative types). It currently needs
neither declared internal edge: its decisions take plain strings and a port.

New internal edge: `rbit-web -> rbit-presentation`, added to `ADDED_EDGES`. It is the
plan's direction (web consumes presentation; presentation knows no HTTP), and it replaces
`rbit-web`'s direct `pulldown-cmark` and `ammonia` dependencies, which `rbit-web` no longer
has.

## Scope

| Decision | Legacy place | Now |
| --- | --- | --- |
| Markdown options, heading anchors and slugs, nesting/raw-HTML bounds, the one shared ammonia policy, `user-content-` id prefix, fragment rewriting, `srcset` filtering, rewritten-text runs, `escape_html` | `rbit-web/src/render/markdown.rs`, `render/markdown_bounds.rs` | `rbit_presentation::markdown` (`markdown.rs`, `links.rs`, `headings.rs`, `bounds.rs`) |
| Mermaid fences keep only `language-mermaid` and escaped source | same sanitiser policy | same policy, moved |
| Relative link targets | `DocumentBase::resolve`, called by the sanitiser | the `LinkResolver` port; `rbit-web` implements it for `DocumentBase` (`urls.rs`), so repository URL policy stays with the URL builder |
| `LinkKind`, `ResolvedLink` | `rbit-web/src/urls.rs` | `rbit_presentation::markdown`, re-exported from `urls.rs` |
| Document tabs: conventional names, kinds and order, README priority, 32-document bound, `.github`/`docs` fallbacks (never licenses), `LICENSES/`, labels and icons | `rbit-web/src/routes/repository_documents.rs` | `rbit_presentation::documents` |

What stays in `rbit-web`, as adapters: the document cache (`render/document_cache.rs`,
keyed by `DocumentBase`, unchanged `RENDERER_VERSION`), the render job pool, Git tree and
blob reads, document URLs, and the browser assets (`repo-documents.js` outline and tabs,
the Mermaid frame). `render_markdown` and `render_markdown_rewriting` keep their
signatures; every caller is unchanged.

## Legacy code removed

- `rbit-web/src/render/markdown.rs` (renderer, sanitiser, thread-local document scope,
  slugs, srcset) and `render/markdown_bounds.rs` (moved with its tests).
- `LinkKind` and `ResolvedLink` definitions in `urls.rs`.
- `Kind`, `Candidate`, `regular`, `text_stem`, `kind`, `candidates`, `readme_priority`,
  `sort_and_bound`, `directory`, `license_name`, `label`, `MAX_DOCUMENTS` and
  `MAX_FALLBACK_ENTRIES` in `repository_documents.rs`; its `discover` only reads trees.
- `pulldown-cmark` and `ammonia` from `rbit-web`'s dependencies.

## Boundary that fails if legacy paths return

`scripts/rewrite/presentation-retirement.mjs`, run by `scripts/rewrite/check.mjs`, scans
authored Rust below `crates/` except `rbit-presentation` (comments stripped) and rejects,
for this row: any `ammonia::` use; Markdown rendered to HTML (`push_html`,
`pulldown_cmark::html`); a redefined `LinkKind`/`ResolvedLink`; and in `rbit-web` a
redefined renderer function (`unique_slug`, `with_heading_ids`, `resolve_srcset`,
`prefixed_fragment`, `raw_depth`, `filter_attribute`, `configured_builder`) or document
tab selection (`text_stem`, `readme_priority`, `license_name`, `sort_and_bound`,
`MAX_DOCUMENTS`). Markdown parsing without rendering (`rbit-core` references) is allowed.
Run against the legacy files at `4868231e`, the checker reports 12 violations across the
three rows (6 for this one); against this tree, none.
`scripts/tests/presentation-retirement.test.mjs` pins each rule, the allowed cases and the
whole tree.

## Gates

| Gate | Evidence |
| --- | --- |
| Byte-identical HTML | The pipeline moved verbatim: same options, event order, bounds, anchor insertion, ammonia builder calls in the same order, attribute filter and `srcset` code. The only change is that the filter calls the base through `&dyn LinkResolver` instead of `&DocumentBase`, and the base is lent as `Box<dyn LinkResolver>` (a clone, as before). The existing rendering tests run unchanged in `rbit-web/src/render/markdown_tests.rs` through the real `DocumentBase` and cache, including the exact anchor markup, cached prefixed ids and `srcset` cases; `tests/properties.rs` (hostile Markdown never yields script or handlers) is unchanged. |
| Sanitisation | One process-wide sanitiser, configured once; the thread-local scope is restored on drop. The test proving both (`one_sanitiser_serves_every_document_and_forgets_each_base`) moved to `rbit-presentation` with a test resolver, as it inspects private state. Hostile-nesting linear-time test moved with the bounds. |
| Document tabs | Same order of tree reads (root, `.github`, `docs`, `LICENSES`), same 512-entry bound per fallback, same sort and labels; the two discovery tests moved verbatim (entries built as `DocumentEntry`), plus fallback precedence and generic-license label tests. `document_links_pin_the_snapshot_and_escape_queries` stays in `rbit-web`. |
| CSP, cache, localisation | No template, asset, CSP hash, cache key or catalog change. `RENDERER_VERSION` is unchanged because output is unchanged. |
| Migration and rollback | No schema, migration or stored data. Latest migration remains 0073. Rollback is the previous binary. |
| Performance | One extra `Box` per rendered document with a base; one virtual call per link attribute. Document discovery maps tree entries through a borrowing iterator (no allocation). No release measurement is claimed. |

## Results

See the [themes and installable app record](presentation-themes-pwa-acceptance.md#results);
the three rows were verified together at one commit.

## Not covered

- The Askama templates (`repo/documents.html` and the shared layout) and the CSS stay in
  `rbit-web`. The [rewrite plan](../ground-up-rewrite-plan.md#proposed-crate-responsibilities) gives `rbit-presentation` "shared Askama components"; moving them
  is a template-wide change with its CSP and inline-style hashes, not part of this row.
- The client-side outline and tab switching (`repo-documents.js`) and the Mermaid frame
  remain in `rbit-web`, which the same table makes the owner of "browser assets".

## Cost

Shared with the other two rows; see
[themes and installable app](presentation-themes-pwa-acceptance.md#cost). This row's share:
the renderer move (+455 lines in `rbit-presentation/src/markdown*`, of which 170 is the
moved bounds file), the document tab move (+260 including tests), and a 30-line adapter in
`render/mod.rs` and `urls.rs`.
