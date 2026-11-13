use super::*;

fn set(lines: &[&str]) -> GlobSet {
    GlobSet::compile(lines.iter().copied(), false).unwrap()
}

#[test]
fn wildcards_match_within_a_segment_and_globstars_across() {
    let star = set(&["dist/*.zip*"]);
    assert!(star.matches("dist/a.zip") && star.matches("dist/a.zip.sha256"));
    assert!(!star.matches("dist/b.txt") && !star.matches("dist/sub/c.zip"), "`*` stays within its segment");
    let deep = set(&["dist/**/*.zip"]);
    assert!(deep.matches("dist/a.zip") && deep.matches("dist/sub/c.zip") && deep.matches("dist/x/y/z.zip"));
    assert!(!deep.matches("other/a.zip"));
    let one = set(&["out/a?.log", "src/[a-c]*.rs", "src/[!x]y.rs"]);
    assert!(one.matches("out/a1.log") && !one.matches("out/b1.log") && !one.matches("out/a12.log"));
    assert!(one.matches("src/b.rs") && !one.matches("src/d.rs"));
    assert!(one.matches("src/ay.rs") && !one.matches("src/xy.rs"));
    assert!(set(&["*"]).matches(".hidden"), "dotfiles match, as with minimatch's dot option");
}

#[test]
fn the_last_matching_pattern_decides_and_directories_bring_their_contents() {
    let globs = set(&["dist/**/*.zip", "!dist/sub/skip.zip", "docs/readme.md", "target", "!target/tmp"]);
    assert!(globs.matches("dist/sub/c.zip") && !globs.matches("dist/sub/skip.zip"));
    assert!(globs.matches("docs/readme.md"));
    assert!(globs.matches("target/x/y.rlib"), "implicit descendants");
    assert!(!globs.matches("target/tmp/a") && !globs.matches("target/tmp"), "an excluded directory takes its contents");
    let again = set(&["a/**", "!a/b/**", "a/b/keep"]);
    assert!(again.matches("a/x") && !again.matches("a/b/y") && again.matches("a/b/keep"));
}

#[test]
fn search_paths_are_the_literal_starts_without_nested_ones() {
    assert_eq!(set(&["dist/*.zip*"]).search_paths(), ["dist"]);
    assert_eq!(set(&["dist/**/*.zip", "!dist/sub/skip.zip", "docs/readme.md"]).search_paths(), ["dist", "docs/readme.md"]);
    assert_eq!(set(&["a/b", "a/b/c/*.txt", "a"]).search_paths(), ["a"]);
    assert_eq!(set(&["**/*.rlib"]).search_paths(), [""]);
    assert_eq!(common_ancestor(["dist", "docs/readme.md"]), "");
    assert_eq!(common_ancestor(["dist/a.txt", "dist/b.txt"]), "dist");
    assert_eq!(common_ancestor(["a/b/c", "a/b"]), "a/b");
}

#[test]
fn walking_prunes_directories_that_cannot_match() {
    let globs = set(&["src/**/*.rs", "docs/*.md"]);
    assert!(globs.may_contain("src") && globs.may_contain("src/deep/er") && globs.may_contain("docs"));
    assert!(!globs.may_contain("target") && !globs.may_contain("docs/sub"));
    assert!(set(&["dist"]).may_contain("dist/any/depth"), "inside a matched directory");
}

#[test]
fn bad_patterns_are_refused() {
    for bad in ["../x", "a/../b", "[abc", "!"] {
        assert!(GlobSet::compile([bad], false).is_err(), "{bad}");
    }
    assert!(GlobSet::compile(["  ", "", "x"], false).is_ok(), "blank lines are skipped");
    let many: Vec<String> = (0..=MAX_PATTERNS).map(|n| format!("f{n}")).collect();
    assert!(GlobSet::compile(many.iter().map(String::as_str), false).is_err());
}

#[test]
fn hostile_patterns_match_in_polynomial_time() {
    let started = std::time::Instant::now();
    let stars = set(&[&format!("{}b", "*a".repeat(40))]);
    assert!(!stars.matches(&"a".repeat(200)));
    let globstars = set(&[&format!("{}b", "**/a/".repeat(30))]);
    assert!(!globstars.matches(&"a/".repeat(200)));
    assert!(set(&["**/a/**/b"]).matches("x/a/y/z/b") && set(&["a*b*c"]).matches("aXXbYc") && !set(&["a*b*c"]).matches("aXXbYcZ"));
    assert!(set(&["*ab*c"]).matches("xabyabc"), "backtracking to the last `*` keeps later ones");
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "{:?}", started.elapsed());
}

#[test]
fn windows_paths_and_case() {
    let globs = GlobSet::compile(["Dist\\*.ZIP"], true).unwrap();
    assert!(globs.matches("dist/a.zip") && globs.matches("dist\\b.Zip"));
    assert!(!set(&["Dist/*.ZIP"]).matches("dist/a.zip"), "case counts elsewhere");
}
