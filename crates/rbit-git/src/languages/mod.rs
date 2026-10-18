//! Language breakdown of a revision, in the style of GitHub Linguist.
//!
//! Every blob in the tree is classified by path alone — exact file name, then extension —
//! and its size is read from the object header, so blob contents are never inflated.
//! Vendored, generated and documentation files are skipped, as are languages of kind
//! Data and Prose; root `.gitattributes` `linguist-*` attributes override all of these.

mod attributes;
mod catalog;
mod exclusions;

use std::{collections::HashMap, sync::LazyLock};

use gix::{
    ObjectId,
    bstr::{BStr, ByteSlice},
    objs::tree::{EntryKind as GixEntryKind, EntryRef},
    traverse::tree::{Recorder, Visit, visit::Action},
};

pub use attributes::{LinguistAttributes, LinguistOverrides};
pub use catalog::{FALLBACK_COLOR, LANGUAGES, Language, LanguageKind};

use crate::{GitError, RepositoryReader, reader::resolve_commit};

/// At most this many languages are listed individually; the rest become "Other".
pub const MAX_LISTED_LANGUAGES: usize = 8;
/// Languages below this share of the total are folded into "Other".
pub const MIN_LISTED_PERCENT: f64 = 0.5;
pub const OTHER_LANGUAGE_NAME: &str = "Other";
/// Neutral grey that reads on both light and dark backgrounds.
pub const OTHER_LANGUAGE_COLOR: &str = "#9e9e9e";

/// A root `.gitattributes` larger than this is ignored.
const MAX_ATTRIBUTES_BYTES: u64 = 64 * 1024;

/// One slice of the language bar.
#[derive(Debug, Clone, PartialEq)]
pub struct LanguageShare {
    pub name: String,
    /// `#rrggbb`.
    pub color: String,
    pub bytes: u64,
    /// 0–100.
    pub percent: f64,
}

/// The language breakdown of one revision, largest first, with "Other" last.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LanguageStats {
    /// Bytes of every counted file, including those folded into "Other".
    pub total_bytes: u64,
    pub languages: Vec<LanguageShare>,
}

impl LanguageStats {
    /// Sorts per-language byte counts and folds the long tail into "Other".
    pub fn from_byte_counts(counts: impl IntoIterator<Item = (&'static Language, u64)>) -> Self {
        let mut counts: Vec<(&Language, u64)> = counts.into_iter().filter(|(_, bytes)| *bytes > 0).collect();
        counts.sort_by(|(a, a_bytes), (b, b_bytes)| b_bytes.cmp(a_bytes).then_with(|| a.name.cmp(b.name)));
        let total_bytes: u64 = counts.iter().map(|(_, bytes)| bytes).sum();

        let mut languages = Vec::new();
        let mut other_bytes = 0;
        for (index, (language, bytes)) in counts.into_iter().enumerate() {
            let percent = percent_of(bytes, total_bytes);
            if index < MAX_LISTED_LANGUAGES && percent >= MIN_LISTED_PERCENT {
                languages.push(LanguageShare { name: language.name.to_owned(), color: language.color.to_owned(), bytes, percent });
            } else {
                other_bytes += bytes;
            }
        }
        if other_bytes > 0 {
            languages.push(LanguageShare {
                name: OTHER_LANGUAGE_NAME.to_owned(),
                color: OTHER_LANGUAGE_COLOR.to_owned(),
                bytes: other_bytes,
                percent: percent_of(other_bytes, total_bytes),
            });
        }
        Self { total_bytes, languages }
    }

    pub fn is_empty(&self) -> bool {
        self.languages.is_empty()
    }
}

#[allow(clippy::cast_precision_loss, reason = "a percentage needs no more than f64's 52 bits of precision")]
fn percent_of(part: u64, total: u64) -> f64 {
    if total == 0 { 0.0 } else { part as f64 * 100.0 / total as f64 }
}

static BY_FILENAME: LazyLock<HashMap<&'static str, &'static Language>> =
    LazyLock::new(|| LANGUAGES.iter().flat_map(|language| language.filenames.iter().map(move |name| (*name, language))).collect());

static BY_EXTENSION: LazyLock<HashMap<&'static str, &'static Language>> = LazyLock::new(|| {
    LANGUAGES.iter().flat_map(|language| language.extensions.iter().map(move |extension| (*extension, language))).collect()
});

/// The language of `path` judged by its name alone.
pub fn detect(path: &str) -> Option<&'static Language> {
    let name = path.rsplit_once('/').map_or(path, |(_, name)| name);
    if let Some(language) = BY_FILENAME.get(name) {
        return Some(language);
    }
    let (stem, extension) = name.rsplit_once('.')?;
    if stem.is_empty() && !BY_EXTENSION.contains_key(extension) {
        // A dotfile such as `.env` has no extension.
        return None;
    }
    BY_EXTENSION.get(extension.to_ascii_lowercase().as_str()).copied()
}

