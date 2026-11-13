# gix-utils 0.5.2 compatibility source

Source: the published crates.io `gix-utils` 0.5.2 package, upstream
[GitoxideLabs/gitoxide](https://github.com/GitoxideLabs/gitoxide) revision
`584a602385dd3ad4dacfdd9fd77a13c193a87dfe`, subdirectory `gix-utils`, as recorded
in `.cargo_vcs_info.json`. `UPSTREAM-SHA256.json` records the original published
file hashes. Both upstream licenses and the original manifest are retained.
The published crate archive SHA-256 is
`11a9446060447e9ec58f5d333b98bed53389dfa10de5d8dd46fc76d01dfc2954`,
matching the original registry lockfile checksum. All 18 original file hashes
were also verified against that archive. The package-local Cargo.lock and Cargo
cache marker are omitted.

All Rust source files are byte-identical to the published package. The sole code
configuration change is the normalized manifest's unicode-normalization dependency:
it uses official Unicode 17 source revision
`5a69b3bafb625caccdec7871a42bed0d9a6604d1` with its existing default-features=false.
The root patch affects only gix-utils; registry normalization consumers remain current.
This preserves gitoxide's existing macOS ref/path precomposition and byte identity
as its tables update. No algorithm was rewritten and no generated tables were copied.

Revalidate every upstream upgrade against ordinary/pinned RepositoryReader tests,
native Git, NFC/NFD loose/packed refs and distinct Unicode 17 packed identities.
The package is excluded from first-party workspace membership, remains a local
Cargo dependency in source-proof snapshots, and retains cargo-deny license coverage.
