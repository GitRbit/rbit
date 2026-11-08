//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/webhooks.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::webhooks::{WebhookMessage, WebhookText};

entries! { ZhHans:
    WebhookMessage => MessageWriter |text| {}
    WebhookText<'a> => MessageWriter |text| {}
}
