//! Logical Unicode names, not a substitute for URL decoding or filesystem ownership.
use rbit_wikis::{WikiNameError, WikiPath, WikiTitle};

#[test]
fn paths_normalize_canonical_unicode_without_case_folding_or_transliteration() {
    let decomposed: WikiPath = "Cafe\u{301}/Guide".parse().unwrap();
    let composed: WikiPath = "Caf\u{e9}/Guide".parse().unwrap();
    assert_eq!(decomposed, composed);
    assert_ne!(composed, "Caf\u{e9}/guide".parse().unwrap());
    for path in
        ["\u{6307}\u{5357}/Install", "\u{0915}\u{093f}\u{0924}\u{093e}\u{092c}", "\u{062f}\u{0644}\u{064a}\u{0644}", "Guide.v2/Step_1"]
    {
        assert_eq!(path.parse::<WikiPath>().unwrap().as_str(), path);
    }
}

#[test]
fn paths_reject_url_reinterpretation_traversal_and_platform_separators() {
    for path in [
        "",
        "/Guide",
        "Guide/",
        "Guide//Page",
        ".",
        "..",
        "a/../b",
        "a/./b",
        "a..b",
        "C:/Guide",
        "a\\b",
        "\\\\host\\page",
        "a%2fb",
        "%2e%2e",
        "a/%252e%252e",
        "a%00b",
        "a?b",
        "a#b",
        "a:b",
        "a b",
        "a\0b",
        "a\nb",
        "a\tb",
        "a\u{2215}b",
        "a\u{ff0f}b",
        "a\u{ff3c}b",
        "a\u{ff0e}b",
    ] {
        assert!(path.parse::<WikiPath>().is_err(), "accepted {path:?}");
    }
    // Raw encoded text is never automatically decoded, including otherwise benign
    // UTF-8. The eventual HTTP adapter owns exactly one guarded decoding step.
    assert!("Caf%C3%A9".parse::<WikiPath>().is_err());
}

#[test]
fn reserved_windows_git_and_invisible_components_never_become_logical_paths() {
    for path in [
        "CON",
        "con.txt",
        "Dir/NuL.md",
        "PRN",
        "aux",
        "COM0",
        "com9.md",
        "LPT1",
        "lpt9.txt",
        "com\u{00b9}.txt",
        "LPT\u{00b2}",
        "COM\u{00b3}",
        "name.",
        "name ",
        "a/.git",
        "a/.GITMODULES",
        "a/foo.GIT",
        "a/foo.LOCK",
        "a\u{200b}b",
        "a\u{202e}b",
        "a\u{2066}b",
        "a\u{feff}b",
        "a\u{034f}",
        "a\u{180b}",
        "a\u{180f}",
        "a\u{fe0f}",
        "a\u{e0100}",
    ] {
        assert!(path.parse::<WikiPath>().is_err(), "accepted {path:?}");
    }
    for path in ["console", "auxiliary", "COM10", "lpt10", "nulled", "a.b"] {
        assert!(path.parse::<WikiPath>().is_ok(), "refused {path:?}");
    }
}

#[test]
fn path_bounds_apply_before_normalization_and_to_each_component() {
    assert!("a".repeat(WikiPath::MAX_COMPONENT_BYTES).parse::<WikiPath>().is_ok());
    assert_eq!("a".repeat(WikiPath::MAX_COMPONENT_BYTES + 1).parse::<WikiPath>(), Err(WikiNameError::TooLong));
    assert_eq!("a".repeat(WikiPath::MAX_RAW_BYTES + 1).parse::<WikiPath>(), Err(WikiNameError::RawLimit));
    assert!(["a"; WikiPath::MAX_COMPONENTS].join("/").parse::<WikiPath>().is_ok());
    assert_eq!(["a"; WikiPath::MAX_COMPONENTS + 1].join("/").parse::<WikiPath>(), Err(WikiNameError::TooDeep));
    let segment = "a".repeat(WikiPath::MAX_COMPONENT_BYTES);
    assert_eq!([segment.as_str(); WikiPath::MAX_COMPONENTS].join("/").parse::<WikiPath>(), Err(WikiNameError::TooLong));
    // More than 64 UTF-8 bytes is rejected even below 64 scalar values.
    assert_eq!("\u{754c}".repeat(22).parse::<WikiPath>(), Err(WikiNameError::TooLong));
}

#[test]
fn titles_preserve_localized_text_and_markup_as_text_with_explicit_trim_and_nfc() {
    let title: WikiTitle = "  Cafe\u{301} <script> & \u{1f680}  ".parse().unwrap();
    assert_eq!(title.as_str(), "Caf\u{e9} <script> & \u{1f680}");
    assert_eq!(title.to_string(), title.as_str());
    assert!("\u{754c}".repeat(WikiTitle::MAX_CHARACTERS).parse::<WikiTitle>().is_ok());
    assert_eq!("a".repeat(WikiTitle::MAX_CHARACTERS + 1).parse::<WikiTitle>(), Err(WikiNameError::TooLong));
    assert_eq!(" ".repeat(WikiTitle::MAX_RAW_BYTES + 1).parse::<WikiTitle>(), Err(WikiNameError::RawLimit));
    for title in ["", "   ", "\nTitle", "Title\t", "a\0b", "a\u{2028}b", "a\u{202e}b", "a\u{2069}b"] {
        assert!(title.parse::<WikiTitle>().is_err(), "accepted {title:?}");
    }
    // Joiners used in scripts/emoji remain valid title text, never path characters.
    assert!("\u{1f469}\u{200d}\u{1f4bb}".parse::<WikiTitle>().is_ok());
}
