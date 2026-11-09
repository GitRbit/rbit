//! The profile, appearance and notification settings pages.

labels! {
    /// Fixed labels of `/settings/profile`.
    ProfileMessage, "settings.profile" {
        Title = "title" => "Profile",
        IntroLead = "intro-lead" => "Shown on",
        IntroLink = "intro-link" => "your profile page",
        IntroTail = "intro-tail" => "and next to your activity. Everything here is public and optional.",
        Updated = "updated" => "Profile updated.",
        DisplayName = "display-name" => "Display name",
        DisplayNameHint = "display-name-hint" => "Shown instead of your username where there is room.",
        Bio = "bio" => "Bio",
        BioHint = "bio-hint" => "Up to 300 characters.",
        Website = "website" => "Website",
        WebsitePlaceholder = "website-placeholder" => "https://",
        Location = "location" => "Location",
        Save = "save" => "Save profile",
    }
}

labels! {
    /// Fixed labels of `/settings/appearance`.
    AppearanceMessage, "settings.appearance" {
        Title = "title" => "Appearance",
        Intro = "intro" => "Your theme is saved to your account and follows you to every device you sign in on. Hover a theme to preview it; click to keep it.",
    }
}

labels! {
    /// Fixed labels of `/settings/notifications`.
    NotificationsMessage, "settings.notifications" {
        Title = "title" => "Notifications",
        IntroLead = "intro-lead" => "Everything still appears in",
        IntroLink = "intro-link" => "your inbox",
        IntroTail = "intro-tail" => ". Choose what is also sent to your email address.",
        Saved = "saved" => "Preferences saved.",
        Unavailable = "unavailable" => "Email is not available on this server, so notifications only appear in your inbox.",
        VerifyLead = "verify-lead" => "Verify ",
        VerifyMiddle = "verify-middle" => " in ",
        VerifyLink = "verify-link" => "account settings",
        VerifyTail = "verify-tail" => " to receive notification emails.",
        Email = "email" => "Email",
        SendTo = "send-to" => "Send to",
        When = "when" => "When",
        DigestTime = "digest-time" => "Digest time",
        DigestHint = "digest-hint" => "Digests leave out threads you have already read here. Every email has a link that turns them off in one click.",
        Save = "save" => "Save preferences",
        Off = "off" => "Off",
        OffText = "off-text" => "Notifications appear in your inbox only.",
        Participating = "participating" => "Participating",
        ParticipatingText = "participating-text" => "Threads you opened, commented on or reviewed, and when you are mentioned, assigned or asked for a review.",
        Everything = "everything" => "Everything",
        EverythingText = "everything-text" => "Also threads you subscribed to without taking part.",
        Immediately = "immediately" => "Immediately",
        ImmediatelyText = "immediately-text" => "One email for each notification, as it happens.",
        Daily = "daily" => "Daily digest",
        DailyText = "daily-text" => "One email a day with everything since the last one.",
        Weekly = "weekly" => "Weekly digest",
        WeeklyText = "weekly-text" => "One email every Monday with everything from the week before.",
    }
}

messages! {
    /// Notification settings text with arguments.
    NotificationsText<'a>, "settings.notifications" {
        DefaultHour(hour: &'a str) = "default-hour" => ["Default (", {hour}, ")"],
    }
}

labels! {
    /// Fixed labels of the theme gallery on `/settings/appearance`.
    GalleryMessage, "settings.appearance.gallery" {
        Show = "show" => "Show",
        Search = "search" => "Search themes",
        BuiltIn = "built-in" => "Built in",
        Classic = "classic" => "rbit Classic",
        Empty = "empty" => "No themes match your search.",
        CreditLink = "credit-link" => "tinted-theming",
        Licence = "licence" => "Licence and credits",
    }
}

messages! {
    /// Theme gallery text with arguments.
    GalleryText, "settings.appearance.gallery" {
        SearchCount(total: u64) = "search-count" => ["Search ", (total), " themes"],
    }
}

/// Theme gallery sentences that embed markup; the template renders each slot.
#[derive(Clone, Copy, Debug)]
pub enum GallerySentence {
    /// Under the gallery. Slot 0: the tinted-theming link, worded by `GalleryMessage::CreditLink`; slot 1: the licence link, worded by `GalleryMessage::Licence`.
    Credit,
}

impl GallerySentence {
    /// This sentence in `locale`, or its English text marked as a fallback.
    pub fn rich(self, locale: impl std::borrow::Borrow<crate::Locale>) -> crate::catalog::collab::rich::Rich {
        crate::catalog::collab::rich::render(*locale.borrow(), self)
    }
}

impl crate::catalog::collab::rich::RichFamily for GallerySentence {
    fn id(self) -> &'static str {
        match self {
            Self::Credit => "settings.appearance.gallery.credit",
        }
    }

    fn english(self, text: &mut crate::catalog::collab::rich::RichWriter) {
        match self {
            Self::Credit => {
                text.literal("Tinted themes are generated from the ");
                text.slot(0);
                text.literal(" base16 and base24 schemes (MIT); hover a theme to see its author. ");
                text.slot(1);
                text.literal(".");
            }
        }
    }

    fn translate(self, locale: crate::Locale, text: &mut crate::catalog::collab::rich::RichWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

/// Profile and notification settings sentences that embed markup; the template renders each slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileSentence {
    /// Under the profile title. Slot 0: the profile link, worded by `ProfileMessage::IntroLink`.
    ProfileIntro,
    /// Under the notification settings title. Slot 0: the inbox link, worded by `NotificationsMessage::IntroLink`.
    NotificationsIntro,
}

impl ProfileSentence {
    /// This sentence in `locale`, or its English text marked as a fallback.
    pub fn rich(self, locale: impl std::borrow::Borrow<crate::Locale>) -> crate::catalog::collab::rich::Rich {
        crate::catalog::collab::rich::render(*locale.borrow(), self)
    }
}

impl crate::catalog::collab::rich::RichFamily for ProfileSentence {
    fn id(self) -> &'static str {
        match self {
            Self::ProfileIntro => "settings.profile.intro",
            Self::NotificationsIntro => "settings.notifications.intro",
        }
    }

    fn english(self, text: &mut crate::catalog::collab::rich::RichWriter) {
        match self {
            Self::ProfileIntro => {
                text.literal("Shown on ");
                text.slot(0);
                text.literal(" and next to your activity. Everything here is public and optional.");
            }
            Self::NotificationsIntro => {
                text.literal("Everything still appears in ");
                text.slot(0);
                text.literal(". Choose what is also sent to your email address.");
            }
        }
    }

    fn translate(self, locale: crate::Locale, text: &mut crate::catalog::collab::rich::RichWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
