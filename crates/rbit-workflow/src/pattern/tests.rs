//! Existing pattern acceptance fixtures, separated from production logic.
use super::{Pattern, Patterns, to_regex};

/// act's `TestMatchPattern` table, verbatim: patterns, inputs, `Skip`, `Filter`.
    #[rustfmt::skip]
    const ACT_CASES: &[(&[&str], &[&str], bool, bool)] = &[
        (&["*"], &["path/with/slash"], true, false),
        (&["path/a", "path/b", "path/c"], &["meta", "path/b", "otherfile"], false, false),
        (&["path/a", "path/b", "path/c"], &["path/b"], false, true),
        (&["path/a", "path/b", "path/c"], &["path/c", "path/b"], false, true),
        (&["path/a", "path/b", "path/c"], &["path/c", "path/b", "path/a"], false, true),
        (&["path/a", "path/b", "path/c"], &["path/c", "path/b", "path/d", "path/a"], false, false),
        (&[], &[], false, false),
        (&["\\!file"], &["!file"], false, true),
        (&["escape\\\\backslash"], &["escape\\backslash"], false, true),
        (&[".yml"], &["fyml"], true, false),
        // https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#patterns-to-match-branches-and-tags
        (&["feature/*"], &["feature/my-branch"], false, true),
        (&["feature/*"], &["feature/your-branch"], false, true),
        (&["feature/**"], &["feature/beta-a/my-branch"], false, true),
        (&["feature/**"], &["feature/beta-a/my-branch"], false, true),
        (&["feature/**"], &["feature/mona/the/octocat"], false, true),
        (&["main", "releases/mona-the-octocat"], &["main"], false, true),
        (&["main", "releases/mona-the-octocat"], &["releases/mona-the-octocat"], false, true),
        (&["*"], &["main"], false, true),
        (&["*"], &["releases"], false, true),
        (&["**"], &["all/the/branches"], false, true),
        (&["**"], &["every/tag"], false, true),
        (&["*feature"], &["mona-feature"], false, true),
        (&["*feature"], &["feature"], false, true),
        (&["*feature"], &["ver-10-feature"], false, true),
        (&["v2*"], &["v2"], false, true),
        (&["v2*"], &["v2.0"], false, true),
        (&["v2*"], &["v2.9"], false, true),
        (&["v[12].[0-9]+.[0-9]+"], &["v1.10.1"], false, true),
        (&["v[12].[0-9]+.[0-9]+"], &["v2.0.0"], false, true),
        // https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#patterns-to-match-file-paths
        (&["*"], &["README.md"], false, true),
        (&["*"], &["server.rb"], false, true),
        (&["*.jsx?"], &["page.js"], false, true),
        (&["*.jsx?"], &["page.jsx"], false, true),
        (&["**"], &["all/the/files.md"], false, true),
        (&["*.js"], &["app.js"], false, true),
        (&["*.js"], &["index.js"], false, true),
        (&["**.js"], &["index.js"], false, true),
        (&["**.js"], &["js/index.js"], false, true),
        (&["**.js"], &["src/js/app.js"], false, true),
        (&["docs/*"], &["docs/README.md"], false, true),
        (&["docs/*"], &["docs/file.txt"], false, true),
        (&["docs/**"], &["docs/README.md"], false, true),
        (&["docs/**"], &["docs/mona/octocat.txt"], false, true),
        (&["docs/**/*.md"], &["docs/README.md"], false, true),
        (&["docs/**/*.md"], &["docs/mona/hello-world.md"], false, true),
        (&["docs/**/*.md"], &["docs/a/markdown/file.md"], false, true),
        (&["**/docs/**"], &["docs/hello.md"], false, true),
        (&["**/docs/**"], &["dir/docs/my-file.txt"], false, true),
        (&["**/docs/**"], &["space/docs/plan/space.doc"], false, true),
        (&["**/README.md"], &["README.md"], false, true),
        (&["**/README.md"], &["js/README.md"], false, true),
        (&["**/*src/**"], &["a/src/app.js"], false, true),
        (&["**/*src/**"], &["my-src/code/js/app.js"], false, true),
        (&["**/*-post.md"], &["my-post.md"], false, true),
        (&["**/*-post.md"], &["path/their-post.md"], false, true),
        (&["**/migrate-*.sql"], &["migrate-10909.sql"], false, true),
        (&["**/migrate-*.sql"], &["db/migrate-v1.0.sql"], false, true),
        (&["**/migrate-*.sql"], &["db/sept/migrate-v1.sql"], false, true),
        (&["*.md", "!README.md"], &["hello.md"], false, true),
        (&["*.md", "!README.md"], &["README.md"], true, true),
        (&["*.md", "!README.md"], &["docs/hello.md"], true, true),
        (&["*.md", "!README.md", "README*"], &["hello.md"], false, true),
        (&["*.md", "!README.md", "README*"], &["README.md"], false, true),
        (&["*.md", "!README.md", "README*"], &["README.doc"], false, true),
    ];

