//! Repository page families: every key renders in English, keeps a unique id and has its
//! own Spanish entry.
use std::collections::BTreeSet;

use rbit_localization::{
    Catalog, ContributionLabel, ContributionMessage, Locale, MessageFamily, ReleaseLabel, ReleaseMessage, RepoBrowseLabel,
    RepoBrowseMessage, RepoCreateLabel, RepoCreateMessage, RepoEditLabel, RepoEditMessage, RepoHistoryLabel, RepoHistoryMessage,
    RepoNavLabel, RepoNavMessage, RepoSettingsLabel, RepoSettingsMessage,
};

/// Renders `messages` in every catalog and returns their ids. English and the diagnostic
/// catalogs never fall back, and neither does Spanish, which translates every key.
fn check<M: MessageFamily + std::fmt::Debug>(messages: &[M]) -> Vec<&'static str> {
    for &message in messages {
        let english = Catalog::new(Locale::English).localized(message);
        assert!(!english.as_str().trim().is_empty(), "{message:?}");
        assert!(!english.fallback(), "{message:?}");
        for locale in [Locale::PseudoExpanded, Locale::PseudoRtl] {
            assert!(!Catalog::new(locale).localized(message).fallback(), "{message:?} in {locale:?}");
        }
        let spanish = Catalog::new(Locale::Spanish).localized(message);
        assert!(!spanish.as_str().trim().is_empty(), "{message:?}");
        assert!(!spanish.fallback(), "{message:?} has a Spanish entry");
    }
    messages.iter().map(|message| message.id()).collect()
}

#[test]
fn repository_labels_render_in_english_and_translate_to_spanish() {
    let mut ids = Vec::new();
    ids.extend(check(RepoNavLabel::ALL));
    ids.extend(check(RepoBrowseLabel::ALL));
    ids.extend(check(RepoHistoryLabel::ALL));
    ids.extend(check(RepoEditLabel::ALL));
    ids.extend(check(RepoCreateLabel::ALL));
    ids.extend(check(RepoSettingsLabel::ALL));
    ids.extend(check(ReleaseLabel::ALL));
    ids.extend(check(ContributionLabel::ALL));
    let unique: BTreeSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "label ids are unique across the repository families");
}

#[test]
fn repository_messages_render_arguments_and_plurals() {
    let english = |message: &dyn Fn(&Catalog) -> String| message(&Catalog::new(Locale::English));
    assert_eq!(english(&|c| c.localized(RepoNavMessage::ForkCount { count: 1 }).to_string()), "1 fork");
    assert_eq!(english(&|c| c.localized(RepoNavMessage::StargazerCount { count: 2 }).to_string()), "2 stargazers");
    assert_eq!(english(&|c| c.localized(RepoBrowseMessage::BranchNoun { count: 1 }).to_string()), "branch");
    assert_eq!(english(&|c| c.localized(RepoBrowseMessage::LineCount { count: 12 }).to_string()), "12 lines");
    assert_eq!(
        english(&|c| c.localized(RepoBrowseMessage::ListingResults { total: 40, page: 1, pages: 2 }).to_string()),
        "40 results · 1 / 2"
    );
    assert_eq!(english(&|c| c.localized(RepoHistoryMessage::FilesChangedNoun { count: 1 }).to_string()), "file changed");
    assert_eq!(english(&|c| c.localized(RepoHistoryMessage::MoreBranches { count: 3 }).to_string()), "3 more branches");
    assert_eq!(
        english(&|c| c.localized(RepoEditMessage::UploadLimits { files: 100, size: "10 MiB" }).to_string()),
        "Up to 100 files of at most 10 MiB each, committed together."
    );
    assert_eq!(english(&|c| c.localized(RepoCreateMessage::AtLeastMinutes { minutes: 10 }).to_string()), "At least 10 minutes.");
    assert_eq!(
        english(&|c| c.localized(RepoSettingsMessage::PushSchedule { minutes: 1 }).to_string()),
        "Every mirror is pushed at least every 1 minute."
    );
    assert_eq!(english(&|c| c.localized(ReleaseMessage::Downloads { count: 1 }).to_string()), "1 download");
    assert_eq!(english(&|c| c.localized(ContributionMessage::Day { count: 1, date: "2026-05-01" }).to_string()), "1 commit on 2026-05-01");
}

