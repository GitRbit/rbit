# Plugin package parser fuzzing

This standalone workspace keeps libFuzzer and sanitizer instrumentation out of the
server dependency graph. It exercises `rbit-plugin-packages::validate_archive`;
it does not install or execute packages.

Use an installed nightly toolchain with AddressSanitizer support. The tooling can
be installed locally without changing the default Rust toolchain:

```sh
cargo install cargo-fuzz --version 0.13.2 --locked --root target/fuzz-tools
python3 fuzz/seed-corpus.py
RUSTUP_TOOLCHAIN=nightly target/fuzz-tools/bin/cargo-fuzz run archive_bytes --fuzz-dir fuzz -- \
  -max_total_time=60 -max_len=2097152 -rss_limit_mb=1024 -timeout=10 -print_final_stats=1 -seed=20261101
RUSTUP_TOOLCHAIN=nightly target/fuzz-tools/bin/cargo-fuzz run archive_structured --fuzz-dir fuzz -- \
  -max_total_time=60 -max_len=2097152 -rss_limit_mb=1024 -timeout=10 -print_final_stats=1 -seed=20261101
```

The seed generator writes deterministic seed names and preserves discovered corpus
files. Corpora, artifacts, coverage output and build products are ignored. Minimized
findings belong in ordinary regression tests before a fix is considered complete.
The fuzz lockfile preserves the production crates' shared dependency versions;
`libfuzzer-sys` is pinned separately.

`archive_bytes` accepts bounded arbitrary bytes. It compares ordinary and fragmented,
interrupted readers. Successful results must have a valid manifest, exact consumed
wire length and the declared aggregate payload length.

`archive_structured` builds bounded valid data, native and Wasm package envelopes,
then selects either a valid case or a deliberately invalid mutation. It repairs
checksums when necessary to reach deeper paths. Mutations cover entry types, sizes,
content, metadata, raw paths, numeric fields, padding and termination. Its acceptance
oracle and reader-equivalence checks must agree. Seeds include USTAR prefix paths,
zero-length data and 512-byte/64-KiB payload boundaries.

The input cap is 2 MiB; generated structured payloads reach 65,537 bytes. These are
focused parser campaigns, not exhaustive tests of the maximum 1 GiB package size,
transport deadlines, installation, filesystem publication or worker execution.

## Recorded local campaign

On 2026-11-01 UTC, the parser source at `f11c362` ran under installed Rust
`1.95.0-nightly (859951e3c 2026-02-24)`, cargo-fuzz 0.13.2 and libfuzzer-sys 0.4.13,
with AddressSanitizer, debug assertions and overflow checks enabled. Each all-kinds
campaign completed in 61 seconds with exit status zero and no finding:

| Target | Executions | Coverage counters, initial → final | Peak RSS |
| --- | ---: | ---: | ---: |
| `archive_bytes` | 6,875,188 | 2,599 → 2,846 | 579 MiB |
| `archive_structured` | 350,166 | 1,753 → 1,753 | 407 MiB |

The structured target's feature count increased from 2,410 to 2,416. These libFuzzer
counters are not line-coverage percentages. The raw corpus grew during the campaign;
prior exploratory data-only campaigns had already contributed seeds. A clean bounded
campaign is evidence, not proof of safety.

Ignored `target/fuzz-evidence/` retains build logs, complete campaign logs, commands,
compiler and tool versions, source/binary hashes and corpus counts. See the
[Rust Fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz/tutorial.html) for tooling
and [structure-aware fuzzing](https://rust-fuzz.github.io/book/cargo-fuzz/structure-aware-fuzzing.html).
