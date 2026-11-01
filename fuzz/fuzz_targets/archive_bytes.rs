#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if data.len() <= 2 * 1024 * 1024 {
        rbit_plugin_packages_fuzz::check(data);
    }
});
