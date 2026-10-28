# Deploying rbit

rbit is one binary plus the `git` executable. All state lives in one data directory:

| Path (under `data_dir`) | Contents | Back up? |
|---|---|---|
| `rbit.db` (+ `-wal`, `-shm`) | SQLite: accounts, repositories, tokens, settings, audit log | **Yes** |
| `secret.key` | The key that seals credentials stored in `rbit.db` (see [Secret key](#secret-key)) | **Yes**, with the database |
| `repositories/<owner-id>/<repo-id>.git` | Bare Git repositories | **Yes** |
| `attachments/<repo-id>/<asset-id>` | Files attached to releases | **Yes** (not `attachments/tmp`) |
| `lfs/<aa>/<bb>/<oid>` | Git LFS objects, stored once however many repositories use them | **Yes** |
| `tmp/` | Scratch space: merges, web edits, cross-fork overlays, LFS uploads in progress | No (swept at start-up) |
| `rbit.redb` | Login sessions and rebuildable caches | No (see [Backups](#backups)) |
| `actions/secrets.key` | The key that seals Actions secrets (only with Actions enabled) | **Separately**: not in `rbit backup` |
| `actions/logs/`, `actions/artifacts/` | Job logs and workflow artifacts (only with Actions enabled) | No (kept for a retention period) |

Run it behind a reverse proxy that terminates TLS. The sections below cover
[Docker](#docker), [systemd](#systemd), [reverse proxies](#reverse-proxy-and-tls),
[email](#email), [external sign-in](#external-sign-in), [health checks](#health-checks),
[metrics](#metrics), [background jobs](#background-jobs), [Actions and runners](#actions-and-runners),
[backups and restore](#backups) and [upgrades](#upgrades-and-migrations).

## Configuration

Settings come from built-in defaults, then a TOML file (`--config` or `RBIT_CONFIG`),
then `RBIT_<SECTION>__<KEY>` environment variables (later sources win).
[`deploy/rbit.example.toml`](../deploy/rbit.example.toml) lists every setting with
production-minded values. The ones that matter most:

- `server.public_url` — exactly what users type (`https://git.example.com`). It builds
  clone URLs, decides whether cookies are `Secure`, and is the origin that form
  submissions are checked against. A mismatch breaks sign-in.
- `server.bind` — keep it on loopback (`127.0.0.1:3000`) behind a proxy.
- `storage.data_dir` — the directory above.
- `storage.secret_key_file` — where the [secret key](#secret-key) lives, if not in
  `data_dir`.
- `repository.git_binary` — the `git` used for smart HTTP, `git gc` and backups.

### Secret key

Credentials rbit must be able to read back — TOTP seeds, webhook and push mirror secrets,
the server's own signing keys, and the SMTP password, OIDC client secrets and metrics token
saved from the administrator configuration page — are stored in `rbit.db` encrypted
(AES-256-GCM) with a 256-bit key that is **not** in the database. A copy of `rbit.db` on
its own therefore reveals none of them.

The key is `<data_dir>/secret.key`, generated on first start (mode `0600`; rbit warns if
other users can read it). To keep it apart from the data, for example as a container
secret, point `storage.secret_key_file` (`RBIT_STORAGE__SECRET_KEY_FILE`) at another
file; a missing file there is generated too. The file holds the key in base64, so
`head -c 32 /dev/urandom | base64 > secret.key` makes one in advance.

**Back the key up together with the database.** Without it the encrypted values cannot be
read: users lose their authenticator-app sign-in, webhooks and push mirrors their secrets.
rbit refuses to start, rather than generate a new key, if the database holds encrypted
values and the key file is missing, and also if the key file does not match them.

Upgrading from a version without the key: on the first start rbit creates the key and
encrypts the existing plaintext values in place. Backups taken before the upgrade still
hold them in plaintext; delete or re-encrypt those old backups (restoring one and starting
rbit encrypts it again).

## Docker

Released images are published to `ghcr.io/gitrbit/rbit` for `linux/amd64` and `linux/arm64`,
tagged with the version (`0.1.0`), the minor series (`0.1`) and `latest`:

```bash
docker run -d --name rbit --restart unless-stopped -p 127.0.0.1:3000:3000 -v rbit-data:/data \
  -e RBIT_SERVER__PUBLIC_URL=https://git.example.com ghcr.io/gitrbit/rbit:0.1
```

Pin a version (or the minor series) in production rather than `latest`. The released
image packages the same Linux binaries as the release archives. To build the image
yourself, `docker build -t rbit .` compiles from source.

The [`Dockerfile`](../Dockerfile) builds the stylesheet with Node, compiles a release
binary, and ships it on Debian 13 (trixie) slim with `git`, CA certificates and `tini`
(about 75 MB compressed). The container runs as the unprivileged user `rbit`
(uid 10001), keeps all data in the `/data` volume and has a `HEALTHCHECK` against
`/healthz`.

```bash
docker build -t rbit .
docker run -d --name rbit --restart unless-stopped \
  -p 127.0.0.1:3000:3000 \
  -v rbit-data:/data \
  -e RBIT_SERVER__PUBLIC_URL=https://git.example.com \
  rbit
```

The image sets `RBIT_SERVER__BIND=0.0.0.0:3000` and `RBIT_SSH__BIND=0.0.0.0:2222`
(inside the container), `RBIT_STORAGE__DATA_DIR=/data` and `NO_COLOR=1` (plain log
lines in `docker logs`). For Git over SSH add `-e RBIT_SSH__ENABLED=true` and
`-e RBIT_SSH__PUBLIC_HOST=git.example.com`, publish the port (`-p 2222:2222`, or
`-p 22:2222` when the host's own sshd listens elsewhere) and set
`RBIT_SSH__PUBLIC_PORT` to the published port. The host key is generated in `/data/ssh`
on first start.

To use a file instead of environment variables, mount it read-only and point
`RBIT_CONFIG` at it:

```yaml
# compose.yaml
services:
  rbit:
    build: .
    restart: unless-stopped
    ports:
      - "127.0.0.1:3000:3000"
      # - "2222:2222"            # built-in SSH server, when [ssh] enabled = true
    environment:
      RBIT_CONFIG: /etc/rbit/rbit.toml
    volumes:
      - rbit-data:/data
      - ./rbit.toml:/etc/rbit/rbit.toml:ro
volumes:
  rbit-data:
```

In a config file used with the image, keep `[storage] data_dir = "/data"`,
`[server] bind = "0.0.0.0:3000"` and `[ssh] bind = "0.0.0.0:2222"`; if you change the HTTP
port, override the health check (`--health-cmd` / `healthcheck:`) to match.

Backups run in the live container:

```bash
docker exec rbit rbit backup --output /data/backups/$(date +%Y%m%d-%H%M%S)
```

`rbit user create` works while the server runs (it only needs the SQLite database).
The first administrator is also easy to create by signing up in the browser (the first
account becomes the administrator):

```bash
docker exec -i rbit rbit user create alice alice@example.com --admin
```

## systemd

[`deploy/rbit.service`](../deploy/rbit.service) runs rbit as a dedicated user with a
systemd-managed state directory (`/var/lib/rbit`) and a strict sandbox: the file system
is read-only except the data directory, no capabilities, restricted address families and
system calls. Installation steps are at the top of the file.

Download the archive for your platform from the
[GitHub release](https://github.com/GitRbit/rbit/releases), check it against `SHA256SUMS`
(`sha256sum --check --ignore-missing SHA256SUMS`) and unpack it; the archive also contains
`rbit.service` and `rbit.example.toml` (the paths below are the repository's, use the
archive's `rbit` and files instead of `target/release/rbit` and `deploy/`). Linux binaries
need glibc 2.35 or newer (Debian 12, Ubuntu 22.04, RHEL 10 and later); on older systems
build from source. In short:

```bash
sudo useradd --system --home-dir /var/lib/rbit --shell /usr/sbin/nologin rbit
sudo install -m 0755 target/release/rbit /usr/local/bin/rbit
sudo install -d -m 0750 -o root -g rbit /etc/rbit
sudo install -m 0640 -o root -g rbit deploy/rbit.example.toml /etc/rbit/rbit.toml
sudo install -m 0644 deploy/rbit.service /etc/systemd/system/rbit.service
sudo systemctl daemon-reload && sudo systemctl enable --now rbit
journalctl -u rbit -f
```

Run administrative commands as the service user so file ownership stays right.
`user create` and `backup` run while the server serves:

```bash
sudo -u rbit rbit --config /etc/rbit/rbit.toml user create alice alice@example.com --admin
```

The sandbox gives the service no capabilities, so it cannot bind ports below 1024. Keep
the SSH server on 2222 and forward port 22 to it with a firewall rule, or let it bind 22
directly by granting only that capability in an override (`sudo systemctl edit rbit`):

```ini
[Service]
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
AmbientCapabilities=CAP_NET_BIND_SERVICE
```

`SIGTERM` (what `systemctl stop` sends) shuts down gracefully: in-flight requests finish,
background jobs stop, and SQLite is flushed.

## Reverse proxy and TLS

The proxy terminates TLS and forwards to `127.0.0.1:3000`. Git pushes can be large and
clones long-running, so do not cap request bodies and do not buffer Git traffic.

Uploads stream to disk and are bounded by rbit itself, so the proxy must let them through
unbuffered:

| Upload | rbit's limit | Path |
|---|---|---|
| Git LFS objects | `[lfs] max_object_bytes` (2 GiB) | `/{owner}/{repo}[.git]/info/lfs/objects/{oid}` |
| Release files (web and API) | `[releases] max_asset_bytes` (512 MiB) | `/{owner}/{repo}/releases/{id}/assets`, `/api/v1/repos/{owner}/{repo}/releases/{id}/assets` |
| Files uploaded in the browser | `[editor] max_upload_bytes` per file (50 MiB), `max_upload_files` per commit | `/{owner}/{repo}/upload/…` |

With nginx, `client_max_body_size 0` and `proxy_request_buffering off` (as below) cover
all of them; a proxy that buffers request bodies to disk first doubles the I/O and may
time out on large LFS objects. Caddy streams request bodies by default.

### Troubleshooting large HTTPS pushes

rbit streams Git packfiles to `git receive-pack` without a request-body size limit or
the HTML/API request timeout. A push that fails around a repeatable byte count needs
checking at every proxy or CDN between the client and rbit, including any tunnel.
An HTTP `502` alone does not identify which hop failed: compare the proxy error log
with rbit's log at the same time, including whether the receive-pack POST reached rbit.
Temporarily set `RUST_LOG=info,rbit_transport=debug` on the server to include Git's
stderr and request-stream errors.

Compare a push through the public URL with one directly to the origin using a local
test repository and a temporary empty destination. On the server, use the loopback
HTTP listener; from another machine, use an origin HTTPS endpoint or SSH. Keep the
same source and refspec for both attempts. If only the proxied push fails, fix that
path's body limit, request buffering or timeout before retrying. If the direct origin
push fails too, check free disk space and the operating system's logs for process
termination or memory exhaustion.

[Cloudflare documents plan-dependent upload limits](https://developers.cloudflare.com/support/troubleshooting/http-status-codes/4xx-client-error/error-413/)
and a DNS-only endpoint as an alternative for larger requests. Changing nginx's
limit cannot raise a CDN's limit. Git sends a pack in one HTTP request; HTTP chunked
transfer encoding does not split it into independent uploads.

Shallow sources are a separate case. Git normally rejects updates that would require
the destination to add shallow roots, reporting `shallow update not allowed` and
leaving those refs unchanged. Fetch the missing history from the original upstream
with `git fetch --unshallow <original-upstream-url>` before retrying, or use a full
clone. A shallow clone can still push when the destination already has the required
history; rbit delegates that decision to Git's
[receive.shallowUpdate policy](https://git-scm.com/docs/git-config#Documentation/git-config.txt-receiveshallowUpdate).

Do not increase [`http.postBuffer`](https://git-scm.com/docs/git-config#Documentation/git-config.txt-httppostBuffer)
to bypass an upload limit: it changes client buffering and chunked encoding, and can
use substantial memory, but does not reduce the pack's size. When reporting a failure,
include the Git version, refspec, whether the source is shallow, elapsed time and
bytes written, and matching proxy/server errors. Check the remote's refs with
`git ls-remote --refs <url>` after the attempt to establish whether it updated them.

### Trusted proxies

rbit uses the client address for rate limiting and the audit log. It only believes
`X-Forwarded-For` / `Forwarded` / `X-Forwarded-Proto` from addresses listed in
`[security] trusted_proxies`. List exactly your proxy (for a proxy on the same host,
`["127.0.0.1/32", "::1/128"]`), never a broad range: any client inside a trusted range can
claim to be anyone. With no trusted proxies every request appears to come from the proxy,
which makes per-IP rate limits apply to everyone at once.

### Caddy

Caddy obtains and renews certificates automatically and sets `X-Forwarded-For` and
`X-Forwarded-Proto`.

```caddyfile
git.example.com {
	encode zstd gzip
	reverse_proxy 127.0.0.1:3000 {
		# Stream packfiles instead of buffering them.
		flush_interval -1
	}
	# Optional: keep metrics off the public internet entirely.
	@metrics path /metrics
	respond @metrics 404
}
```

### nginx

nginx opens a new connection to its upstream for every request unless the upstream keeps
idle connections and requests use HTTP/1.1 without `Connection: close`; the `upstream`
block and the two `proxy_*` lines below do that, so page loads do not pay for a TCP
handshake to rbit on each asset.

```nginx
upstream rbit {
    server 127.0.0.1:3000;
    keepalive 16;
}

server {
    listen 80;
    listen [::]:80;
    server_name git.example.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    listen [::]:443 ssl;
    http2 on;
    server_name git.example.com;

    ssl_certificate     /etc/letsencrypt/live/git.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/git.example.com/privkey.pem;
    ssl_protocols       TLSv1.2 TLSv1.3;
    add_header Strict-Transport-Security "max-age=63072000" always;

    # Pushes can be arbitrarily large; rbit streams them to git and enforces its own
    # limits on LFS objects, release files and browser uploads ([lfs], [releases], [editor]).
    client_max_body_size 0;

    location / {
        proxy_pass http://rbit;
        proxy_http_version 1.1;
        proxy_set_header Connection        "";
        proxy_set_header Host              $host;
        proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Stream Git and LFS traffic in both directions; allow slow, long clones.
        proxy_buffering         off;
        proxy_request_buffering off;
        proxy_read_timeout      1h;
        proxy_send_timeout      1h;
    }

    # Optional: metrics only from the monitoring host.
    location = /metrics {
        allow 10.0.0.5;
        deny all;
        proxy_pass http://rbit;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
    }
}
```

Get certificates with `certbot --nginx -d git.example.com` (or any ACME client).

### SSH

The built-in SSH server (`[ssh] enabled = true`, `127.0.0.1:2222` by default) is not
proxied over HTTP. Bind it to a public address (`bind = "0.0.0.0:2222"`) and expose the
port directly, or forward port 22 to it with a firewall rule. Binding port 22 itself under
systemd needs `CAP_NET_BIND_SERVICE` (see [systemd](#systemd)).

## Email

`[mail]` (SMTP, or `log` for development) is needed for password resets, address
verification and **notification emails**. With `transport = "disabled"` those flows explain
that email is unavailable and no notification is mailed; the in-app inbox works either
way.

Notification emails go only to verified addresses, are on by default for every user
(`[notifications] default_email = "participating"`; users change it under Settings →
Notifications) and are rate limited per address. Each carries `List-Unsubscribe` and
`List-Unsubscribe-Post` headers (RFC 8058) whose link is built from `server.public_url`, so
set it before mail goes out; the one-click `POST /email/unsubscribe/…` must reach rbit
from outside (mail providers send it). Set `[notifications] email = false` to send none.

Users can also choose a daily or weekly digest instead of one email per notification.
Digests go out at the user's chosen hour or at `[notifications] digest_hour` (UTC, default
8; administrators can change it under Site administration → Settings), weekly ones on
Mondays, grouped by repository and without threads already read in the app. They are
checked for every five minutes, so a digest leaves at most about five minutes after its hour
(later if the server was down: it catches up with one digest per user). Each period is
claimed in the database before its digest is sent, so a restart never sends one twice; a
digest the mail server refuses is not retried.

## External sign-in

Each `[[auth.providers]]` entry adds a "Sign in with …" button (see
[`deploy/rbit.example.toml`](../deploy/rbit.example.toml)). At the provider, register the
redirect URI `{public_url}/login/{id}/callback` (`id` is the provider's slug in the
configuration); `server.public_url` must be the address users reach, because it is also
sent in the authorization request.

- **Keycloak, Authentik, Entra ID and other OpenID Connect providers:** a confidential
  client with the standard (authorization code) flow; `issuer` is the URL whose
  `/.well-known/openid-configuration` the provider serves. PKCE `S256` is always sent.
  Google: an OAuth client of type "Web application", issuer `https://accounts.google.com`.
- **GitHub:** an OAuth App (or a GitHub App's OAuth credentials) with the callback URL
  above; rbit asks for `read:user user:email` and uses only the verified primary address.
  For GitHub Enterprise Server set `base_url`.
- **Secrets:** keep `client_secret` out of the file with
  `RBIT_AUTH__PROVIDERS__<N>__CLIENT_SECRET` (and `…__CLIENT_ID`), where `<N>` is the
  provider's position from 0 or its id in upper case with `-` as `_`
  (`RBIT_AUTH__PROVIDERS__COMPANY_SSO__CLIENT_SECRET`). Every other provider field must be
  in the file.
- Providers on loopback or private addresses, or with a plain `http` issuer (an
  on-premises IdP), need `[auth] allow_private_addresses = true`; otherwise provider
  requests are refused like internal webhook targets.
- **Don't change a provider's `id`** once people have signed in: linked identities are
  stored under it, and they would have to link again.
- The configuration is checked at start-up; rbit refuses to start and lists every problem.
  `allow_registration` (`true` follows the site's registration setting, `"always"` lets the
  provider decide who gets in, `false` never registers), `trust_email` (link a first
  sign-in to the account with the same verified address) and `trust_mfa` (skip rbit's
  two-factor step when the ID token reports MFA) are per provider.

## Health checks

| Endpoint | Answers | Use for |
|---|---|---|
| `GET /healthz` | `{"status":"ok","database":"ok","kv":"ok"}`; **503** with `"error"` for the failing store when SQLite or redb cannot be read | liveness / container `HEALTHCHECK` |
| `GET /readyz` | the same plus `"repositories"` (the repository directory is reachable) | load-balancer readiness |

Both are uncached, unauthenticated and cheap (one trivial read per store, bounded by a
3-second timeout).

## Metrics

With `[operations] metrics = true`, `GET /metrics` serves the Prometheus text format;
otherwise it answers 404. Set `metrics_token` to require `Authorization: Bearer <token>`,
or block `/metrics` at the proxy and scrape the loopback address.

| Metric | Type | Labels |
|---|---|---|
| `rbit_http_requests_total` | counter | `method`, `route` (matched route template, e.g. `/{owner}/{repo}/blob/{*rest}`, or `unmatched`), `status` (`2xx`…`5xx`) |
| `rbit_http_request_duration_seconds` | histogram | `method`, `route` — time until response headers; streamed Git bodies continue afterwards |
| `rbit_git_operations_total` | counter | `service` (`upload-pack` = clone/fetch, `receive-pack` = push), `stage` (`advertise`, `exchange`), `status` |
| `rbit_jobs_total` | counter | `job` (`warm_caches`, `git_gc`, `prune_insights_cache`), `outcome` (`ok`, `error`) |
| `rbit_job_duration_seconds` | histogram | `job` |
| `rbit_insights_cache_entries` | gauge | — (after each prune) |
| `rbit_webhook_attempts_total` | counter | `outcome` (`succeeded`, `retrying`, `failed`) — one per delivery attempt |
| `rbit_mirror_fetches_total` | counter | `outcome` (`ok`, `error`) |
| `rbit_archives_total` | counter | `format` (`.zip`, `.tar.gz`) |
| `rbit_sqlite_acquire_wait_seconds` | histogram | `mode` (`query` = one statement, `transaction`) — waiting for a pooled database connection |
| `rbit_sqlite_acquire_timeouts_total` | counter | `mode` — connection waits that gave up (30 s) |
| `rbit_sqlite_write_lock_wait_seconds` | histogram | `store` — `BEGIN IMMEDIATE`, waiting for the database's single write lock |
| `rbit_sqlite_write_transaction_seconds` | histogram | `store`, `outcome` (`commit`, `rollback`, `failed`) — from `BEGIN` to the end of the transaction |
| `rbit_sqlite_busy_errors_total` | counter | `store`, `phase` (`begin`, `query`, `transaction`, `commit`), `code` (`busy`, `busy_snapshot`, `busy_timeout`, `busy_recovery`, `locked`) |
| `rbit_notification_digests_total` | counter | `outcome` (`sent`, `failed`) — one per email digest handed to the mail transport |
| `rbit_actions_runs_total` | counter | `event` (`push`, `pull_request`, `workflow_dispatch`, `schedule`), `outcome` (`success`, `failure`, `cancelled`, `skipped`) — finished runs, Actions only |
| `rbit_actions_jobs_total` | counter | `outcome` — finished jobs |
| `rbit_actions_job_queue_seconds` | histogram | — from queued to started, when a runner takes a job |
| `rbit_actions_job_duration_seconds` | histogram | — from started to finished |
| `rbit_actions_rpc_total` | counter | `method` (`register`, `declare`, `poll`, `task_state`, `task_logs`, `unregister`, `artifact_list`, `artifact_upload`, `artifact_download`), `code` (`ok` or the protocol's error code) — runner calls |
| `rbit_actions_log_bytes_total` | counter | — job log bytes written |
| `rbit_actions_workflow_errors_total` | counter | `reason` (`parse`, `limit`, `plan`, `timeout`) — runs that failed before any job |
| `rbit_actions_jobs_waiting` | gauge | — jobs waiting for a runner (set once a minute) |
| `rbit_actions_runners_online` | gauge | — runners that called in the last 90 seconds (set once a minute) |

Labels never contain user-controlled values such as repository names or raw paths, so the
number of series is bounded.

```yaml
# prometheus.yml
scrape_configs:
  - job_name: rbit
    scheme: https
    authorization:
      credentials: replace-with-the-metrics-token
    static_configs:
      - targets: ["git.example.com"]
```

## Background jobs

rbit runs its own maintenance; no cron jobs are needed.

- **After every push**, the new commits are added to the repository's commit-graph, then
  the language breakdown and the root directory's last-commit list of the default branch
  are computed and cached (`warm_caches_on_push`).
- **Every `maintenance_interval_minutes`** (default 6 h, ± 10 % jitter): `git gc --auto`
  and `git commit-graph write --reachable --split` for every repository on the first run
  after start-up, afterwards only for repositories pushed to since the previous run; then
  the insights cache is pruned to
  `insights_cache_max_age_days` and `insights_cache_max_entries`. An idle instance does
  nothing else.
- At most `job_concurrency` jobs run at once. Expired sessions are purged hourly.
- **Webhook deliveries** run from a queue in SQLite: at most `[webhooks] concurrency` at
  once, retried with backoff up to `max_attempts`, and the delivery log is pruned daily to
  `delivery_retention_days`. Nothing is lost across restarts.
- **Pull mirrors** are fetched when due (checked every 30 s, at most two fetches at once).
- **Push mirrors** are pushed `debounce_seconds` after a push and every
  `[push_mirrors] interval_minutes` (checked every 30 s, at most two pushes at once).
- **Imports** run in the background, at most `[imports] max_concurrent` at once; an import
  a restart interrupted is marked failed at start-up (its token was never stored).
- **Release files** of a deleted repository are removed in the background; temporary
  files of uploads interrupted by a crash are swept at start-up.
- **Git LFS objects** no repository has used for a day are removed on the maintenance
  schedule; interrupted LFS uploads are swept at start-up.
- The notification email outbox is pruned with the inbox (`notification_retention_days`).

Webhooks, mirrors, imports and external sign-in make outbound HTTP(S) requests, so the
host needs outbound access to wherever users point them and to the configured providers.
Internal addresses are refused unless `allow_private_addresses` is set in `[webhooks]`,
`[mirrors]`, `[imports]`, `[push_mirrors]` or `[auth]`; set it only on a trusted instance (for example to reach a CI server or an
identity provider on the same network).

## Actions and runners

rbit Actions is off by default. With `[actions] enabled = true` (a restart is needed) rbit
plans workflow runs and hands their jobs to **rbit-runner**, a separate program shipped in the
same release archives and as the image `ghcr.io/gitrbit/rbit-runner`. The server never runs
builds itself. [docs/actions.md](actions.md) covers workflows and registering runners; this
section is about running them safely.

```toml
[actions]
enabled = true
enable_for_new_repositories = false
# The key that seals secrets; empty means <data_dir>/actions/secrets.key.
secrets_key_file = ""
log_retention_days = 90
artifact_retention_days = 90
max_artifact_bytes = 2147483648          # per artifact
max_artifact_storage_bytes = 10737418240 # per repository
instance_runners_for_all = false         # instance runners serve only owners you allow
```

**The secrets key.** rbit creates `actions/secrets.key` (mode `0600` on Unix) the first time it
starts with Actions on, and refuses a key group or others can read. It is never created while
sealed secrets exist, so a lost key cannot be replaced silently: restore it, or delete the
secrets and set them again. `rbit backup` does not copy it — the point of the key is to keep
secrets out of the database and its backups — so back it up separately, or point
`secrets_key_file` at a systemd credential or another secret store. On Windows, restrict the
data directory's ACLs.

**Runners.** Install `rbit-runner` on dedicated machines or VMs, not on the rbit server:

```bash
sudo useradd --system --create-home --home-dir /var/lib/rbit-runner --shell /usr/sbin/nologin rbit-runner
sudo usermod -aG docker rbit-runner   # container labels with rootful Docker
sudo install -m 0755 rbit-runner /usr/local/bin/rbit-runner
sudo -u rbit-runner sh -c 'cd /var/lib/rbit-runner && read -rs RBIT_RUNNER_TOKEN && export RBIT_RUNNER_TOKEN &&   rbit-runner register --url https://git.example.com --labels ubuntu-latest:container:docker.io/library/ubuntu:24.04'
sudo rbit-runner --config /var/lib/rbit-runner/runner.toml install --user rbit-runner
```

**Running in the background.** `rbit-runner install` registers the runner with the
system's own service manager, starts it, and has it start again at boot or logon and
restart when it fails. `uninstall`, `start`, `stop` and `status` take the same `--mode` and
`--name`; `--config` picks the runner (its directory holds `runner.toml`, `.credential` and
the work directory, and must stay where it is: `uninstall` before moving it).

| `--mode` | Linux | macOS | Windows |
|---|---|---|---|
| `service` (default) | systemd unit in `/etc/systemd/system` | LaunchDaemon in `/Library/LaunchDaemons` | Windows Service (session 0) |
| `desktop` | systemd `--user` unit | LaunchAgent in `~/Library/LaunchAgents` | Task Scheduler task at logon |

- `service` starts at boot, before anyone logs in, and needs root (an elevated prompt on
  Windows). It runs as `--user`: a system account such as `rbit-runner` on Linux and macOS
  (root when omitted), and `NT AUTHORITY\NETWORK SERVICE` on Windows unless `--user` names
  another account, whose password is read from `RBIT_RUNNER_SERVICE_PASSWORD`. On Windows,
  `install` grants that account the right to change the runner's directory, as GitHub's
  runner does. The service is named `rbit-runner.<runner name>` (`--name` changes it) and
  logs to the journal on Linux, and to `runner.log` next to the configuration on macOS and
  Windows.
- `desktop` runs in the session of the user who installs it, from when they log in until
  they log off. Pick it when jobs need that desktop: UI and browser tests that open windows,
  screenshots, code signing with a key in the user's keychain or certificate store, or
  anything that talks to the logged-in session. A Windows Service has no desktop at all
  (session 0), and a LaunchDaemon cannot reach the logged-in user's GUI. The machine must
  stay logged in (set it to log in automatically) for the runner to run.

Every mode restarts the runner when it fails (systemd `Restart=on-failure`, launchd
`KeepAlive`, the Windows Service's recovery actions, the task's restart on failure) and stops
it gracefully: polling stops at once and running tasks get `shutdown_grace_seconds` (10
minutes by default) before they are cancelled. systemd and launchd send SIGTERM and wait that
long plus a minute; the Windows Service turns the service manager's stop and pre-shutdown
notice into the same stop and reports itself stopping meanwhile; `rbit-runner stop` of a
desktop task leaves a stop request in the work directory, and logging off stops it.
`deploy/rbit-runner.service` remains an example unit for hosts set up by hand.

`deploy/rbit-runner.example.toml` documents `runner.toml`: capacity, the work directory, labels
and the container limits (memory, CPUs, pids). The container image keeps `runner.toml`, the
credential and the work directory in its `/var/lib/rbit-runner` volume.

**Isolation.** Each label runs its jobs one way:

- `container` (Docker or Podman, through their CLI): each job gets its own container and
  network, `--security-opt no-new-privileges`, pids, memory and CPU limits, and never
  `--privileged`, the Docker socket or host networking. Secrets reach containers through
  `--env-file`s that are deleted after each step, never through command lines. Rootless Podman
  is the strongest setup; a container escape is a kernel or runtime bug, so prefer one runner
  host or VM per trust level, and ephemeral runners (`rbit-runner run --once`) for public
  repositories.
  The runner works on a job's files from the host only through checks that refuse links,
  pipes and device nodes (containers also drop `MKNOD`), and a job's environment variables
  never reach the container CLI's own process unless they are harmless there.

  What containers can still reach: the network the runner host is on, including a cloud
  provider's metadata endpoint (`169.254.169.254`), which hands out the VM's credentials. Block
  it for the runner's networks (an `iptables` rule on the bridge, or the provider's setting that
  turns the endpoint off or requires a token), and keep the VM's role free of permissions jobs
  should not have. `memory` has no default: set one, or a single job can exhaust the host.
- `host`: jobs run as the runner's user, with no isolation — they can read the runner's
  credential and anything else that user can. `rbit-runner` warns at registration and at every
  start, and the runners pages say "host — no isolation". Use it only for repository runners of
  repositories you trust.

Pull requests from forks and from people without write access get a read-only token, no
secrets and no variables, and wait for a writer's approval. Instance runners (registered under
**Administration → Runners**) serve only the owners an administrator allows, unless
`instance_runners_for_all` is set.

**Network.** Runners connect to rbit over HTTPS (plain HTTP only with `--insecure-http`, for a
server on the same machine). rbit makes no outbound connections for Actions.

**Storage.** Job logs (gzipped once a job ends) and artifacts live under `actions/` in the data
directory and are not backed up; the sweeper deletes them after their retention and keeps the
instance within `max_log_storage_bytes` by expiring the oldest finished runs' logs first.

## Backups

`rbit backup --output DIR` writes a consistent backup to a **new** directory while the
server keeps running:

```text
DIR/
├── rbit.db                          SQLite snapshot (VACUUM INTO: one consistent read)
├── repositories/<owner-id>/<repo-id>.git   bare mirror of every repository in that snapshot
├── attachments/<repo-id>/<asset-id>   every release file listed in that snapshot
├── lfs/<aa>/<bb>/<oid>                every Git LFS object listed in that snapshot
├── secret.key                         the secret key, when it lives in the data directory
└── manifest.json                      format (4), version, time, counts; written last
```

The backup directory is created `0700` and the database, key and manifest `0600`: the
backup holds password hashes and the key to every stored credential. Keep it as private as
the data directory. With `storage.secret_key_file` set, the key is deliberately **not**
copied (the manifest says where it is): back it up separately, ideally not next to the
database backups.

The repositories are mirrored through Git's pack protocol, which copies each repository's
refs and every object they reach consistently even during a push or `git gc`. A
repository created after the database snapshot is not included (it would be orphaned on
restore anyway); one deleted in between is listed under `missing_repositories`.

Release files are copied as they are: a file is renamed into place only once its upload
is complete and never changes afterwards. Uploads still in progress (`attachments/tmp`)
are skipped, and a file whose release was deleted after the snapshot is listed under
`missing_attachments`. Git LFS objects are content-addressed and never change either;
one removed after the snapshot (no repository used it any more) is listed under
`missing_lfs_objects`.

**The redb store (`rbit.redb`) is not backed up.** It holds only login sessions and
caches. After a restore everyone signs in again and caches refill on demand.

A backup is complete when `manifest.json` exists. The command exits non-zero, without a
manifest, if any repository could not be copied.

Schedule it with a systemd timer or cron, as the service user, and copy the result off the
machine (restic, borg, rsync, object storage):

```bash
sudo install -d -o rbit -g rbit -m 0750 /var/backups/rbit   # once
sudo -u rbit rbit --config /etc/rbit/rbit.toml backup \
  --output /var/backups/rbit/$(date +%Y%m%d-%H%M%S)
```

### Restore

The backup directory mirrors the data directory, so restoring is a copy:

1. Stop rbit: `sudo systemctl stop rbit` (or `docker stop rbit`).
2. Move the current data directory aside: `sudo mv /var/lib/rbit /var/lib/rbit.old`.
3. Copy the backup into place, excluding the manifest, and fix ownership:
   ```bash
   sudo mkdir -m 0750 /var/lib/rbit
   sudo cp -a DIR/rbit.db DIR/secret.key DIR/repositories DIR/attachments DIR/lfs /var/lib/rbit/
   sudo chown -R rbit:rbit /var/lib/rbit
   ```
   For Docker, copy into the volume, e.g.
   `docker run --rm -v rbit-data:/data -v "$PWD/DIR":/backup:ro debian:bookworm-slim sh -c 'cp -a /backup/rbit.db /backup/secret.key /backup/repositories /backup/attachments /backup/lfs /data/ && chown -R 10001:10001 /data'`.
   Backups in format 3 have no `secret.key`, format 2 no `lfs/` (and format 1 no
   `attachments/`): leave out what is not there. With `storage.secret_key_file` set,
   restore the key from where you keep it instead.
4. Start rbit. Pending migrations (if the backup came from an older version) run on
   start-up; a fresh `rbit.redb` is created.
5. Check `/healthz`, sign in, browse and clone a repository, then delete the old directory.

Restore onto the **same or a newer** rbit version. Restoring a backup into an older
version is not supported (its database may have migrations the older binary does not know).

## Upgrades and migrations

1. Take a backup (`rbit backup`).
2. Replace the binary (or pull/rebuild the image).
3. Restart. Database migrations are embedded in the binary and applied automatically at
   start-up, before the server accepts requests. To apply them separately (for example
   in a deploy pipeline), run `rbit migrate` while the server is stopped; it migrates
   and exits.

Migrations only move forward. To roll back, stop the new version, restore the backup
taken in step 1 and start the old binary. The [changelog](../CHANGELOG.md) calls out
anything else, such as the one-time sign-out when upgrading to milestone 1.
