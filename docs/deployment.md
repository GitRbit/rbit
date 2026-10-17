# Deploying Rbit

Rbit is one binary plus the `git` executable. All state lives in one data directory:

| Path (under `data_dir`) | Contents | Back up? |
|---|---|---|
| `rbit.db` (+ `-wal`, `-shm`) | SQLite: accounts, repositories, tokens, settings, audit log | **Yes** |
| `repositories/<owner-id>/<repo-id>.git` | Bare Git repositories | **Yes** |
| `attachments/<repo-id>/<asset-id>` | Files attached to releases | **Yes** (not `attachments/tmp`) |
| `rbit.redb` | Login sessions and rebuildable caches | No (see [Backups](#backups)) |

Run it behind a reverse proxy that terminates TLS. The sections below cover
[Docker](#docker), [systemd](#systemd), [reverse proxies](#reverse-proxy-and-tls),
[health checks](#health-checks), [metrics](#metrics), [background jobs](#background-jobs),
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
- `repository.git_binary` — the `git` used for smart HTTP, `git gc` and backups.

## Docker

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

[`deploy/rbit.service`](../deploy/rbit.service) runs Rbit as a dedicated user with a
systemd-managed state directory (`/var/lib/rbit`) and a strict sandbox: the file system
is read-only except the data directory, no capabilities, restricted address families and
system calls. Installation steps are at the top of the file; in short:

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

### Trusted proxies

Rbit uses the client address for rate limiting and the audit log. It only believes
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

```nginx
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

    # Pushes can be arbitrarily large; Rbit streams them to git and enforces its own
    # limit on release uploads ([releases] max_asset_bytes).
    client_max_body_size 0;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_http_version 1.1;
        proxy_set_header Host              $host;
        proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Stream Git traffic in both directions; allow slow, long clones.
        proxy_buffering         off;
        proxy_request_buffering off;
        proxy_read_timeout      1h;
        proxy_send_timeout      1h;
    }

    # Optional: metrics only from the monitoring host.
    location = /metrics {
        allow 10.0.0.5;
        deny all;
        proxy_pass http://127.0.0.1:3000;
    }
}
```

Get certificates with `certbot --nginx -d git.example.com` (or any ACME client).

### SSH

The built-in SSH server (`[ssh] enabled = true`, `127.0.0.1:2222` by default) is not
proxied over HTTP. Bind it to a public address (`bind = "0.0.0.0:2222"`) and expose the
port directly, or forward port 22 to it with a firewall rule. Binding port 22 itself under
systemd needs `CAP_NET_BIND_SERVICE` (see [systemd](#systemd)).

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

Rbit runs its own maintenance; no cron jobs are needed.

- **After every push**, the language breakdown and the root directory's last-commit list
  of the default branch are computed and cached (`warm_caches_on_push`).
- **Every `maintenance_interval_minutes`** (default 6 h, ± 10 % jitter): `git gc --auto`
  for every repository on the first run after start-up, afterwards only for repositories
  pushed to since the previous run; then the insights cache is pruned to
  `insights_cache_max_age_days` and `insights_cache_max_entries`. An idle instance does
  nothing else.
- At most `job_concurrency` jobs run at once. Expired sessions are purged hourly.
- **Webhook deliveries** run from a queue in SQLite: at most `[webhooks] concurrency` at
  once, retried with backoff up to `max_attempts`, and the delivery log is pruned daily to
  `delivery_retention_days`. Nothing is lost across restarts.
- **Pull mirrors** are fetched when due (checked every 30 s, at most two fetches at once).
- **Release files** of a deleted repository are removed in the background; temporary
  files of uploads interrupted by a crash are swept at start-up.

Webhooks and mirrors make outbound HTTP(S) requests, so the host needs outbound access to
wherever users point them. Internal addresses are refused unless
`allow_private_addresses` is set in `[webhooks]` or `[mirrors]`; set it only on a trusted
instance (for example to reach a CI server on the same network).

## Backups

`rbit backup --output DIR` writes a consistent backup to a **new** directory while the
server keeps running:

```text
DIR/
├── rbit.db                          SQLite snapshot (VACUUM INTO: one consistent read)
├── repositories/<owner-id>/<repo-id>.git   bare mirror of every repository in that snapshot
├── attachments/<repo-id>/<asset-id>   every release file listed in that snapshot
└── manifest.json                      format, version, time, counts; written last
```

The repositories are mirrored through Git's pack protocol, which copies each repository's
refs and every object they reach consistently even during a push or `git gc`. A
repository created after the database snapshot is not included (it would be orphaned on
restore anyway); one deleted in between is listed under `missing_repositories`.

Release files are copied as they are: a file is renamed into place only once its upload
is complete and never changes afterwards. Uploads still in progress (`attachments/tmp`)
are skipped, and a file whose release was deleted after the snapshot is listed under
`missing_attachments`.

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

1. Stop Rbit: `sudo systemctl stop rbit` (or `docker stop rbit`).
2. Move the current data directory aside: `sudo mv /var/lib/rbit /var/lib/rbit.old`.
3. Copy the backup into place, excluding the manifest, and fix ownership:
   ```bash
   sudo mkdir -m 0750 /var/lib/rbit
   sudo cp -a DIR/rbit.db DIR/repositories DIR/attachments /var/lib/rbit/
   sudo chown -R rbit:rbit /var/lib/rbit
   ```
   For Docker, copy into the volume, e.g.
   `docker run --rm -v rbit-data:/data -v "$PWD/DIR":/backup:ro debian:bookworm-slim sh -c 'cp -a /backup/rbit.db /backup/repositories /backup/attachments /data/ && chown -R 10001:10001 /data'`.
4. Start Rbit. Pending migrations (if the backup came from an older version) run on
   start-up; a fresh `rbit.redb` is created.
5. Check `/healthz`, sign in, browse and clone a repository, then delete the old directory.

Restore onto the **same or a newer** Rbit version. Restoring a backup into an older
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