#[test]
fn repository_messages_translate_to_spanish_with_plain_arguments() {
    let mut ids = Vec::new();
    ids.extend(check(&[RepoNavMessage::ForkCount { count: 2 }, RepoNavMessage::StargazerCount { count: 1 }]));
    ids.extend(check(&[
        RepoBrowseMessage::BranchNoun { count: 2 },
        RepoBrowseMessage::TagNoun { count: 1 },
        RepoBrowseMessage::CollaboratorAccess { permission: "Write" },
        RepoBrowseMessage::LineCount { count: 1 },
        RepoBrowseMessage::LanguageFiles { language: "Rust" },
        RepoBrowseMessage::ListingResults { total: 1, page: 1, pages: 1 },
        RepoBrowseMessage::RefPage { page: 1, pages: 3, total: 52 },
        RepoBrowseMessage::ForkTitle { owner: "<alice>", name: "project" },
    ]));
    ids.extend(check(&[
        RepoHistoryMessage::CommitsOn { date: "May 1" },
        RepoHistoryMessage::ViewCommit { id: "abc" },
        RepoHistoryMessage::ParentNoun { count: 2 },
        RepoHistoryMessage::BlameOf { file: "a.rs" },
        RepoHistoryMessage::BlameTitle { path: "src/a.rs" },
        RepoHistoryMessage::Ahead { count: "1000+" },
        RepoHistoryMessage::Behind { count: "0" },
        RepoHistoryMessage::OnlyNewest { count: 250 },
        RepoHistoryMessage::HighlightTrack { track: 2 },
        RepoHistoryMessage::Track { track: 2 },
        RepoHistoryMessage::BranchName { name: "main" },
        RepoHistoryMessage::MoreBranches { count: 1 },
        RepoHistoryMessage::FilesChangedNoun { count: 3 },
        RepoHistoryMessage::CompareTitle { base: "main", head: "dev" },
    ]));
    ids.extend(check(&[
        RepoEditMessage::AtMost { size: "1 MiB" },
        RepoEditMessage::UploadLimits { files: 1, size: "1 MiB" },
        RepoEditMessage::BranchChanged { branch: "main" },
        RepoEditMessage::BranchMissing { branch: "main" },
        RepoEditMessage::NowPointsAt,
        RepoEditMessage::NotEditable { limit: "1 MiB" },
        RepoEditMessage::EditingTitle { path: "a.rs" },
        RepoEditMessage::DeleteTitle { path: "a.rs" },
    ]));
    ids.extend(check(&[
        RepoCreateMessage::AtLeastMinutes { minutes: 1 },
        RepoCreateMessage::ImportFrom { source: "GitHub" },
        RepoCreateMessage::MoreWarnings { count: 3 },
        RepoCreateMessage::MirroringName { name: "project" },
        RepoCreateMessage::FetchFailed { error: "timeout" },
    ]));
    ids.extend(check(&[
        RepoSettingsMessage::AtLeastMinutes { minutes: 5 },
        RepoSettingsMessage::PushSchedule { minutes: 60 },
        RepoSettingsMessage::DeleteKey { title: "ci" },
    ]));
    ids.extend(check(&[
        ReleaseMessage::Checksum { sha256: "ab" },
        ReleaseMessage::Downloads { count: 3 },
        ReleaseMessage::RemoveFile { name: "a.zip" },
        ReleaseMessage::UploadLimit { size: "100 MiB" },
        ReleaseMessage::Uploaded { name: "a.zip" },
        ReleaseMessage::Removed { name: "a.zip" },
        ReleaseMessage::EditTitle { tag: "v1" },
    ]));
    ids.extend(check(&[
        ContributionMessage::CommitsInYear { count: 1, year: 2026 },
        ContributionMessage::ActivityInYear { year: 2026 },
        ContributionMessage::Description { count: 3, days: 2 },
        ContributionMessage::Day { count: 2, date: "2026-05-01" },
        ContributionMessage::QuarterActivity { quarter: "Q1", year: 2026 },
        ContributionMessage::Days { count: 4 },
    ]));
    let unique: BTreeSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "message ids are unique");
    let fork = Catalog::new(Locale::Spanish).localized(RepoBrowseMessage::ForkTitle { owner: "<alice>", name: "project" });
    assert_eq!(fork.as_str(), "Bifurcar <alice>/project", "arguments stay plain text");
}
