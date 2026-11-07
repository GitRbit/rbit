//! Metadata observations must never invoke expensive Argon verification.
use rbit_identity::inventory::{Algorithm, Classification, Inventory, Limits, classify};
const VECTOR: &str = "$argon2id$v=19$m=256,t=2,p=2$c29tZXNhbHQ$bQk8UB/VmZZF4Oo79iDXuL5/0ttZwg2f/5U52iv1cDc";
#[test]
fn published_legacy_costs_and_lengths_are_observed_without_passwords() {
    let Classification::Parameters { parameters, salt_bytes, digest_bytes } = classify(VECTOR.as_bytes(), 1024) else {
        panic!("expected parameters");
    };
    assert_eq!(parameters.algorithm, Algorithm::Argon2id);
    assert_eq!((parameters.version, parameters.memory_kib, parameters.iterations, parameters.parallelism), (Some(19), 256, 2, 2));
    assert_eq!((salt_bytes, digest_bytes), (8, 32));
    // Parsing a cost that would require terabytes of work memory must remain cheap.
    let huge = VECTOR.replace("m=256,t=2,p=2", "m=4294967295,t=4294967295,p=1");
    assert!(matches!(classify(huge.as_bytes(), 1024), Classification::Parameters { .. }));
}
#[test]
fn version_algorithm_defaults_and_invalid_inputs_remain_distinct() {
    for algorithm in ["argon2d", "argon2i", "argon2id"] {
        for version in ["v=16$", "v=19$", ""] {
            let hash = VECTOR.replace("argon2id", algorithm).replace("v=19$", version);
            assert!(matches!(classify(hash.as_bytes(), 1024), Classification::Parameters { .. }));
        }
    }
    assert_eq!(classify(b"$argon2id$v=19$m=256,t=2,p=2", 1024), Classification::Incomplete);
    assert_eq!(classify(VECTOR.replace("argon2id", "scrypt").as_bytes(), 1024), Classification::Unsupported);
    assert_eq!(classify(VECTOR.replace("v=19", "v=42").as_bytes(), 1024), Classification::Unsupported);
    for bytes in [b"privatePHC".as_slice(), b"\xff", VECTOR.replace("m=256", "m=0").as_bytes()] {
        assert_eq!(classify(bytes, 1024), Classification::Malformed);
    }
    assert_eq!(classify(&[b'x'; 4097], usize::MAX), Classification::Oversized);
}
#[test]
fn aggregate_bounds_preserve_counts_and_never_retain_raw_values() {
    let limits = Limits { rows: 6, groups: 1, prefix_bytes: 128 };
    let mut inventory = Inventory::new(limits).unwrap();
    for text in [VECTOR.to_owned(), VECTOR.to_owned(), VECTOR.replace("t=2", "t=3"), "private-secret".to_owned()] {
        assert!(inventory.observe(text.as_bytes(), Some(text.len() as u64)));
    }
    assert!(inventory.observe(b"", Some(10000)));
    assert!(inventory.observe(b"", None));
    assert!(!inventory.observe(VECTOR.as_bytes(), Some(VECTOR.len() as u64)));
    let report = inventory.finish();
    assert!(!report.complete);
    assert!(report.row_limit_reached);
    assert_eq!((report.rows, report.counts.parameters_observed, report.counts.groups_omitted), (6, 3, 1));
    assert_eq!((report.counts.malformed, report.counts.oversized, report.counts.invalid_storage), (1, 1, 1));
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].rows, 2);
    let serialized = serde_json::to_string(&report).unwrap();
    for secret in [VECTOR, "private-secret", "c29tZXNhbHQ", "bQk8UB"] {
        assert!(!serialized.contains(secret));
    }
}
#[test]
fn truncated_adapter_input_and_invalid_limits_cannot_look_complete() {
    let mut inventory = Inventory::new(Limits::default()).unwrap();
    inventory.observe(b"$argon2id", Some(100));
    assert_eq!(inventory.finish().counts.incomplete, 1);
    for limits in [
        Limits { rows: 0, ..Limits::default() },
        Limits { groups: 0, ..Limits::default() },
        Limits { prefix_bytes: 4097, ..Limits::default() },
    ] {
        assert!(Inventory::new(limits).is_none());
    }
}

#[test]
fn omitted_cost_fields_use_pinned_parser_defaults_without_inventing_cost_policy() {
    let hash = VECTOR.replace("m=256,t=2,p=2$", "");
    let Classification::Parameters { parameters, .. } = classify(hash.as_bytes(), 1024) else {
        panic!("default metadata");
    };
    let defaults = argon2::Params::default();
    assert_eq!(
        (parameters.memory_kib, parameters.iterations, parameters.parallelism),
        (defaults.m_cost(), defaults.t_cost(), defaults.p_cost())
    );
}