/// Looks a language up by its Linguist name, ignoring case.
pub fn find_by_name(name: &str) -> Option<&'static Language> {
    LANGUAGES.iter().find(|language| language.name.eq_ignore_ascii_case(name))
}

/// The language `path` counts toward, or `None` when it is excluded from the breakdown.
pub fn classify(path: &str, attributes: &LinguistAttributes) -> Option<&'static Language> {
    let overrides = attributes.for_path(path);
    let excluded = overrides.vendored.unwrap_or_else(|| exclusions::is_vendored(path))
        || overrides.generated.unwrap_or_else(|| exclusions::is_generated(path))
        || overrides.documentation.unwrap_or_else(|| exclusions::is_documentation(path));
    if excluded {
        return None;
    }
    let language = match overrides.language.as_deref() {
        Some(name) => find_by_name(name)?,
        None => detect(path)?,
    };
    overrides.detectable.unwrap_or_else(|| language.kind.is_detectable_by_default()).then_some(language)
}

impl RepositoryReader {
    /// Counted files for a language at this revision. `Other` uses the same grouping
    /// as the language bar, including languages below the display threshold.
    pub async fn language_files(&self, revision: String, name: String) -> Result<Vec<String>, GitError> {
        self.with_repo(move |repo| {
            let commit = repo.find_commit(resolve_commit(repo, &revision)?).map_err(GitError::backend)?;
            let tree = commit.tree().map_err(GitError::backend)?;
            let attributes = root_attributes(repo, &tree)?;
            let mut collector = FileCollector::new(&attributes, true);
            tree.traverse().breadthfirst(&mut collector).map_err(GitError::backend)?;
            let mut counts: HashMap<&'static str, (&'static Language, u64)> = HashMap::new();
            if name == "Other" {
                for (language, id, _) in &collector.files {
                    counts.entry(language.name).or_insert((language, 0)).1 += repo.find_header(*id).map_err(GitError::backend)?.size();
                }
            }
            let stats = LanguageStats::from_byte_counts(counts.into_values());
            let mut paths: Vec<_> = collector
                .files
                .into_iter()
                .filter_map(|(language, _, path)| {
                    let matches = if name == "Other" {
                        !stats.languages.iter().any(|share| share.name == language.name)
                    } else {
                        language.name == name
                    };
                    matches.then_some(path)
                })
                .collect();
            paths.sort();
            Ok(paths)
        })
        .await
    }

    /// The language breakdown of the tree at `revision`.
    pub async fn language_stats(&self, revision: String) -> Result<LanguageStats, GitError> {
        self.with_repo(move |repo| {
            let commit = repo.find_commit(resolve_commit(repo, &revision)?).map_err(GitError::backend)?;
            let tree = commit.tree().map_err(GitError::backend)?;
            let attributes = root_attributes(repo, &tree)?;

            let mut collector = FileCollector::new(&attributes, false);
            tree.traverse().breadthfirst(&mut collector).map_err(GitError::backend)?;

            let mut counts: HashMap<&'static str, (&'static Language, u64)> = HashMap::new();
            for (language, id, _) in collector.files {
                let size = repo.find_header(id).map_err(GitError::backend)?.size();
                counts.entry(language.name).or_insert((language, 0)).1 += size;
            }
            Ok(LanguageStats::from_byte_counts(counts.into_values()))
        })
        .await
    }
}

