//! Whether Actions runs on this instance (`[actions] enabled`). Disabled Actions has no
//! routes, workers or writes: composition builds the Actions service, its runner protocol,
//! pages, API endpoints and background tasks only through [`Availability::provide`], so
//! when it is off none of them exists, rather than each answering "disabled". Webhooks
//! keep stored Actions subscriptions, but no form offers those events and none may be added.

/// Whether Actions is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Enabled,
    Disabled,
}

impl Availability {
    /// From `[actions] enabled`.
    pub const fn of(enabled: bool) -> Self {
        if enabled { Self::Enabled } else { Self::Disabled }
    }

    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }

    /// Builds what exists only while Actions is on: the service, its routes and workers.
    pub fn provide<T>(self, build: impl FnOnce() -> T) -> Option<T> {
        self.is_enabled().then(build)
    }

    /// `enabled` while Actions is on, else `disabled`: the API document and route set served.
    pub fn choose<T>(self, enabled: T, disabled: T) -> T {
        if self.is_enabled() { enabled } else { disabled }
    }

    /// Whether a webhook may subscribe to an event; `actions_event`: one of the Actions events.
    pub const fn offers_event(self, actions_event: bool) -> bool {
        self.is_enabled() || !actions_event
    }

    /// Why an Actions event named `name` cannot be subscribed to while Actions is off.
    pub fn refused_event(name: &str) -> String {
        format!("\"{name}\" needs Actions, which this instance does not have")
    }
}

impl From<bool> for Availability {
    /// `[actions] enabled`.
    fn from(enabled: bool) -> Self {
        Self::of(enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::Availability;

    #[test]
    fn disabled_actions_provides_nothing_and_offers_no_actions_events() {
        assert_eq!(Availability::of(false).provide(|| 1), None);
        assert_eq!(Availability::of(true).provide(|| 1), Some(1));
        assert_eq!((Availability::Enabled.choose("a", "b"), Availability::Disabled.choose("a", "b")), ("a", "b"));
        assert!(Availability::Disabled.offers_event(false) && !Availability::Disabled.offers_event(true));
        assert!(Availability::Enabled.offers_event(true));
        assert_eq!(Availability::refused_event("workflow_run"), "\"workflow_run\" needs Actions, which this instance does not have");
    }
}
