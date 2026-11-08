//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/profile.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::profile::{
    AppearanceMessage, GalleryMessage, GalleryText, NotificationsMessage, NotificationsText, ProfileMessage,
};

entries! { ZhHans:
    ProfileMessage => MessageWriter |text| {}
    AppearanceMessage => MessageWriter |text| {}
    NotificationsMessage => MessageWriter |text| {}
    NotificationsText<'a> => MessageWriter |text| {}
    GalleryMessage => MessageWriter |text| {}
    GalleryText => MessageWriter |text| {}
}
