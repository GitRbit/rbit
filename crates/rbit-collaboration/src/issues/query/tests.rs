//! Original query language behavior.

use super::*;

#[test]
fn parses_every_qualifier() {
    let query = IssueQuery::parse(
        r#"is:open is:pr is:merged is:draft is:locked author:alice assignee:@me label:bug label:"good first issue" -label:wontfix milestone:"v1.0" sort:updated-asc crash   boot"#,
    );
    assert_eq!(query.state, Some(StateFilter::Open));
    assert_eq!(query.kind, Some(KindFilter::Pull));
    assert!(query.merged && query.draft && query.locked);
    assert_eq!(query.author, Some(Who::Name("alice".into())));
    assert_eq!(query.assignee, Some(Who::Me));
    assert_eq!(query.labels, ["bug", "good first issue"]);
    assert_eq!(query.not_labels, ["wontfix"]);
    assert_eq!(query.milestone.as_deref(), Some("v1.0"));
    assert_eq!(query.sort, IssueSort::UpdatedAsc);
    assert_eq!(query.text, ["crash", "boot"]);
    assert_eq!(query.ignored, Vec::<String>::new());
}

type Check = fn(&IssueQuery) -> bool;

#[test]
fn table_of_terms() {
    let cases: &[(&str, Check)] = &[
        ("", |q| q.state == Some(StateFilter::Open) && q.kind == Some(KindFilter::Issue)),
        ("   ", |q| q.state == Some(StateFilter::Open)),
        ("IS:Closed", |q| q.state == Some(StateFilter::Closed) && q.kind.is_none()),
        ("is:issue", |q| q.kind == Some(KindFilter::Issue) && q.state.is_none()),
        ("no:assignee no:label no:milestone", |q| q.no_assignee && q.no_label && q.no_milestone),
        ("author:@alice", |q| q.author == Some(Who::Name("alice".into()))),
        ("label:bug label:BUG", |q| q.labels == ["bug"]),
        ("foo:bar", |q| q.ignored == ["foo:bar"] && q.text.is_empty()),
        ("sort:sideways", |q| q.ignored == ["sort:sideways"] && q.sort == IssueSort::CreatedDesc),
        ("-author:x", |q| q.ignored == ["-author:x"]),
        ("label:", |q| q.text == ["label:"]),
        ("http://x", |q| q.ignored == ["http://x"]),
        ("\"two words\"", |q| q.text == ["two words"]),
        ("-", |q| q.text.is_empty() && q.ignored.is_empty()),
    ];
    for (input, check) in cases {
        assert!(check(&IssueQuery::parse(input)), "{input:?}: {:?}", IssueQuery::parse(input));
    }
}

#[test]
fn reads_at_most_twenty_terms() {
    let input = (0..25).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
    let query = IssueQuery::parse(&input);
    assert_eq!(query.text.len(), MAX_TERMS);
    assert_eq!(query.ignored.len(), 5);
}

#[test]
fn writes_the_canonical_query_back() {
    let query = IssueQuery::parse(r#"crash label:"good first issue" is:open sort:comments-desc assignee:bob"#);
    let text = query.to_string();
    assert_eq!(text, r#"is:open assignee:bob label:"good first issue" sort:comments-desc crash"#);
    assert_eq!(IssueQuery::parse(&text), query);
    assert_eq!(IssueQuery::parse("").to_string(), DEFAULT_QUERY);
}

#[test]
fn modifiers_change_one_part() {
    let query = IssueQuery::parse("is:open is:issue no:label");
    assert_eq!(query.with_state(StateFilter::Closed).to_string(), "is:closed is:issue no:label");
    let labelled = query.toggling_label("bug");
    assert_eq!(labelled.to_string(), "is:open is:issue label:bug");
    assert_eq!(labelled.toggling_label("BUG").to_string(), "is:open is:issue");
    assert_eq!(query.with_sort(IssueSort::UpdatedDesc).to_string(), "is:open is:issue no:label sort:updated-desc");
    assert_eq!(query.without_assignee().with_assignee(Some(Who::Me)).to_string(), "is:open is:issue assignee:@me no:label");
    assert_eq!(query.without_milestone().to_string(), "is:open is:issue no:label no:milestone");
    assert_eq!(query.with_milestone(Some("v 1".into())).to_string(), r#"is:open is:issue no:label milestone:"v 1""#);
    assert_eq!(query.with_author(Some(Who::Name("al".into()))).to_string(), "is:open is:issue author:al no:label");
}
