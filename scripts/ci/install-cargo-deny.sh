#!/usr/bin/env bash
# Installs a prebuilt cargo-deny into the directory given as $1, pinned by version and
# SHA-256 so CI never runs an unchecked download. Linux x86_64 only (the musl build runs on
# any distribution). To update: change both values to a new release and its published
# `.sha256`.
set -euo pipefail
version="0.20.2"
sha256="9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f"
dest="${1:?usage: install-cargo-deny.sh DIR}"
name="cargo-deny-${version}-x86_64-unknown-linux-musl"
mkdir -p "$dest"
curl -fsSL --retry 3 -o "$dest/$name.tar.gz" "https://github.com/EmbarkStudios/cargo-deny/releases/download/${version}/${name}.tar.gz"
echo "${sha256}  $dest/$name.tar.gz" | sha256sum --check --quiet
tar -xzf "$dest/$name.tar.gz" -C "$dest" --strip-components=1 "$name/cargo-deny"
rm "$dest/$name.tar.gz"
"$dest/cargo-deny" --version
