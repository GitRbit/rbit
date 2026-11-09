//! The stored deployment configuration document: its settings key and opening its
//! sealed secret fields. Shared by the service editor and the server's startup loader;
//! the document rules themselves live in `rbit_core::config::document`.

use rbit_core::Config;
use serde_json::Value;

use crate::SealingKey;

pub use crate::sqlite::instance::DEPLOYMENT_CONFIG_KEY;

/// Parses a saved configuration document, opening its sealed secret fields.
pub fn open_stored(document: &str, key: &SealingKey) -> Result<Config, String> {
    let mut value: Value = serde_json::from_str(document).map_err(|error| error.to_string())?;
    key.open_secret_fields(&mut value).map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}
