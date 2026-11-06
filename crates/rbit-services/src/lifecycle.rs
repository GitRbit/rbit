//! Feature lifecycle composition drains admitted session finalization before stores close.

use crate::{ActionsService, Services};

impl Services {
    /// Starts background subscribers and periodic work. Call once, after start-up.
    pub fn spawn_background(&self) {
        self.updates.spawn_background();
        self.jobs.spawn_background();
        self.recovery.spawn_background();
        self.webhooks.spawn_background();
        self.releases.spawn_background();
        self.mirrors.spawn_background();
        self.issues.spawn_background();
        self.notifications.spawn_background();
        self.pulls.spawn_background();
        self.lfs.spawn_background();
        // Milestone 3: importing and push mirrors (M).
        self.imports.spawn_background();
        self.push_mirrors.spawn_background();
        // Actions: foundation (F).
        if let Some(actions) = &self.actions {
            actions.spawn_background();
        }
    }

    /// Actions background tasks running now: always 0 with Actions disabled.
    pub fn actions_background_tasks(&self) -> usize {
        self.actions.as_ref().map_or(0, ActionsService::background_tasks)
    }

    /// Stops background work (jobs, webhook deliveries, mirror fetches, release clean-up,
    /// commit references, notifications, merges) and waits according to each owner's
    /// drain contract. Recovery finishes its current operation without an implicit timeout.
    /// Call during graceful shutdown, before closing the stores.
    pub async fn shutdown(&self) {
        self.passwords.close();
        self.accounts.close_session_effects();
        tracing::info!(
            admitted_password_attempts = self.passwords.admitted(),
            "password admission closed; admitted attempts retain their slots until completion"
        );
        tokio::join!(
            self.accounts.shutdown_session_effects(),
            self.updates.shutdown(),
            self.recovery.shutdown(),
            self.jobs.shutdown(),
            self.webhooks.shutdown(),
            self.releases.shutdown(),
            self.mirrors.shutdown(),
            self.issues.shutdown(),
            self.notifications.shutdown(),
            self.pulls.shutdown(),
            self.lfs.shutdown(),
            // Milestone 3: importing and push mirrors (M).
            self.imports.shutdown(),
            self.push_mirrors.shutdown(),
            // Actions: foundation (F).
            async {
                if let Some(actions) = &self.actions {
                    actions.shutdown().await;
                }
            },
        );
    }
}
