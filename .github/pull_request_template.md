## What and why

<!-- What this changes and why. Link the issue it fixes, e.g. "Fixes #123". -->

## How it was tested

<!-- New or changed tests, and anything checked by hand (pages at 375 px, Git clients, ...). -->

## Checklist

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass
- [ ] Each commit is atomic, with an imperative subject of at most 72 characters and a body that explains why
- [ ] User-visible changes are in `CHANGELOG.md` (under "Unreleased", with upgrade notes if needed) and in the docs
- [ ] New pages follow the design rules in [CONTRIBUTING.md](https://github.com/GitRbit/rbit/blob/main/CONTRIBUTING.md) (no inline styles or scripts, checked at 375 px)
