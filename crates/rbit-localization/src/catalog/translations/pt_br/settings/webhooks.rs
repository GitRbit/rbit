//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/webhooks.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::webhooks::{WebhookMessage, WebhookText};

entries! { PtBr:
    WebhookMessage => MessageWriter |text| {}
    WebhookText<'a> => MessageWriter |text| {}
}