#[test]
fn act_match_pattern_table() {
    for (patterns, inputs, skip, filter) in ACT_CASES {
        let compiled = Patterns::compile(*patterns).unwrap();
        assert_eq!(compiled.skip(*inputs), *skip, "skip {patterns:?} {inputs:?}");
        assert_eq!(compiled.filter(*inputs), *filter, "filter {patterns:?} {inputs:?}");
    }
}

#[test]
fn translation_matches_act() {
    let cases = [
        ("*", "^[^/]*$"),
        ("**", "^.*$"),
        ("**/a", "^(.+/)?a$"),
        ("a.b", "^a\\.b$"),
        ("+a?", "^\\+a?$"),
        ("v[12].[0-9]+", "^v[12]\\.[0-9]+$"),
        ("[A-z]", "^[A-z]$"),
        ("\\*", "^\\*$"),
        ("(x|y){2}", "^\\(x\\|y\\)\\{2\\}$"),
        ("é*", "^é[^/]*$"),
    ];
    for (pattern, expected) in cases {
        assert_eq!(to_regex(pattern).unwrap(), expected, "{pattern}");
    }
}

#[test]
fn invalid_patterns_name_every_problem() {
    let cases: &[(&str, &[(usize, &str)])] = &[
        ("[]", &[(1, "unexpected empty brackets '[]'")]),
        ("[a", &[(2, "missing closing bracket ']' after '['")]),
        ("[-a]", &[(1, "invalid range")]),
        ("[a-]", &[(2, "ranges can only include a-z, A-Z, A-z, and 0-9")]),
        ("[a-", &[(2, "invalid range"), (3, "missing closing bracket ']' after '['")]),
        ("[z-a]", &[(2, "ranges can only include a-z, A-Z, A-z, and 0-9")]),
        ("[a-9]", &[(2, "ranges can only include a-z, A-Z, A-z, and 0-9")]),
        ("[.]", &[(1, "ranges can only include a-z, A-Z and 0-9")]),
        ("[é]", &[(1, "ranges can only include a-z, A-Z and 0-9"), (2, "ranges can only include a-z, A-Z and 0-9")]),
        ("a\\", &[(1, "missing symbol after \\")]),
    ];
    for (pattern, problems) in cases {
        let error = Pattern::compile(pattern).unwrap_err();
        assert_eq!(error.problems, *problems, "{pattern}");
        assert!(error.to_string().starts_with(&format!("invalid pattern '{pattern}': position ")));
    }
    // Go's RE2 refuses a repetition of a repetition; the translation does too.
    for pattern in ["a*+", "a++", "a?+", "**/+", "a*??", "a+??"] {
        assert_eq!(Pattern::compile(pattern).unwrap_err().problems.len(), 1, "{pattern}");
    }
    for pattern in ["a*?", "a+?", "a??", "**/?", "+a", "?a", "[ab]+", "a+b+"] {
        assert!(Pattern::compile(pattern).is_ok(), "{pattern}");
    }
    assert!(Pattern::compile("!").is_ok_and(|pattern| pattern.is_negative() && pattern.is_match("")));
}

#[test]
fn negation_order_is_github_not_glob() {
    let list = Patterns::compile(["**", "!docs/**", "docs/keep.md"]).unwrap();
    assert!(!list.skip(["src/main.rs"]));
    assert!(list.skip(["docs/a.md"]), "excluded by the later negation");
    assert!(!list.skip(["docs/keep.md"]), "re-included by the last pattern");
    assert!(!list.skip(["docs/a.md", "src/lib.rs"]), "one included input is enough");
    let ignore = Patterns::compile(["docs/**", "!docs/keep.md"]).unwrap();
    assert!(ignore.filter(["docs/a.md"]));
    assert!(ignore.filter(["docs/keep.md"]), "the first deciding pattern wins");
    // act's (and Forgejo's) quirk, kept: a negative pattern that does not match an input
    // ignores it, so in this list every path outside `docs/keep.md` is ignored.
    assert!(ignore.filter(["docs/a.md", "src/lib.rs"]));
    let ignore = Patterns::compile(["docs/**"]).unwrap();
    assert!(!ignore.filter(["docs/a.md", "src/lib.rs"]), "one input not ignored is enough");
    assert!(!Patterns::default().skip(["x"]) && !Patterns::default().filter(["x"]));
}