/// Parses the root `.gitattributes`, if present and of reasonable size.
fn root_attributes(repo: &gix::Repository, tree: &gix::Tree<'_>) -> Result<LinguistAttributes, GitError> {
    let Some(entry) = tree.find_entry(".gitattributes") else {
        return Ok(LinguistAttributes::default());
    };
    let is_file = matches!(entry.mode().kind(), GixEntryKind::Blob | GixEntryKind::BlobExecutable);
    if !is_file || repo.find_header(entry.object_id()).map_err(GitError::backend)?.size() > MAX_ATTRIBUTES_BYTES {
        return Ok(LinguistAttributes::default());
    }
    let blob = repo.find_blob(entry.object_id()).map_err(GitError::backend)?;
    Ok(LinguistAttributes::parse(&blob.data.to_str_lossy()))
}

/// Tree visitor that records every counted blob, pruning directories that cannot count.
struct FileCollector<'a> {
    /// Used only for its path tracking.
    location: Recorder,
    attributes: &'a LinguistAttributes,
    /// Pruning is only safe when no attribute could re-include a default-excluded path.
    may_prune: bool,
    files: Vec<(&'static Language, ObjectId, String)>,
    keep_paths: bool,
}

impl<'a> FileCollector<'a> {
    fn new(attributes: &'a LinguistAttributes, keep_paths: bool) -> Self {
        Self { location: Recorder::default(), attributes, may_prune: attributes.is_empty(), files: Vec::new(), keep_paths }
    }

    fn current_path(&self) -> String {
        self.location.path().to_str_lossy().into_owned()
    }
}

