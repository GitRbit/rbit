#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if data.len() <= 2 * 1024 * 1024 {
        let (archive, valid) = rbit_plugin_packages_fuzz::structured(data);
        let accepted = rbit_plugin_packages_fuzz::check(&archive);
        assert_eq!(accepted, valid, "structured acceptance oracle");
    }
});
