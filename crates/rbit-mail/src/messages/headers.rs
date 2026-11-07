//! Ordered threading and one-click unsubscribe header values.

/// Optional headers of a message. Message ids are written with their angle brackets
/// (`<owner/repo/issues/1@host>`).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct MailHeaders {
    /// `Message-ID`; the relay assigns one when `None`.
    pub message_id: Option<String>,
    /// `In-Reply-To` and `References`: the message this one continues.
    pub in_reply_to: Option<String>,
    /// `List-Unsubscribe` (RFC 2369) with `List-Unsubscribe-Post: List-Unsubscribe=One-Click`
    /// (RFC 8058): an HTTPS link that unsubscribes with a single `POST`.
    pub one_click_unsubscribe: Option<String>,
}

impl MailHeaders {
    /// Each header as `(name, value)`, in the order they are written.
    pub fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        if let Some(id) = &self.message_id {
            pairs.push(("Message-ID", id.clone()));
        }
        if let Some(parent) = &self.in_reply_to {
            pairs.push(("In-Reply-To", parent.clone()));
            pairs.push(("References", parent.clone()));
        }
        if let Some(link) = &self.one_click_unsubscribe {
            pairs.push(("List-Unsubscribe", format!("<{link}>")));
            pairs.push(("List-Unsubscribe-Post", "List-Unsubscribe=One-Click".to_owned()));
        }
        pairs
    }
}

impl std::fmt::Debug for MailHeaders {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("MailHeaders").finish_non_exhaustive()
    }
}
