//! The SSH keys settings page.

labels! {
    /// Fixed labels of `/settings/keys`.
    KeysMessage, "settings.keys" {
        Title = "title" => "SSH keys",
        NewKey = "new" => "New SSH key",
        TitleLabel = "title-label" => "Title",
        Optional = "optional" => "optional",
        TitlePlaceholder = "title-placeholder" => "e.g. Work laptop",
        TitleHint = "title-hint" => "Defaults to the key's comment.",
        PublicKey = "public-key" => "Public Key",
        KeyPlaceholder = "key-placeholder" => "ssh-ed25519 AAAA… you@example.com",
        KeyHintLead = "key-hint-lead" => "Ed25519, ECDSA, RSA (2048 bits or more) or a security key (",
        KeyHintMiddle = "key-hint-middle" => "). Create one with",
        KeyHintTail = "key-hint-tail" => ".",
        AddKey = "add" => "Add key",
        YourKeys = "list" => "Your keys",
        EmptyTitle = "empty-title" => "No SSH keys",
        EmptyText = "empty-text" => "Add a public key to use Git over SSH.",
        Delete = "delete" => "Delete",
        TestLead = "test-lead" => "Check that a key works with",
        TestTail = "test-tail" => ".",
    }
}

messages! {
    /// Labels of `/settings/keys` that name a key.
    KeyLabel<'a>, "settings.keys" {
        Delete(title: &'a str) = "delete-label" => ["Delete ", {title}],
    }
}

/// SSH key page sentences that embed markup; the template renders each slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeysSentence {
    /// Under the title. Slot 0: the usual public key path.
    Intro,
}

impl KeysSentence {
    /// This sentence in `locale`, or its English text marked as a fallback.
    pub fn rich(self, locale: impl std::borrow::Borrow<crate::Locale>) -> crate::catalog::collab::rich::Rich {
        crate::catalog::collab::rich::render(*locale.borrow(), self)
    }
}

impl crate::catalog::collab::rich::RichFamily for KeysSentence {
    fn id(self) -> &'static str {
        match self {
            Self::Intro => "settings.keys.intro",
        }
    }

    fn english(self, text: &mut crate::catalog::collab::rich::RichWriter) {
        match self {
            Self::Intro => {
                text.literal(
                    "Keys let you clone, fetch and push over SSH without a password. Paste the contents of your public key file, usually ",
                );
                text.slot(0);
                text.literal(".");
            }
        }
    }

    fn translate(self, locale: crate::Locale, text: &mut crate::catalog::collab::rich::RichWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
