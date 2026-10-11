#[cfg(feature = "std")]
use crate::ast::{MySqlText, MySqlTextSegment, SqlSourceAdmission, SqlSourceOwner};
#[cfg(feature = "std")]
use std::sync::Arc;

#[cfg(feature = "std")]
#[test]
fn adjacent_text_display_prevalidates_all_contributions_and_preserves_first_source() {
    let source = SqlSourceOwner::adopt(
        b"'first' 'second'".to_vec(),
        SqlSourceAdmission::new(Arc::new(())),
    );
    let semantic = SqlSourceOwner::adopt(
        b"firstsecond".to_vec(),
        SqlSourceAdmission::new(Arc::new(())),
    );
    let text = MySqlText::from_segments(
        semantic.occurrence(0..11).unwrap(),
        vec![
            MySqlTextSegment::new(
                0..5,
                source.occurrence(0..7).unwrap(),
                "utf8mb4".into(),
                '\'',
                None,
            )
            .unwrap(),
            MySqlTextSegment::new(
                5..11,
                source.occurrence(8..16).unwrap(),
                "utf8mb4".into(),
                '\'',
                None,
            )
            .unwrap(),
        ],
        SqlSourceAdmission::new(Arc::new(())),
    )
    .unwrap();
    assert_eq!(text.bytes().unwrap(), b"firstsecond");
    assert_eq!(
        text.source().bytes().unwrap(),
        b"'first'",
        "labels still use actual first segment"
    );
    assert_eq!(text.to_string(), "'first' 'second'");
    let cloned = text.clone();
    let weak = Arc::downgrade(&source);
    drop((source, semantic, text));
    assert!(weak.upgrade().is_some());
    assert_eq!(cloned.to_string(), "'first' 'second'");
    drop(cloned);
    assert!(
        weak.upgrade().is_none(),
        "final physical source owner released"
    );
}

#[cfg(feature = "std")]
#[test]
fn invalid_late_text_contribution_does_not_publish_a_valid_prefix() {
    let source = SqlSourceOwner::adopt(
        b"'first' '\xe9'".to_vec(),
        SqlSourceAdmission::new(Arc::new(())),
    );
    let semantic =
        SqlSourceOwner::adopt(b"first\xe9".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let text = MySqlText::from_segments(
        semantic.occurrence(0..6).unwrap(),
        vec![
            MySqlTextSegment::new(
                0..5,
                source.occurrence(0..7).unwrap(),
                "latin1".into(),
                '\'',
                None,
            )
            .unwrap(),
            MySqlTextSegment::new(
                5..6,
                source.occurrence(8..11).unwrap(),
                "latin1".into(),
                '\'',
                None,
            )
            .unwrap(),
        ],
        SqlSourceAdmission::new(Arc::new(())),
    )
    .unwrap();
    assert_eq!(text.source().bytes().unwrap(), b"'first'");
    assert_eq!(
        text.bytes().unwrap(),
        b"first\xe9",
        "valid native bytes are not fabricated Unicode"
    );
    let mut displayed = String::new();
    assert!(std::fmt::write(&mut displayed, format_args!("{text}")).is_err());
    assert!(
        displayed.is_empty(),
        "all source windows were validated before writing"
    );
}

#[cfg(all(feature = "std", feature = "serde"))]
#[test]
fn detached_text_display_cannot_reissue_source_from_serde_or_valid_semantics() {
    let source = SqlSourceOwner::adopt(b"'x'".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let text = MySqlText::from_occurrence(
        source.occurrence(1..2).unwrap(),
        source.occurrence(0..3).unwrap(),
        "utf8mb4".into(),
        '\'',
        None,
    )
    .unwrap();
    let detached: MySqlText = serde_json::from_str(&serde_json::to_string(&text).unwrap()).unwrap();
    let mut displayed = String::new();
    assert!(std::fmt::write(&mut displayed, format_args!("{detached}")).is_err());
    assert!(displayed.is_empty());
    assert!(detached.source().owner().is_none());
    assert_eq!(text.to_string(), "'x'");
}
