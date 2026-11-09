//! One-click unsubscribe tokens: `{account id}.{HMAC-SHA256 of the id}` keyed with a
//! server secret. Unforgeable, with nothing stored per message; composition owns the key.
use hmac::{Hmac, KeyInit, Mac};
use rbit_contracts::AccountId;
use sha2::Sha256;

/// The `server_secrets` row whose key signs unsubscribe links.
pub const LINK_KEY_NAME: &str = "notification_unsubscribe";
/// What the link MAC covers besides the account id, so the key can sign other things later.
const LINK_PURPOSE: &str = "rbit/notification-unsubscribe/v1";

type LinkMac = Hmac<Sha256>;

fn link_mac(key: &[u8], account: AccountId) -> LinkMac {
    // HMAC accepts keys of any length; this cannot fail.
    let mut mac = <LinkMac as KeyInit>::new_from_slice(key).unwrap_or_else(|_| unreachable!("HMAC takes any key length"));
    mac.update(LINK_PURPOSE.as_bytes());
    mac.update(b"\0");
    mac.update(account.0.to_string().as_bytes());
    mac
}

/// `{account id}.{lower-case hex MAC}`.
pub fn sign_link(key: &[u8], account: AccountId) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let tag = link_mac(key, account).finalize().into_bytes();
    let mut token = format!("{}.", account.0);
    for byte in tag {
        token.push(char::from(DIGITS[usize::from(byte >> 4)]));
        token.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    token
}

/// The account a token was signed for, if the MAC matches (compared in constant time).
pub fn verify_link(key: &[u8], token: &str) -> Option<AccountId> {
    let (id, hex) = token.split_once('.')?;
    if id.is_empty() || id.len() > 19 || !id.bytes().all(|b| b.is_ascii_digit()) || id.starts_with('0') {
        return None;
    }
    let account = AccountId(id.parse().ok()?);
    if hex.len() != 64 {
        return None;
    }
    let tag: Vec<u8> = (0..32).map(|i| u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()).collect::<Option<_>>()?;
    link_mac(key, account).verify_slice(&tag).ok().map(|()| account)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"0123456789abcdef0123456789abcdef";

    #[test]
    fn links_verify_only_unchanged() {
        let token = sign_link(KEY, AccountId(42));
        assert!(token.starts_with("42."));
        assert_eq!(token.len(), 3 + 64);
        assert_eq!(verify_link(KEY, &token), Some(AccountId(42)));

        assert_eq!(verify_link(b"another key", &token), None, "another server's key");
        let other_account = token.replacen("42.", "43.", 1);
        assert_eq!(verify_link(KEY, &other_account), None, "the MAC binds the account");
        let mut flipped = token.clone();
        let last = flipped.pop().unwrap();
        flipped.push(if last == '0' { '1' } else { '0' });
        assert_eq!(verify_link(KEY, &flipped), None, "a changed MAC");
        for malformed in ["", "42", "42.", ".abc", "042.00", "x.y", &token[..token.len() - 2], &format!("{token}00"), "-1.00"] {
            assert_eq!(verify_link(KEY, malformed), None, "{malformed:?}");
        }
        let upper = token.to_uppercase();
        assert_eq!(verify_link(KEY, &upper), Some(AccountId(42)), "hex case does not matter");
    }

    #[test]
    fn the_token_format_is_pinned() {
        // A fixed vector: changing the purpose, separator or encoding breaks every link already mailed.
        // Independently computed: HMAC-SHA256(KEY, "rbit/notification-unsubscribe/v1\0" + "7").
        let expected = "7.675323528826ec50f7ba3f9c378ef6e3ecb33198e91be5755b55267a1dd8a06f";
        assert_eq!(sign_link(KEY, AccountId(7)), expected);
        assert_eq!(verify_link(KEY, expected), Some(AccountId(7)));
    }
}
