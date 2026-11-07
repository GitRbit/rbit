//! Cursor syntax and ordering stay compatible without runtime or event payload defaults.
use rbit_collaboration::{
    comments::CommentId,
    timeline::{EventId, Timeline, TimelineCursor, TimelineItemId, TimelinePage},
};
use rbit_contracts::Timestamp;
#[test]
fn cursor_wire_order_and_default_pages_preserve_original_contract() {
    struct NonDefault;
    let cursor = TimelineCursor { at: Timestamp(17), item: TimelineItemId::Comment(CommentId(42)) };
    assert_eq!(cursor.to_param(), "17-c42");
    assert_eq!(TimelineCursor::parse("17-c42"), Some(cursor));
    assert_eq!(cursor.key(), (17, 0, 42));
    let event = TimelineCursor { at: Timestamp(17), item: TimelineItemId::Event(EventId(1)) };
    assert!(cursor < event);
    assert_eq!(event.key(), (17, 1, 1));
    for value in ["-1-c1", "1-c0", "1-e-1", "1-x1", "1-c1-extra", ""] {
        assert_eq!(TimelineCursor::parse(value), None);
    }
    assert_eq!(TimelinePage::<NonDefault>::default().items.len(), 0);
    assert_eq!(Timeline::<NonDefault>::default().entries.len(), 0);
    assert_eq!(serde_json::to_string(&EventId(-7)).unwrap(), "-7");
}
