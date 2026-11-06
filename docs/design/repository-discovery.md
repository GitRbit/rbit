# Repository model ownership and public discovery

`rbit-repositories` owns repository names, metadata, visibility and listing selectors.
Account projections come from identity's model-only dependency surface. Core's
existing public paths reexport the same types, preserving validation, derives,
JSON variant names, field visibility and storage conversion identity.

The public discovery application owns search normalization, fixed page size,
page clamping and offset arithmetic. Its static `PublicRepositoryCatalog` port
returns domain models and implementation-defined errors through ordinary Rust
futures. It imports no configuration, runtime, SQL, Git or HTTP implementation.
The service remains the public compatibility adapter and delegates to this
application. Storage forwards the port to its existing statements unchanged.

Both storage statements independently enforce public visibility, archive state
and escaped literal matching. Count and row reads remain separate statements:
concurrent updates can yield stale totals or an empty final page. This port does
not add a snapshot guarantee, buffer packs, cache authorization, or alter query
plans. Stable SQL sorting and row validation remain storage responsibilities.

Legacy public page fields remain public, including arbitrary `per_page` values;
their arithmetic behavior is unchanged. English legacy selector labels remain
available; localized page rendering continues at the presentation boundary.

Creation, owner authorization, transfers, forks, settings, grants, redirects,
Git initialization and compensation remain in their existing adapters. Moving
those workflows requires a separate coherent ownership and concurrency design.

Acceptance covers exact model aliases and serialization; fake-port normalization,
failure order, extreme pagination and separate-query behavior; real service/SQL
privacy, literal escaping, archive filters, owner sorting and statement counts.
Existing Explore localization/full/fragment/history tests remain protocol parity
gates. No migration, schema, SQL or route changes are required for this slice.
