//! English labels and help of the configuration fields, part 1.

config_fields! {
    "updates" {
        "check_for_updates" => "Check for software updates",
            "Contact the public GitHub release feed at most daily after success. No running version or instance identity is sent. Restart required.";
        "report_version" => "Report the running version",
            "Expose the software version in the public footer, version API, SSH banner and outbound integration User-Agents. Disable to hide it; administrators retain local diagnostics. Restart required.";
    }
    "server" {
        "bind" => "Listen address",
            "Address the HTTP server binds to.";
        "public_url" => "Public URL",
            "Public base URL, used for clone URLs and the CSRF origin check.";
        "site_name" => "Default site name",
            "Default name before live branding overrides. Change the active name in Branding and policies.";
        "open_registration" => "Open registration by default",
            "Default registration policy. The live policy in Branding and policies takes precedence.";
        "explore_enabled" => "Offer the Explore page by default",
            "Default for the Explore page, which lists every public repository. The live setting in Branding and policies takes precedence.";
    }
    "storage" {
        "data_dir" => "Data directory",
            "Read-only here. Stop the server, move the data and update the deployment file or environment.";
        "secret_key_file" => "Secret key file",
            "Read-only here. The key that encrypts stored credentials; empty means secret.key in the data directory. Back it up with the database.";
    }
    "security" {
        "session_lifetime_days" => "Session lifetime (days)",
            "How long a session stays valid without activity, in days.";
        "session_max_age_days" => "Maximum session age (days)",
            "How long a session stays valid at most, however active, in days. 0 means no limit.";
        "min_password_length" => "Min password length",
            "Minimum password length accepted at sign-up.";
        "max_password_jobs" => "Maximum password jobs",
            "Simultaneous hashing or verification jobs in the shared password-work budget (1–64; default 2). Restart required.";
        "trusted_proxies" => "Trusted proxy networks",
            "Reverse proxies (CIDR networks, e.g. 127.0.0.1/32, 10.0.0.0/8, ::1/128) whose X-Forwarded-For, Forwarded and X-Forwarded-Proto headers are believed. Empty (the default) means the TCP peer is always the client.";
        "client_ip_header" => "Client address header",
            "A header in which a trusted proxy reports the client's address, such as CF-Connecting-IP behind Cloudflare or Cloudflare Tunnel. Believed only from the trusted proxy networks above, ahead of X-Forwarded-For. Empty uses X-Forwarded-For alone.";
        "rate_limit" => "Enable rate limiting",
            "Throttle sign-in, sign-up, two-factor codes, Git credentials and form posts per client. Only worth turning off for load tests.";
        "hsts" => "Send HSTS",
            "Send Strict-Transport-Security (one year) when the public URL is https://, so browsers refuse plain HTTP to this host. Turn off only while moving away from HTTPS.";
        "hsts_include_subdomains" => "HSTS includes subdomains",
            "Add includeSubDomains to that header, binding every subdomain of the host to HTTPS as well.";
        "external_images" => "Allow external images",
            "Let pages show images from other sites, which also tells those sites who viewed the page. Off limits images to this instance and inline data: images.";
    }
    "repository" {
        "default_branch" => "Default branch",
            "Branch that HEAD points to in newly created repositories.";
        "git_binary" => "Git executable",
            "Path to the git executable used by the smart-HTTP transport.";
        "max_render_bytes" => "Max render (bytes)",
            "Files larger than this are not rendered inline in the browser.";
        "max_git_processes" => "Max Git processes",
            "Git transport processes (clones, fetches and pushes over HTTP and SSH) running at once; further requests are refused (503 over HTTP).";
        "max_git_requests_per_ip" => "Max Git requests per ip",
            "Concurrent smart-HTTP Git requests from one IP address (429 beyond). Behind a reverse proxy this needs trusted proxy networks.";
        "git_idle_timeout_seconds" => "Git idle timeout (seconds)",
            "Seconds a smart-HTTP Git exchange may go without traffic in either direction.";
        "git_max_seconds" => "Git max duration (seconds)",
            "Upper bound, in seconds, on a single smart-HTTP Git exchange (a very large clone or push).";
        "max_push_bytes" => "Max push (bytes)",
            "Largest pack one push may send, over HTTP and SSH. 0 means unlimited.";
        "max_archives_per_ip" => "Max archives per ip",
            "Source archive downloads one IP address may have running at once.";
        "archive_idle_timeout_seconds" => "Archive idle timeout (seconds)",
            "Seconds an archive download may stall (the client not reading, or git not writing).";
        "archive_max_seconds" => "Archive max duration (seconds)",
            "Upper bound, in seconds, on one archive download.";
    }
    "mail" {
        "transport" => "Transport",
            "Deployment setting applied after restarting the server.";
        "from_address" => "Sender email address",
            "The From: address of every message.";
        "from_name" => "Sender display name",
            "The display name in From:. Blank means the site name.";
        "smtp_host" => "SMTP server",
            "Host name of the SMTP relay (transport = \"smtp\").";
        "smtp_port" => "SMTP port",
            "Leave blank for port 587 with STARTTLS or no encryption, or port 465 with implicit TLS.";
        "smtp_security" => "SMTP encryption",
            "Deployment setting applied after restarting the server.";
        "smtp_username" => "SMTP username",
            "Credentials for the relay. Blank means no authentication.";
        "smtp_password" => "SMTP password",
            "Deployment setting applied after restarting the server.";
        "timeout_seconds" => "Timeout (seconds)",
            "Give up on a message that has not been delivered after this many seconds.";
    }
    "ssh" {
        "enabled" => "Enable this feature",
            "Run the built-in SSH server.";
        "bind" => "Listen address",
            "Address the SSH server binds to.";
        "public_host" => "Public host",
            "Host name shown in SSH clone URLs. Defaults to the host of server.public_url.";
        "public_port" => "Public port",
            "Port shown in SSH clone URLs, when a proxy or port mapping differs from bind.";
        "host_key_path" => "Host key path",
            "Path to the SSH host key file. Blank uses ssh/host_ed25519 inside the data directory; generated on first start if missing.";
        "max_connections" => "Max connections",
            "Concurrent connections across all clients; further connections are closed at once.";
        "max_connections_per_ip" => "Max connections per ip",
            "Concurrent connections from one IP address.";
        "auth_timeout_seconds" => "Auth timeout (seconds)",
            "Seconds a client has to authenticate after connecting.";
        "idle_timeout_seconds" => "Idle timeout (seconds)",
            "Seconds without any traffic before a connection is closed.";
        "max_session_seconds" => "Max session (seconds)",
            "Upper bound, in seconds, on a single Git operation (a very large clone or push).";
    }
    "auth" {
        "timeout_seconds" => "Timeout (seconds)",
            "Upper bound for one request to a provider.";
        "allow_private_addresses" => "Allow private network destinations",
            "Allow providers whose addresses are loopback, private, link-local or unique-local, and issuers or base URLs over plain http.";
    }
    "branding" {
        "primary_color" => "Primary color",
            "Primary button background in #RRGGBB format. Set both colors or leave both blank.";
        "primary_text_color" => "Primary text color",
            "Primary button text in #RRGGBB format. Choose a readable contrast.";
        "description" => "Description",
            "Shown to visitors on this instance.";
        "welcome_title" => "Welcome title",
            "Shown to visitors on this instance.";
        "welcome_message" => "Welcome message",
            "Shown to visitors on this instance.";
        "logo_url" => "Logo url",
            "Shown to visitors on this instance.";
        "favicon_url" => "Favicon url",
            "Shown to visitors on this instance.";
        "app_icon_url" => "App icon url",
            "Shown to visitors on this instance.";
        "footer_text" => "Footer text",
            "Shown to visitors on this instance.";
        "support_url" => "Support url",
            "Shown to visitors on this instance.";
        "terms_url" => "Terms url",
            "Shown to visitors on this instance.";
        "privacy_url" => "Privacy url",
            "Shown to visitors on this instance.";
        "show_powered_by" => "Show powered by",
            "Shown to visitors on this instance.";
        "show_version" => "Show version",
            "Shown to visitors on this instance.";
        "show_features" => "Show features",
            "Shown to visitors on this instance.";
    }
    "notifications" {
        "email" => "Send notification emails",
            "Whether notifications are also sent by email.";
        "default_email" => "Default email subscription",
            "The preference of users who never chose one.";
        "digest_hour" => "Default digest hour (UTC)",
            "The hour, 0 to 23 in UTC, daily and weekly email digests are sent at for users who did not choose one. Site administration → Settings overrides it at runtime.";
    }
    "operations" {
        "metrics" => "Enable metrics endpoint",
            "Expose Prometheus metrics at /metrics.";
        "metrics_token" => "Metrics access token",
            "When set, /metrics requires Authorization: Bearer <token>. Leave unset only if the endpoint is not reachable from untrusted networks.";
        "warm_caches_on_push" => "Warm caches on push",
            "Warm the language and last-commit caches of the default branch after each push, so the first visitor after a push does not pay for the computation.";
        "maintenance_interval_minutes" => "Maintenance interval (minutes)",
            "Minutes between maintenance runs (git gc --auto for repositories pushed to since the last run, then insights cache pruning). 0 disables periodic maintenance.";
        "job_concurrency" => "Job concurrency",
            "Upper bound on background jobs (cache warming, git gc) running at the same time.";
        "insights_cache_max_age_days" => "Insights cache max age (days)",
            "Insights cache entries older than this are removed by maintenance. 0 keeps entries regardless of age.";
        "insights_cache_max_entries" => "Insights cache max entries",
            "When the insights cache holds more entries than this, maintenance removes the oldest. 0 means no limit.";
    }
    "api" {
        "enabled" => "Enable this feature",
            "Serve /api/v1. When off, every API path answers 404.";
        "default_page_size" => "Default page size",
            "Items per page when the request does not say.";
        "max_page_size" => "Max page size",
            "Upper bound for ?per_page=.";
    }
}
