//! English labels and help of the configuration fields, part 2.

config_fields! {
    "webhooks" {
        "enabled" => "Enable this feature",
            "Deliver webhooks at all. When off, hooks can still be configured but nothing is sent.";
        "allow_private_addresses" => "Allow private network destinations",
            "Allow targets that resolve to loopback, private, link-local or unique-local addresses.";
        "timeout_seconds" => "Timeout (seconds)",
            "Upper bound for one delivery attempt (connect, send, read the response status).";
        "max_attempts" => "Max attempts",
            "Attempts per delivery, including the first, before it is marked failed.";
        "concurrency" => "Concurrency",
            "Deliveries in flight at once across the instance.";
        "delivery_retention_days" => "Delivery retention (days)",
            "Days the delivery log keeps requests and responses.";
    }
    "releases" {
        "max_asset_bytes" => "Max asset (bytes)",
            "Largest attached file, in bytes.";
        "max_assets_per_release" => "Max assets per release",
            "Attached files per release.";
        "upload_timeout_seconds" => "Upload timeout (seconds)",
            "Upper bound, in seconds, for one upload.";
    }
    "mirrors" {
        "enabled" => "Enable this feature",
            "Allow creating mirrors and run scheduled fetches.";
        "default_interval_minutes" => "Default interval (minutes)",
            "Fetch interval for new mirrors.";
        "min_interval_minutes" => "Min interval (minutes)",
            "The shortest interval a mirror may be configured with.";
        "fetch_timeout_seconds" => "Fetch timeout (seconds)",
            "Upper bound for one fetch.";
        "allow_private_addresses" => "Allow private network destinations",
            "Allow upstream URLs that resolve to loopback, private, link-local or unique-local addresses.";
    }
    "push_mirrors" {
        "enabled" => "Enable this feature",
            "Allow configuring push mirrors and push to them.";
        "interval_minutes" => "Interval (minutes)",
            "Push every mirror at least this often.";
        "debounce_seconds" => "Debounce (seconds)",
            "Delay after a push before mirrors are pushed.";
        "push_timeout_seconds" => "Push timeout (seconds)",
            "Upper bound for one git push.";
        "allow_private_addresses" => "Allow private network destinations",
            "Allow remotes that resolve to loopback, private, link-local or unique-local addresses.";
        "allow_local_urls" => "Allow local file remotes (testing only)",
            "Admit file:// remotes (tests only; see the module docs).";
    }
    "imports" {
        "enabled" => "Enable this feature",
            "Offer importing on the \"New repository\" page.";
        "allow_private_addresses" => "Allow private network destinations",
            "Allow sources that resolve to loopback, private, link-local or unique-local addresses.";
        "max_concurrent" => "Max concurrent",
            "Imports running at the same time.";
        "git_timeout_seconds" => "Git timeout (seconds)",
            "Upper bound for the Git copy.";
        "api_timeout_seconds" => "Api timeout (seconds)",
            "Upper bound for one API request.";
        "rate_limit_max_wait_seconds" => "Rate limit max wait (seconds)",
            "Longest wait for the source's rate limit to reset.";
        "author_mapping" => "Match imported authors",
            "How authors on the source become local accounts.";
    }
    "issues" {
        "scan_commit_limit" => "Scan commit limit",
            "Commits scanned per pushed ref update for references and closing keywords.";
        "notifications" => "Notifications",
            "Whether in-app notifications are produced and shown.";
        "notification_retention_days" => "Notification retention (days)",
            "Days a read notification is kept.";
    }
    "pulls" {
        "merge_timeout_seconds" => "Merge timeout (seconds)",
            "Upper bound, in seconds, for one merge or trial merge: all of its git work, from creating the scratch repository to the base repository's fetch and ref update.";
        "max_concurrent_merges" => "Max concurrent merges",
            "Merges running at once across the instance (always one per base repository).";
        "mergeability_concurrency" => "Mergeability concurrency",
            "Trial merges for mergeability running at once.";
        "max_commits" => "Max commits",
            "Commits shown per pull request.";
        "noreply_domain" => "No-reply email domain",
            "Domain used for no-reply authors in generated commits. Blank uses noreply followed by your public host name.";
    }
    "editor" {
        "max_upload_bytes" => "Max upload (bytes)",
            "Largest uploaded file, in bytes.";
        "max_upload_files" => "Max upload files",
            "Files per upload.";
        "upload_timeout_seconds" => "Upload timeout (seconds)",
            "Upper bound, in seconds, for one upload request.";
    }
    "lfs" {
        "enabled" => "Enable this feature",
            "Serve the LFS API and the SSH git-lfs-authenticate command.";
        "max_object_bytes" => "Max object (bytes)",
            "Largest object, in bytes.";
        "token_lifetime_seconds" => "Token lifetime (seconds)",
            "Lifetime of the signed transfer headers, in seconds.";
    }
    "actions" {
        "enabled" => "Enable this feature",
            "Run workflows on rbit-runner. When off, Actions is fully absent: no pages, no runner endpoint, no background work, and workflow files are ignored. Restart required.";
        "enable_for_new_repositories" => "Enable for new repositories",
            "Turn Actions on for repositories created from now on. Existing repositories keep their own setting.";
        "secrets_key_file" => "Secrets key file",
            "Encryption key for Actions secrets. Blank uses actions/secrets.key in the data directory. Back it up separately.";
        "log_retention_days" => "Log retention (days)",
            "Days job logs are kept.";
        "run_retention_days" => "Run retention (days)",
            "Days finished runs are kept.";
        "max_log_bytes" => "Max log (bytes)",
            "Log bytes kept per task; later lines are dropped.";
        "max_workflow_bytes" => "Max workflow file (bytes)",
            "Largest workflow file read, in bytes.";
        "zombie_timeout_seconds" => "Heartbeat timeout (seconds)",
            "A running task without a heartbeat for this long fails.";
        "endless_timeout_seconds" => "Task time limit (seconds)",
            "A task running for this long fails.";
        "abandoned_timeout_seconds" => "Queue time limit (seconds)",
            "A waiting job no runner picks up for this long is cancelled.";
        "evaluation_concurrency" => "Evaluation concurrency",
            "Workflow evaluations (Git reads and YAML parsing) running at once, 1 to 16.";
        "instance_runners_for_all" => "Instance runners serve everyone",
            "Let instance runners run jobs of every owner. When off, they serve only owners an administrator allowed.";
        "max_log_storage_bytes" => "Log storage budget (bytes)",
            "Log bytes kept across the instance; beyond it the oldest finished runs lose their logs first.";
        "max_waiting_jobs_per_repository" => "Max waiting jobs per repository",
            "New runs beyond this many waiting jobs fail with a readable error.";
        "max_runs_per_event" => "Max runs per event",
            "Runs one push or pull request event may start.";
        "max_jobs_per_event" => "Max jobs per event",
            "Jobs one event may create across its runs.";
        "max_schedules_per_repository" => "Max schedules per repository",
            "Scheduled workflow entries per repository.";
        "long_poll_seconds" => "Runner long poll (seconds)",
            "How long a runner's poll for work is held open, 1 to 25.";
        "max_polls" => "Max runner polls",
            "Runner polls held open at once; beyond it polls are refused until one ends.";
        "max_artifact_bytes" => "Max artifact size (bytes)",
            "Largest artifact a job may upload.";
        "max_artifact_storage_bytes" => "Artifact storage per repository (bytes)",
            "Artifact bytes kept per repository; uploads beyond it are refused. Backups (format 5 and later) include artifacts.";
        "artifact_retention_days" => "Artifact retention (days)",
            "Days artifacts are kept. A workflow's retention-days may only lower it.";
        "max_cache_storage_bytes" => "Cache storage per repository (bytes)",
            "actions/cache bytes kept per repository for all its runners; the least recently used entries go first. Backups (format 5 and later) include cache entries.";
        "cache_retention_days" => "Cache retention (days)",
            "Days a cache entry is kept without being restored.";
    }
    "provider" {
        "kind" => "Provider type",
            "Deployment setting applied after restarting the server.";
        "id" => "Provider ID",
            "A unique short ID used in callback URLs. Changing it requires entering the provider secret again.";
        "name" => "Display name",
            "Shown on buttons and in settings.";
        "issuer" => "OIDC issuer URL",
            "OIDC issuer URL (kind = \"oidc\" only).";
        "base_url" => "GitHub Enterprise URL",
            "GitHub Enterprise Server URL (kind = \"github\" only); default https://github.com.";
        "client_id" => "Client ID",
            "Deployment setting applied after restarting the server.";
        "client_secret" => "Client secret",
            "Never logged or shown (see the Debug implementation).";
        "scopes" => "Requested scopes",
            "Enter one scope per line. Use provider defaults to request its recommended scopes.";
        "allow_registration" => "Account creation",
            "Deployment setting applied after restarting the server.";
        "trust_email" => "Trust verified email addresses",
            "Link a first sign-in to the one account whose verified email equals the provider's verified email. Only for providers that control their users' addresses.";
        "trust_mfa" => "Trust provider multi-factor authentication",
            "Skip the account's own two-factor step when the ID token's amr claim says the provider performed multi-factor authentication (\"mfa\"). OIDC only.";
    }
}
