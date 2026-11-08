//! The access tokens settings page.

labels! {
    /// Fixed labels of `/settings/tokens`.
    TokensMessage, "settings.tokens" {
        Title = "title" => "Access tokens",
        IntroLead = "intro-lead" => "Use a token instead of your password for Git over HTTPS: enter your username",
        IntroUsername = "intro-username" => "and paste the token when Git asks for a password. The REST API at",
        IntroApi = "intro-api" => "takes it as",
        IntroPrefix = "intro-prefix" => ". Tokens start with",
        IntroLegacy = "intro-legacy" => "; tokens from earlier releases start with",
        IntroTail = "intro-tail" => "and keep working.",
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
