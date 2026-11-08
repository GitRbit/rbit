//! Locale-independent kind of a finalized page change, recorded for history display.

/// Presentation metadata of one finalized generation. It never grants authority and is
/// absent for generations published before the kind was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WikiChange {
    Create,
    Edit,
    Rename,
    Delete,
    Restore,
}

impl WikiChange {
    /// Stable storage/event code, independent of the presentation locale.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Edit => "edit",
            Self::Rename => "rename",
            Self::Delete => "delete",
            Self::Restore => "restore",
        }
    }

    /// Strict inverse of [`Self::code`]; unknown codes are refused, never guessed.
    pub fn from_code(code: &str) -> Option<Self> {
        [Self::Create, Self::Edit, Self::Rename, Self::Delete, Self::Restore].into_iter().find(|change| change.code() == code)
    }
}

#[cfg(test)]
mod tests {
    use super::WikiChange;

    #[test]
    fn codes_round_trip_and_unknown_codes_are_refused() {
        for change in [WikiChange::Create, WikiChange::Edit, WikiChange::Rename, WikiChange::Delete, WikiChange::Restore] {
            assert_eq!(WikiChange::from_code(change.code()), Some(change));
        }
        assert_eq!(WikiChange::from_code("Edit"), None);
        assert_eq!(WikiChange::from_code(""), None);
    }
}
