# syntax=docker/dockerfile:1.7
#
# Rustit container image: one static-ish binary plus `git`, running as an unprivileged user.
#
#   docker build -t rustit .
#   docker run -d --name rustit -p 3000:3000 -v rustit-data:/data \
#     -e RUSTIT_SERVER__PUBLIC_URL=https://git.example.com rustit
#
# Configuration comes from RUSTIT_* environment variables or a TOML file mounted into the
# container and passed with RUSTIT_CONFIG=/path/to/rustit.toml. See docs/deployment.md.

ARG RUST_VERSION=1
ARG NODE_VERSION=24
ARG DEBIAN_RELEASE=trixie

# ---------------------------------------------------------------- 1. stylesheet
# Tailwind scans the templates and Rust sources for class names, so only those (and the
# stylesheet entry point) are copied in; Node never reaches the final image.
FROM node:${NODE_VERSION}-${DEBIAN_RELEASE}-slim AS assets
WORKDIR /src
COPY package.json package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci --ignore-scripts --no-audit --no-fund
COPY crates/rustit-web/styles crates/rustit-web/styles
COPY crates/rustit-web/templates crates/rustit-web/templates
COPY crates/rustit-web/src crates/rustit-web/src
RUN mkdir -p crates/rustit-web/assets && npm run build:css

# ---------------------------------------------------------------- 2. release build
FROM rust:${RUST_VERSION}-${DEBIAN_RELEASE} AS build
WORKDIR /src
COPY . .
# Assets are embedded into the binary at compile time, so the CSS must exist first.
COPY --from=assets /src/crates/rustit-web/assets/app.css crates/rustit-web/assets/app.css
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --package rustit-server \
    && install -D -m 0755 target/release/rustit /out/rustit

# ---------------------------------------------------------------- 3. runtime
FROM debian:${DEBIAN_RELEASE}-slim AS runtime

# git: the smart-HTTP transport, `git gc` and backups run the git executable.
# tini: PID 1 that forwards signals and reaps orphaned git processes.
# curl: the health check below.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git tini curl \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --system --gid 10001 rustit \
    && useradd --system --uid 10001 --gid rustit --home-dir /data --shell /usr/sbin/nologin rustit \
    && install -d -o rustit -g rustit -m 0750 /data

COPY --from=build /out/rustit /usr/local/bin/rustit

# Both listeners bind every interface inside the container; publish them with -p. The SSH
# server only starts with RUSTIT_SSH__ENABLED=true; its built-in default bind is loopback,
# which a published port cannot reach. NO_COLOR keeps `docker logs` free of ANSI escapes.
ENV RUSTIT_SERVER__BIND=0.0.0.0:3000 \
    RUSTIT_SSH__BIND=0.0.0.0:2222 \
    RUSTIT_STORAGE__DATA_DIR=/data \
    RUST_LOG=info \
    NO_COLOR=1

USER rustit:rustit
WORKDIR /data
VOLUME ["/data"]
# 3000: HTTP. 2222: the built-in SSH server, when [ssh] enabled = true.
EXPOSE 3000 2222

# /healthz answers 503 when SQLite or redb cannot be read. If you change the bind port,
# override this check (docker run --health-cmd …) to match.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD ["curl", "--fail", "--silent", "--show-error", "--max-time", "4", "http://127.0.0.1:3000/healthz"]

STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/rustit"]
CMD ["serve"]
