use rbit_i18n::{LanguagePack, MAX_ARCHIVE_BYTES, PackError, PackManifest, PackSet};
use rbit_messages::{Argument, ArgumentKind, English, MessageRef, MessageSchema, MessageSpec, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
    sync::Arc,
};
use zip::{ZipWriter, write::SimpleFileOptions};

fn schema() -> Arc<MessageSchema> {
    Arc::new(
        MessageSchema::new(vec![
            spec("hello", "Hello {name}", &[("name", ArgumentKind::Text)]),
            spec("count", "{count} items", &[("count", ArgumentKind::Number)]),
            spec("fallback", "English fallback", &[]),
        ])
        .unwrap(),
    )
}
fn spec(key: &str, english: &str, args: &[(&str, ArgumentKind)]) -> MessageSpec {
    MessageSpec {
        namespace: "core".into(),
        key: key.into(),
        english: english.into(),
        args: args.iter().map(|(name, kind)| ((*name).into(), *kind)).collect(),
    }
}
fn manifest(locale: &str, direction: &str) -> PackManifest {
    PackManifest {
        schema: 1,
        id: format!("rbit.language.{}", locale.to_lowercase()),
        version: "1.0.0".parse().unwrap(),
        locale: locale.into(),
        direction: direction.into(),
        parent: None,
        message_schema: "^1.0".parse().unwrap(),
        publisher: "Rbit language contributors".into(),
        license: "MIT".into(),
        license_file: "LICENSE".into(),
        catalogs: BTreeMap::from([("core".into(), "catalogs/core.ftl".into())]),
        checksums: BTreeMap::new(),
    }
}
fn archive(mut manifest: PackManifest, text: &str) -> Vec<u8> {
    let files = [("LICENSE", "Permission to use.".as_bytes()), ("catalogs/core.ftl", text.as_bytes())];
    manifest.checksums = files.iter().map(|(path, bytes)| ((*path).into(), hex_digest(bytes))).collect();
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).unix_permissions(0o644);
    writer.start_file("pack.toml", options).unwrap();
    writer.write_all(toml::to_string(&manifest).unwrap().as_bytes()).unwrap();
    for (path, bytes) in files {
        writer.start_file(path, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn pack(manifest: PackManifest, text: &str, schema: &MessageSchema) -> Arc<LanguagePack> {
    Arc::new(LanguagePack::parse(&archive(manifest, text), schema).unwrap())
}
fn set(packs: Vec<Arc<LanguagePack>>, schema: &Arc<MessageSchema>, revision: u64) -> PackSet {
    PackSet::new(packs, Arc::clone(schema), Arc::new(English::new(Arc::clone(schema))), revision).unwrap()
}
fn text(snapshot: &rbit_messages::LocaleSnapshot, key: &str, args: &[Argument<'_>]) -> String {
    snapshot.format(&MessageRef { namespace: "core", key, args })
}

#[test]
fn interpolation_is_text_and_unknown_messages_fall_back_to_english() {
    let schema = schema();
    let pack = pack(manifest("fr", "ltr"), "hello = Bonjour { $name }\n", &schema);
    let set = set(vec![pack], &schema, 7);
    let french = set.snapshot("fr");
    let output = text(&french, "hello", &[Argument { name: "name", value: Value::Text("<script>alert(1)</script>") }]);
    assert!(output.contains("<script>alert(1)</script>")); // Caller must HTML-escape text.
    assert!(output.starts_with("Bonjour"));
    assert_eq!(text(&french, "fallback", &[]), "English fallback");
    assert_eq!(french.revision(), 7);
    assert_eq!(french.lang(), "fr");
}

#[test]
fn typed_numeric_arguments_drive_plural_and_exact_variants() {
    let schema = schema();
    let pack = pack(manifest("fr", "ltr"), "count = { $count ->\n [0] aucun\n [one] un\n *[other] { $count } éléments\n}\n", &schema);
    let french = set(vec![pack], &schema, 1).snapshot("fr");
    for (count, expected) in [(0.0, "aucun"), (1.0, "un"), (8.0, "8")] {
        assert!(text(&french, "count", &[Argument { name: "count", value: Value::Number(count) }]).contains(expected));
    }
}

#[test]
fn rtl_and_immutable_replacement_keep_old_snapshot_coherent() {
    let schema = schema();
    let old = pack(manifest("ar", "rtl"), "fallback = قديم\n", &schema);
    let old_snapshot = set(vec![old], &schema, 1).snapshot("ar");
    let new = pack(manifest("ar", "rtl"), "fallback = جديد\n", &schema);
    let new_snapshot = set(vec![new], &schema, 2).snapshot("ar");
    assert_eq!(old_snapshot.direction(), "rtl");
    assert_eq!(text(&old_snapshot, "fallback", &[]), "قديم");
    assert_eq!(text(&new_snapshot, "fallback", &[]), "جديد");
    assert_eq!(old_snapshot.revision(), 1);
    assert_eq!(new_snapshot.revision(), 2);
}

#[test]
fn fallback_is_exact_then_declared_parent_then_english() {
    let schema = schema();
    let parent = pack(manifest("fr", "ltr"), "hello = Parent { $name }\n", &schema);
    let mut child_manifest = manifest("fr-CA", "ltr");
    child_manifest.parent = Some("fr".into());
    let child = pack(child_manifest, "fallback = Enfant\n", &schema);
    let set = set(vec![parent, child], &schema, 1);
    let snapshot = set.snapshot("fr-CA");
    assert!(text(&snapshot, "hello", &[Argument { name: "name", value: Value::Text("A") }]).starts_with("Parent"));
    assert_eq!(text(&snapshot, "fallback", &[]), "Enfant");
    assert_eq!(set.snapshot("fr-FR").lang(), "en"); // No implicit catalog inheritance.
}

#[test]
fn conflicting_providers_and_missing_or_cyclic_parents_are_rejected() {
    let schema = schema();
    let first = pack(manifest("fr", "ltr"), "fallback = Un\n", &schema);
    let mut second_manifest = manifest("fr", "ltr");
    second_manifest.id = "other.pack".into();
    let second = pack(second_manifest, "fallback = Deux\n", &schema);
    assert!(PackSet::new(vec![first, second], Arc::clone(&schema), Arc::new(English::new(Arc::clone(&schema))), 1).is_err());
    let mut child = manifest("fr-CA", "ltr");
    child.parent = Some("fr".into());
    let child = pack(child, "fallback = Enfant\n", &schema);
    assert!(PackSet::new(vec![child], Arc::clone(&schema), Arc::new(English::new(Arc::clone(&schema))), 1).is_err());
    let mut first = manifest("fr", "ltr");
    first.parent = Some("fr-CA".into());
    let mut second = manifest("fr-CA", "ltr");
    second.parent = Some("fr".into());
    let packs = vec![pack(first, "fallback = A\n", &schema), pack(second, "fallback = B\n", &schema)];
    assert!(PackSet::new(packs, Arc::clone(&schema), Arc::new(English::new(schema)), 1).is_err());
}

#[test]
fn malformed_unsupported_or_unbounded_fluent_is_rejected() {
    let schema = schema();
    for catalog in [
        "fallback = {",
        "fallback = { hello }",
        "fallback = { -term }",
        "fallback = { NUMBER(2) }",
        "fallback = { DATETIME($date) }",
        "-term = hidden\nfallback = value\n",
        "fallback = Value\n .title = attr\n",
        "unknown = Value\n",
        "hello = { $unknown }\n",
        "fallback = A\nfallback = B\n",
    ] {
        assert!(LanguagePack::parse(&archive(manifest("fr", "ltr"), catalog), &schema).is_err(), "accepted {catalog}");
    }
    let nested = format!("fallback = {}\"x\"{}\n", "{ ".repeat(100), " }".repeat(100));
    assert!(matches!(LanguagePack::parse(&archive(manifest("fr", "ltr"), &nested), &schema), Err(PackError::Limit)));
    let over_output = format!("fallback = {}\n", "x".repeat(8193));
    assert!(LanguagePack::parse(&archive(manifest("fr", "ltr"), &over_output), &schema).is_err());
}

#[test]
fn manifests_reject_locale_direction_schema_and_cross_language_parent() {
    let schema = schema();
    for (locale, direction) in [
        ("not-a-real-tag", "ltr"),
        ("xx", "ltr"),
        ("ar", "ltr"),
        ("fr", "rtl"),
        ("en", "ltr"),
        ("fr-fr", "ltr"),
        ("fr-u-ca-gregory", "ltr"),
    ] {
        assert!(LanguagePack::parse(&archive(manifest(locale, direction), "fallback = X\n"), &schema).is_err(), "accepted {locale}");
    }
    let mut bad = manifest("fr", "ltr");
    bad.schema = 2;
    assert!(LanguagePack::parse(&archive(bad, "fallback = X\n"), &schema).is_err());
    let mut bad = manifest("fr", "ltr");
    bad.message_schema = "^9".parse().unwrap();
    assert!(LanguagePack::parse(&archive(bad, "fallback = X\n"), &schema).is_err());
    let mut bad = manifest("fr", "ltr");
    bad.parent = Some("ar".into());
    assert!(LanguagePack::parse(&archive(bad, "fallback = X\n"), &schema).is_err());
    for (locale, direction) in [("en-XA", "ltr"), ("ar-XB", "rtl")] {
        assert!(LanguagePack::parse(&archive(manifest(locale, direction), "fallback = Pseudo\n"), &schema).is_ok());
    }
}

#[test]
fn malformed_archives_and_compression_bombs_fail_before_publication() {
    let schema = schema();
    for bytes in [Vec::new(), b"not a zip".to_vec(), vec![0; MAX_ARCHIVE_BYTES + 1]] {
        assert!(LanguagePack::parse(&bytes, &schema).is_err());
    }
    let bomb = archive(manifest("fr", "ltr"), &format!("fallback = {}\n", "x".repeat(500_000)));
    assert!(matches!(LanguagePack::parse(&bomb, &schema), Err(PackError::Limit)));
    let mut bytes = archive(manifest("fr", "ltr"), "fallback = texte\n");
    bytes.truncate(bytes.len() - 3);
    assert!(LanguagePack::parse(&bytes, &schema).is_err());
}

#[test]
fn checksums_unlisted_entries_symlinks_executables_and_unsafe_paths_are_rejected() {
    let schema = schema();
    let valid = archive(manifest("fr", "ltr"), "fallback = texte\n");
    for (path, mode) in [("../evil", 0o644), ("/evil", 0o644), ("a\\b", 0o644), ("script.sh", 0o755), ("extra.txt", 0o644)] {
        let mut writer = ZipWriter::new_append(Cursor::new(valid.clone())).unwrap();
        writer.start_file(path, SimpleFileOptions::default().unix_permissions(mode)).unwrap();
        writer.write_all(b"extra").unwrap();
        assert!(LanguagePack::parse(&writer.finish().unwrap().into_inner(), &schema).is_err(), "accepted {path}");
    }
    let mut writer = ZipWriter::new_append(Cursor::new(valid.clone())).unwrap();
    writer.add_symlink("link", "LICENSE", SimpleFileOptions::default()).unwrap();
    assert!(LanguagePack::parse(&writer.finish().unwrap().into_inner(), &schema).is_err());
    // Corrupting the license is detected by CRC or SHA-256, never accepted.
    let mut bytes = valid;
    let position = bytes.windows(7).position(|window| window == b"LICENSE").unwrap();
    bytes[position] = b'X';
    assert!(LanguagePack::parse(&bytes, &schema).is_err());
}

#[test]
fn negotiation_is_bounded_and_preserves_preference_priority() {
    let schema = schema();
    let packs =
        vec![pack(manifest("fr", "ltr"), "fallback = Français\n", &schema), pack(manifest("ar", "rtl"), "fallback = عربي\n", &schema)];
    let set = set(packs, &schema, 1);
    assert_eq!(set.negotiate(Some("fr"), Some("ar"), Some("ar"), "en").lang(), "fr");
    assert_eq!(set.negotiate(Some("xx"), Some("ar"), Some("fr"), "en").lang(), "ar");
    assert_eq!(set.negotiate(None, None, Some("ar;q=0.2, fr;q=0.9"), "en").lang(), "fr");
    assert_eq!(set.negotiate(None, None, Some("FR-ca"), "en").lang(), "fr");
    assert_eq!(set.negotiate(None, None, Some("fr;q=0, ar;q=NaN"), "en").lang(), "en");
    assert_eq!(set.negotiate(None, None, Some(&"fr,".repeat(1000)), "en").lang(), "en");
}

#[test]
fn concurrent_readers_share_immutable_catalogs_without_disk_io() {
    let schema = schema();
    let set = Arc::new(set(vec![pack(manifest("fr", "ltr"), "fallback = Bonjour\n", &schema)], &schema, 1));
    let readers: Vec<_> = (0..8)
        .map(|_| {
            let set = Arc::clone(&set);
            std::thread::spawn(move || {
                for _ in 0..100 {
                    assert_eq!(text(&set.snapshot("fr"), "fallback", &[]), "Bonjour");
                }
            })
        })
        .collect();
    for reader in readers {
        reader.join().unwrap();
    }
}

#[test]
fn duplicate_and_encrypted_entries_are_rejected_even_with_consistent_headers() {
    let schema = schema();
    let valid = archive(manifest("fr", "ltr"), "fallback = texte\n");
    let mut writer = ZipWriter::new_append(Cursor::new(valid.clone())).unwrap();
    writer.start_file("LICENSX", SimpleFileOptions::default().unix_permissions(0o644)).unwrap();
    writer.write_all(b"duplicate").unwrap();
    let mut duplicate = writer.finish().unwrap().into_inner();
    for position in 0..duplicate.len() - 7 {
        if &duplicate[position..position + 7] == b"LICENSX" {
            duplicate[position..position + 7].copy_from_slice(b"LICENSE");
        }
    }
    assert!(LanguagePack::parse(&duplicate, &schema).is_err());
    let mut encrypted = valid;
    for position in 0..encrypted.len() - 10 {
        let offset = if &encrypted[position..position + 4] == b"PK\x03\x04" {
            Some(6)
        } else if &encrypted[position..position + 4] == b"PK\x01\x02" {
            Some(8)
        } else {
            None
        };
        if let Some(offset) = offset {
            encrypted[position + offset] |= 1;
        }
    }
    assert!(LanguagePack::parse(&encrypted, &schema).is_err());
}

#[test]
fn checksum_is_checked_independently_of_zip_crc() {
    let schema = schema();
    let mut manifest = manifest("fr", "ltr");
    manifest.checksums = BTreeMap::from([("LICENSE".into(), "0".repeat(64)), ("catalogs/core.ftl".into(), "0".repeat(64))]);
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (path, content) in [
        ("pack.toml", toml::to_string(&manifest).unwrap()),
        ("LICENSE", "License".into()),
        ("catalogs/core.ftl", "fallback = texte\n".into()),
    ] {
        writer.start_file(path, SimpleFileOptions::default().unix_permissions(0o644)).unwrap();
        writer.write_all(content.as_bytes()).unwrap();
    }
    assert!(matches!(LanguagePack::parse(&writer.finish().unwrap().into_inner(), &schema), Err(PackError::Checksum)));
}

#[test]
fn installation_order_cannot_change_resolution() {
    let schema = schema();
    let french = pack(manifest("fr", "ltr"), "fallback = Français\n", &schema);
    let arabic = pack(manifest("ar", "rtl"), "fallback = عربي\n", &schema);
    let forward = set(vec![Arc::clone(&french), Arc::clone(&arabic)], &schema, 1);
    let reverse = set(vec![arabic, french], &schema, 1);
    for locale in ["fr", "ar", "en"] {
        assert_eq!(text(&forward.snapshot(locale), "fallback", &[]), text(&reverse.snapshot(locale), "fallback", &[]));
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| [char::from(HEX[usize::from(byte >> 4)]), char::from(HEX[usize::from(byte & 15)])])
        .collect()
}

#[test]
fn invalid_or_large_runtime_arguments_never_expand_without_a_bound() {
    let schema = schema();
    let set = set(vec![pack(manifest("fr", "ltr"), "hello = Bonjour { $name }\ncount = { $count }\n", &schema)], &schema, 1);
    let snapshot = set.snapshot("fr");
    for value in [f64::MAX, f64::MIN, f64::NAN, f64::INFINITY] {
        assert!(text(&snapshot, "count", &[Argument { name: "count", value: Value::Number(value) }]).len() <= 8192);
    }
    assert!(text(&snapshot, "hello", &[Argument { name: "name", value: Value::Text(&"x".repeat(100_000)) }]).len() <= 8192);
    assert!(text(&snapshot, "hello", &[Argument { name: "name", value: Value::Number(1.0) }]).len() <= 8192);
}

#[test]
fn parser_nesting_is_bounded_before_rejected_function_ast_exists() {
    let schema = schema();
    let catalog = format!("fallback = {{ {}\"x\"{} }}\n", "FUNC(".repeat(2000), ")".repeat(2000));
    assert!(LanguagePack::parse(&archive(manifest("fr", "ltr"), &catalog), &schema).is_err());
    // Comment text is inert, and ordinary parentheses remain available in text.
    let valid = "# A translator comment\nfallback = Texte (parenthèses)\n";
    assert!(LanguagePack::parse(&archive(manifest("fr", "ltr"), valid), &schema).is_ok());
}

#[test]
fn browser_zero_quality_exclusions_survive_wildcard_and_default_lookup() {
    let schema = schema();
    let set = set(vec![pack(manifest("fr", "ltr"), "fallback = Français\n", &schema)], &schema, 1);
    assert_eq!(set.negotiate(None, None, Some("fr;q=0, *;q=1"), "fr").lang(), "en");
    assert_eq!(set.negotiate(None, None, Some("fr;q=0"), "fr").lang(), "en");
    assert_eq!(set.negotiate(None, None, Some("fr;q=1, *;q=0"), "en").lang(), "fr");
    for invalid in ["fr;q=1e0", "fr;q=0.1234", "fr;q=+1", "fr;q=1.1", "fr;q=0.5;x=2"] {
        assert_eq!(set.negotiate(None, None, Some(invalid), "en").lang(), "en");
    }
}

#[test]
fn inconsistent_headers_overlaps_and_unsupported_compression_are_rejected() {
    let schema = schema();
    let valid = archive(manifest("fr", "ltr"), "fallback = Texte\n");
    let central = valid.windows(4).position(|bytes| bytes == b"PK\x01\x02").unwrap();
    let mut inconsistent = valid.clone();
    inconsistent[30] ^= 1;
    assert!(LanguagePack::parse(&inconsistent, &schema).is_err());
    let mut unsupported = valid.clone();
    unsupported[8..10].copy_from_slice(&99u16.to_le_bytes());
    unsupported[central + 10..central + 12].copy_from_slice(&99u16.to_le_bytes());
    assert!(LanguagePack::parse(&unsupported, &schema).is_err());
    let mut overlap = valid.clone();
    let original = u32::from_le_bytes(overlap[18..22].try_into().unwrap());
    overlap[18..22].copy_from_slice(&(original + 64).to_le_bytes());
    overlap[central + 20..central + 24].copy_from_slice(&(original + 64).to_le_bytes());
    assert!(LanguagePack::parse(&overlap, &schema).is_err());
    let mut too_many = valid;
    let end = too_many.len() - 22;
    too_many[end + 8..end + 10].copy_from_slice(&u16::MAX.to_le_bytes());
    too_many[end + 10..end + 12].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(LanguagePack::parse(&too_many, &schema).is_err());
}

/// Run without a wall-clock assertion: `cargo test --release -p rbit-i18n
/// benchmark_warm_100_message_page -- --ignored --nocapture`.
#[test]
#[ignore = "manual warm formatting benchmark; no timing assertion"]
fn benchmark_warm_100_message_page() {
    use std::fmt::Write as _;
    let schema = Arc::new(
        MessageSchema::new(
            (0..100).map(|index| spec(&format!("label-{index}"), "Hello {name}", &[("name", ArgumentKind::Text)])).collect(),
        )
        .unwrap(),
    );
    let mut catalog = String::new();
    for index in 0..100 {
        writeln!(catalog, "label-{index} = Bonjour {{ $name }}").unwrap();
    }
    let set = set(vec![pack(manifest("fr", "ltr"), &catalog, &schema)], &schema, 1);
    let snapshot = set.snapshot("fr");
    let keys: Vec<_> = (0..100).map(|index| format!("label-{index}")).collect();
    let args = [Argument { name: "name", value: Value::Text("a <reviewed> contributor") }];
    for key in &keys {
        std::hint::black_box(text(&snapshot, key, &args));
    }
    let started = std::time::Instant::now();
    for _ in 0..1000 {
        for key in &keys {
            std::hint::black_box(text(&snapshot, key, &args));
        }
    }
    let elapsed = started.elapsed();
    println!("1000 warm pages × 100 messages: {elapsed:?}; {} µs/page", elapsed.as_micros() / 1000);
}

#[test]
fn quoted_delimiters_cannot_hide_parser_recursion() {
    let schema = schema();
    for literal in ["\")\", ", "\"}\", "] {
        let catalog = format!("fallback = {{ {}\"x\"{} }}\n", format!("FUNC({literal}").repeat(2000), ")".repeat(2000));
        assert!(LanguagePack::parse(&archive(manifest("fr", "ltr"), &catalog), &schema).is_err());
    }
    let valid = "# A comment with { FUNC( }\nfallback = Text (optional)\nhello = { $name ->\n [formal] Hello (formal)\n *[other] Hello (ordinary)\n}\n";
    assert!(LanguagePack::parse(&archive(manifest("fr", "ltr"), valid), &schema).is_ok());
}

#[test]
fn orphan_local_entries_are_rejected_even_when_not_in_the_directory() {
    let schema = schema();
    let valid = archive(manifest("fr", "ltr"), "fallback = Texte\n");
    let mut writer = ZipWriter::new_append(Cursor::new(valid)).unwrap();
    writer.start_file("orphan.bin", SimpleFileOptions::default().unix_permissions(0o644)).unwrap();
    writer.write_all(b"unlisted data").unwrap();
    let mut orphan = writer.finish().unwrap().into_inner();
    let end = orphan.len() - 22;
    let last_header = orphan.windows(4).rposition(|window| window == b"PK\x01\x02").unwrap();
    let removed = u32::try_from(end - last_header).unwrap();
    let directory_size = u32::from_le_bytes(orphan[end + 12..end + 16].try_into().unwrap());
    orphan.drain(last_header..end);
    let end = orphan.len() - 22;
    orphan[end + 8..end + 10].copy_from_slice(&3u16.to_le_bytes());
    orphan[end + 10..end + 12].copy_from_slice(&3u16.to_le_bytes());
    orphan[end + 12..end + 16].copy_from_slice(&(directory_size - removed).to_le_bytes());
    assert!(LanguagePack::parse(&orphan, &schema).is_err());
}
