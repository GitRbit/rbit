//! The access tokens settings page.

labels! {
    /// Fixed labels of `/settings/tokens`.
    TokensMessage, "settings.tokens" {
        Title = "title" => "Access tokens",
        CopyNow = "copy-now" => "Copy your new token now. You won't be able to see it again.",
        CopyToken = "copy-token" => "Copy token",
        Copy = "copy" => "Copy",
        NewToken = "new" => "New token",
        Name = "name" => "Name",
        NamePlaceholder = "name-placeholder" => "e.g. Work laptop",
        NameHint = "name-hint" => "Something to recognise it by later.",
        Scope = "scope" => "Scope",
        ScopeHint = "scope-hint" => "A token can never do more than your account can.",
        Expiration = "expiration" => "Expiration",
        Generate = "generate" => "Generate token",
        YourTokens = "list" => "Your tokens",
        EmptyTitle = "empty-title" => "No access tokens",
        EmptyText = "empty-text" => "Generate a token to clone, fetch and push over HTTPS without your password.",
        Revoke = "revoke" => "Revoke",
        NoExpiration = "no-expiration" => "No expiration",
        OneDay = "one-day" => "1 day",
        NeverUsed = "never-used" => "Never used",
        ReadDescription = "read-description" => "Clone and fetch; read through the API.",
        WriteDescription = "write-description" => "Also push, create repositories and star through the API.",
        AdminDescription = "admin-description" => "Also change repository settings and webhooks, and delete repositories through the API.",
    }
}

messages! {
    /// Access token text with arguments.
    TokenText<'a>, "settings.tokens" {
        Created(name: &'a str) = "created" => ["Token “", {name}, "” created"],
        RevokeLabel(name: &'a str) = "revoke-label" => ["Revoke ", {name}],
        Days(days: u64) = "days" => [(days), " days"],
        CreatedOn(date: &'a str) = "created-on" => ["Created ", {date}],
        LastUsed(when: &'a str) = "last-used" => ["Last used ", {when}],
        Expires(date: &'a str) = "expires" => ["Expires ", {date}],
        Expired(date: &'a str) = "expired" => ["Expired ", {date}],
    }
}

/// Access token page sentences that embed markup; the template renders each slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokensSentence {
    /// Under the title. Slot 0: the username; 1: the API path; 2: the authorization header; 3: the token prefix; 4: the earlier releases' prefix.
    Intro,
}

impl TokensSentence {
    /// This sentence in `locale`, or its English text marked as a fallback.
    pub fn rich(self, locale: impl std::borrow::Borrow<crate::Locale>) -> crate::catalog::collab::rich::Rich {
        crate::catalog::collab::rich::render(*locale.borrow(), self)
    }
}

impl crate::catalog::collab::rich::RichFamily for TokensSentence {
    fn id(self) -> &'static str {
        match self {
            Self::Intro => "settings.tokens.intro",
        }
    }

    fn english(self, text: &mut crate::catalog::collab::rich::RichWriter) {
        match self {
            Self::Intro => {
                text.literal("Use a token instead of your password for Git over HTTPS: enter your username ");
                text.slot(0);
                text.literal(" and paste the token when Git asks for a password. The REST API at ");
                text.slot(1);
                text.literal(" takes it as ");
                text.slot(2);
                text.literal(". Tokens start with ");
                text.slot(3);
                text.literal("; tokens from earlier releases start with ");
                text.slot(4);
                text.literal(" and keep working.");
            }
        }
    }

    fn translate(self, locale: crate::Locale, text: &mut crate::catalog::collab::rich::RichWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