impl Visit for FileCollector<'_> {
    fn pop_back_tracked_path_and_set_current(&mut self) {
        self.location.pop_back_tracked_path_and_set_current();
    }

    fn pop_front_tracked_path_and_set_current(&mut self) {
        self.location.pop_front_tracked_path_and_set_current();
    }

    fn push_back_tracked_path_component(&mut self, component: &BStr) {
        self.location.push_back_tracked_path_component(component);
    }

    fn push_path_component(&mut self, component: &BStr) {
        self.location.push_path_component(component);
    }

    fn pop_path_component(&mut self) {
        self.location.pop_path_component();
    }

    fn visit_tree(&mut self, _entry: &EntryRef<'_>) -> Action {
        let descend = !(self.may_prune && exclusions::is_excluded_directory(&self.current_path()));
        Action::Continue(descend)
    }

    fn visit_nontree(&mut self, entry: &EntryRef<'_>) -> Action {
        let path = self.current_path();
        if matches!(entry.mode.kind(), GixEntryKind::Blob | GixEntryKind::BlobExecutable)
            && let Some(language) = classify(&path, self.attributes)
        {
            self.files.push((language, entry.oid.to_owned(), if self.keep_paths { path } else { String::new() }));
        }
        Action::Continue(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn language(name: &str) -> &'static Language {
        find_by_name(name).unwrap()
    }

    #[test]
    fn detects_by_filename_then_extension() {
        assert_eq!(detect("src/main.rs").map(|l| l.name), Some("Rust"));
        assert_eq!(detect("Makefile").map(|l| l.name), Some("Makefile"));
        assert_eq!(detect("docker/Dockerfile").map(|l| l.name), Some("Dockerfile"));
        assert_eq!(detect("CMakeLists.txt").map(|l| l.name), Some("CMake"));
        assert_eq!(detect("Main.JAVA").map(|l| l.name), Some("Java"));
        assert_eq!(detect("analysis.R").map(|l| l.name), Some("R"));
        assert_eq!(detect("README"), None);
        assert_eq!(detect(".env"), None);
        assert_eq!(detect("archive.unknownext"), None);
    }

    #[test]
    fn only_programming_and_markup_count_by_default() {
        let none = LinguistAttributes::default();
        assert_eq!(classify("src/lib.rs", &none).map(|l| l.name), Some("Rust"));
        assert_eq!(classify("web/index.html", &none).map(|l| l.name), Some("HTML"));
        assert_eq!(classify("config.yaml", &none), None, "data");
        assert_eq!(classify("README.md", &none), None, "prose");
        assert_eq!(classify("Cargo.lock", &none), None, "generated");
        assert_eq!(classify("vendor/lib.go", &none), None, "vendored");
        assert_eq!(classify("docs/conf.py", &none), None, "documentation");
        assert_eq!(classify("static/app.min.js", &none), None, "minified");
    }

    #[test]
    fn attributes_override_defaults() {
        let attributes = LinguistAttributes::parse(
            "vendor/** -linguist-vendored\n\
             *.sql linguist-detectable\n\
             src/gen/** linguist-generated\n\
             *.inc linguist-language=php\n\
             legacy.py linguist-detectable=false\n\
             docs/** linguist-documentation=false\n",
        );
        assert_eq!(classify("vendor/lib.go", &attributes).map(|l| l.name), Some("Go"));
        assert_eq!(classify("db/schema.sql", &attributes).map(|l| l.name), Some("SQL"));
        assert_eq!(classify("src/gen/types.rs", &attributes), None);
        assert_eq!(classify("lib/helpers.inc", &attributes).map(|l| l.name), Some("PHP"));
        assert_eq!(classify("legacy.py", &attributes), None);
        assert_eq!(classify("docs/site.ts", &attributes).map(|l| l.name), Some("TypeScript"));
    }

    #[test]
    fn stats_are_sorted_with_percentages() {
        let stats = LanguageStats::from_byte_counts([(language("Rust"), 600), (language("HTML"), 300), (language("CSS"), 100)]);
        assert_eq!(stats.total_bytes, 1000);
        let names: Vec<_> = stats.languages.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Rust", "HTML", "CSS"]);
        assert!((stats.languages[0].percent - 60.0).abs() < f64::EPSILON);
        assert_eq!(stats.languages[0].color, "#dea584");
    }

    #[test]
    fn long_tail_is_folded_into_other() {
        let names = ["Rust", "Go", "Python", "Java", "C", "C++", "Ruby", "PHP", "Lua", "Perl"];
        // Ten languages of 1000 bytes each, plus one tiny one under 0.5 %.
        let mut counts: Vec<_> = names.iter().map(|name| (language(name), 1000)).collect();
        counts.push((language("Zig"), 10));
        let stats = LanguageStats::from_byte_counts(counts);

        assert_eq!(stats.languages.len(), MAX_LISTED_LANGUAGES + 1);
        let other = stats.languages.last().unwrap();
        assert_eq!(other.name, OTHER_LANGUAGE_NAME);
        assert_eq!(other.bytes, 2010);
        let sum: f64 = stats.languages.iter().map(|l| l.percent).sum();
        assert!((sum - 100.0).abs() < 1e-9);
    }

    #[test]
    fn tiny_languages_are_folded_even_within_the_top_eight() {
        let stats = LanguageStats::from_byte_counts([(language("Rust"), 10_000), (language("Shell"), 20)]);
        let names: Vec<_> = stats.languages.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Rust", OTHER_LANGUAGE_NAME]);
    }

    #[test]
    fn empty_input_gives_empty_stats() {
        let stats = LanguageStats::from_byte_counts([]);
        assert!(stats.is_empty());
        assert_eq!(stats.total_bytes, 0);
    }
}
