//! The run layout's messages exist, render in every catalog and are translated in every
//! translated locale.
use std::collections::BTreeSet;

use rbit_localization::{Catalog, Locale, MessageFamily, RunLayoutMessage};

const DERIVED: [Locale; 3] = [Locale::English, Locale::PseudoExpanded, Locale::PseudoRtl];
const TRANSLATED: [Locale; 8] = [
    Locale::German,
    Locale::Spanish,
    Locale::French,
    Locale::Japanese,
    Locale::Korean,
    Locale::PortugueseBrazil,
    Locale::Russian,
    Locale::ChineseSimplified,
];

fn messages() -> Vec<RunLayoutMessage<'static>> {
    use RunLayoutMessage as M;
    vec![
        M::Summary,
        M::Navigation,
        M::Usage,
        M::WorkflowFile,
        M::JobCount { count: 1 },
        M::JobCount { count: 5 },
        M::Graph,
        M::MoreJobs { count: 4 },
        M::Annotations,
        M::Errors { count: 2 },
        M::Warnings { count: 1 },
        M::Notices { count: 3 },
        M::ErrorLevel,
        M::WarningLevel,
        M::NoticeLevel,
        M::StepFailed { step: "<Test>" },
        M::MoreAnnotations { count: 10 },
        M::TriggeredVia,
        M::Status,
        M::TotalDuration,
        M::RerunBy { name: "alice" },
        M::Inputs,
        M::Waited { time: "12s" },
        M::RanOn { runner: "box-1" },
    ]
}

#[test]
fn run_layout_keys_render_and_translate() {
    let messages = messages();
    let ids: BTreeSet<_> = messages.iter().map(|message| message.id()).collect();
    assert_eq!(ids.len(), 23, "ids are unique and every variant is listed");
    for &message in &messages {
        for locale in DERIVED {
            let text = Catalog::new(locale).localized(message);
            assert!(!text.as_str().is_empty() && !text.fallback(), "{message:?} in {locale:?}");
        }
        for locale in TRANSLATED {
            let text = Catalog::new(locale).localized(message);
            assert!(!text.as_str().is_empty() && !text.fallback(), "{message:?} is translated in {locale:?}");
        }
    }
}

#[test]
fn counts_choose_their_plural_forms() {
    let count = |locale, count| Catalog::new(locale).localized(RunLayoutMessage::JobCount { count }).as_str().to_owned();
    assert_eq!(count(Locale::English, 1), "1 job");
    assert_eq!(count(Locale::English, 3), "3 jobs");
    assert_eq!(count(Locale::Spanish, 2), "2 trabajos");
    assert_eq!(count(Locale::Russian, 3), "3 задания");
    assert_eq!(count(Locale::Russian, 5), "5 заданий");
    assert_eq!(Catalog::new(Locale::English).localized(RunLayoutMessage::MoreJobs { count: 4 }).as_str(), "+4 more");
    let errors = |locale, count| Catalog::new(locale).localized(RunLayoutMessage::Errors { count }).as_str().to_owned();
    assert_eq!((errors(Locale::English, 1), errors(Locale::English, 2)), ("1 error".to_owned(), "2 errors".to_owned()));
    assert_eq!(errors(Locale::Russian, 2), "2 ошибки");
    let failed = Catalog::new(Locale::Spanish).localized(RunLayoutMessage::StepFailed { step: "<Test>" });
    assert_eq!(failed.as_str(), "Falló el paso <Test>.", "names stay plain text");
}
