#![cfg(not(unix))]
use rbit_plugin_packages::{ImageError, NativeImageStore};
#[test]
fn unsupported_host_does_not_probe_or_provision_a_root() {
    assert_eq!(NativeImageStore::open("nonexistent-native-image-root").unwrap_err(), ImageError::Unsupported);
}
