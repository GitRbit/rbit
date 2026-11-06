//! Label catalog ownership shared by issues and pull requests.

mod models;
mod validation;

pub use models::{Label, LabelFields, LabelId, LabelInput, LabelSnapshot, LabelUsage};
pub use validation::{
    DEFAULT_LABELS, MAX_LABEL_DESCRIPTION_CHARS, MAX_LABEL_NAME_CHARS, MAX_LABELS_PER_REPO, label_fields, normalise_color,
};

#[cfg(feature = "applications")]
mod application;
#[cfg(feature = "applications")]
mod repository;
#[cfg(feature = "applications")]
pub use application::{LabelAccess, LabelApplication, LabelError};
#[cfg(feature = "applications")]
pub use repository::LabelRepository;
