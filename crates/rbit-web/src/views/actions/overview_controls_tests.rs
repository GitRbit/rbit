//! Production option mapping and dispatch include stay plain escaped text in every catalog.
use super::*;
use rbit_localization::{Catalog, Locale, OverviewControlMessage};

#[test]
fn every_machine_filter_retains_its_value_selection_and_exact_english_label() {
    let catalog = Catalog::new(Locale::English);
    for event in RunEvent::ALL {
        for status in Status::ALL {
            let filters = RunFilters::new("/actions".into(), "main", "alice", Some(event), Some(status));
            for (option, expected) in filters.events.iter().zip(RunEvent::ALL) {
                assert_eq!(option.value, expected.as_str());
                assert_eq!(catalog.overview_control(option.label).to_string(), event_label(expected));
                assert_eq!(option.selected, expected == event);
            }
            for (option, expected) in filters.statuses.iter().zip(Status::ALL) {
                assert_eq!(option.value, expected.as_str());
                assert_eq!(catalog.overview_control(option.label).to_string(), StateLook::of(expected).label);
                assert_eq!(option.selected, expected == status);
            }
        }
    }
}

#[derive(Template)]
#[template(source = "{% import \"macros.html\" as ui %}{% include \"actions/dispatch.html\" %}", ext = "html")]
struct Dispatch<'a> {
    form: &'a DispatchFormView,
    text: super::super::OverviewText,
}

#[test]
fn actual_dispatch_include_localizes_only_controls_and_escapes_user_supplied_values() {
    let hostile = "<script>雪 & \"unsafe\"</script>";
    let form = DispatchFormView {
        action: "/actions/dispatch".into(),
        page_url: "/actions/workflow".into(),
        reference: hostile.into(),
        refs: vec![hostile.into()],
        inputs: vec![super::super::DispatchInputView {
            name: hostile.into(),
            field: "inputs[choice]".into(),
            id: "dispatch-input-0".into(),
            description: Some(hostile.into()),
            required: true,
            kind: "choice",
            value: hostile.into(),
            options: vec![super::super::DispatchOption { value: hostile.into(), selected: true }],
        }],
        error: Some(hostile.into()),
        can_run: true,
        open: true,
        name: None,
    };
    for locale in [Locale::English, Locale::Spanish, Locale::PseudoExpanded, Locale::PseudoRtl] {
        let output = Dispatch { form: &form, text: super::super::OverviewText::new(locale, 0) }.render().unwrap();
        let expected = Catalog::new(locale).overview_control(OverviewControlMessage::RunWorkflow).to_string();
        assert_eq!(output.matches(&expected).count(), 2);
        assert!(!output.contains("<script>"));
        assert!(output.contains("&#60;script&#62;雪 &#38; &#34;unsafe&#34;&#60;/script&#62;"));
        for field in ["method=\"get\"", "method=\"post\"", "name=\"ref\"", "name=\"inputs[choice]\"", "selected", "required"] {
            assert!(output.contains(field), "missing native contract {field}");
        }
    }
}
