#[cfg(feature = "std")]
use crate::ast::{MySqlText, MySqlTextSegment, SqlSourceAdmission, SqlSourceOwner};
#[cfg(feature = "std")]
use std::sync::Arc;

#[cfg(feature = "std")]
#[test]
fn source_admission_shared_guard_roundtrip_keeps_one_arc_and_exact_layout() {
    let guard: Arc<dyn Send + Sync> = Arc::new(());
    let original = Arc::clone(&guard);
    let admission = SqlSourceAdmission::from_shared_guard(guard);
    assert_eq!(Arc::strong_count(&original), 2);
    let guard = admission.into_shared_guard();
    assert!(Arc::ptr_eq(&original, &guard));
    assert_eq!(Arc::strong_count(&original), 2);
    let admission = SqlSourceAdmission::from_shared_guard(guard);
    let cloned = admission.clone();
    assert_eq!(Arc::strong_count(&original), 3);
    drop(admission);
    let guard = cloned.into_shared_guard();
    assert!(Arc::ptr_eq(&original, &guard));
    drop(guard);
    assert_eq!(Arc::strong_count(&original), 1);
    assert_eq!(
        core::mem::size_of::<SqlSourceAdmission>(),
        core::mem::size_of::<Arc<dyn Send + Sync>>(),
    );
    assert_eq!(
        core::mem::size_of::<Option<SqlSourceAdmission>>(),
        core::mem::size_of::<Option<Arc<dyn Send + Sync>>>(),
    );
}

#[cfg(feature = "std")]
#[test]
fn shared_guard_moves_back_without_releasing_the_original_source_lifetime() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Lease(Arc<AtomicUsize>);
    impl Drop for Lease {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let guard = Arc::new(Lease(Arc::clone(&drops)));
    let weak = Arc::downgrade(&guard);
    let opaque = SqlSourceAdmission::new(guard).into_shared_guard();
    assert_eq!(weak.strong_count(), 1);
    let typed = SqlSourceAdmission::from_shared_guard(opaque);
    let source = SqlSourceOwner::adopt(b"'x'".to_vec(), typed);
    let occurrence = source.occurrence(0..3).unwrap();
    drop(source);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(weak.strong_count(), 1);
    assert_eq!(occurrence.bytes().unwrap(), b"'x'");
    drop(occurrence);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(weak.strong_count(), 0);
}

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

#[cfg(feature = "std")]
#[test]
fn system_semantic_bytes_require_the_same_original_source_and_preserve_last_owner() {
    use crate::ast::MySqlTextRole;
    let source = SqlSourceOwner::adopt(
        b"'\xff\\0x'".to_vec(),
        SqlSourceAdmission::new(Arc::new(())),
    );
    let original = MySqlText::from_occurrence(
        source.occurrence(1..source.bytes().len() - 1).unwrap(),
        source.occurrence(0..source.bytes().len()).unwrap(),
        "utf8mb3".into(),
        '\'',
        None,
    )
    .unwrap()
    .with_role(MySqlTextRole::System);
    assert!(
        original.system_bytes().is_err(),
        "role alone never issues System"
    );
    let semantic =
        SqlSourceOwner::adopt(b"\xff\0x".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let foreign = SqlSourceOwner::adopt(
        b"'\xff\\0x'".to_vec(),
        SqlSourceAdmission::new(Arc::new(())),
    );
    let mut rejected = original.clone();
    assert!(rejected
        .bind_system(semantic.occurrence(0..3).unwrap(), &foreign)
        .is_err());
    assert!(rejected.system_bytes().is_err());
    let mut wrong_role = original.clone().with_role(MySqlTextRole::Literal);
    assert!(wrong_role
        .bind_system(semantic.occurrence(0..3).unwrap(), &source)
        .is_err());
    let source_weak = Arc::downgrade(&source);
    let semantic_weak = Arc::downgrade(&semantic);
    let mut received = original.clone();
    received
        .bind_system(semantic.occurrence(0..3).unwrap(), &source)
        .unwrap();
    assert_eq!(received.system_bytes().unwrap(), b"\xff\0x");
    assert_eq!(received.charset(), "utf8mb3");
    assert!(
        received.declaration_bytes().is_err(),
        "System does not issue a type member"
    );
    drop((original, rejected, wrong_role, source, semantic, foreign));
    assert!(source_weak.upgrade().is_some());
    assert!(semantic_weak.upgrade().is_some());
    let clone = received.clone();
    drop(received);
    assert_eq!(clone.system_bytes().unwrap(), b"\xff\0x");
    drop(clone);
    assert!(source_weak.upgrade().is_none());
    assert!(semantic_weak.upgrade().is_none());
}

#[cfg(all(feature = "std", feature = "serde"))]
#[test]
fn serialized_system_facts_do_not_restore_source_or_declaration_authority() {
    use crate::ast::MySqlTextRole;
    let source = SqlSourceOwner::adopt(b"'\xff'".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let semantic = SqlSourceOwner::adopt(b"\xff".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let mut received = MySqlText::from_occurrence(
        source.occurrence(1..2).unwrap(),
        source.occurrence(0..3).unwrap(),
        "utf8mb3".into(),
        '\'',
        None,
    )
    .unwrap()
    .with_role(MySqlTextRole::System);
    received
        .bind_system(semantic.occurrence(0..1).unwrap(), &source)
        .unwrap();
    let detached: MySqlText =
        serde_json::from_str(&serde_json::to_string(&received).unwrap()).unwrap();
    assert!(detached.is_system(), "durable grammar state is transported");
    assert!(
        detached.system_bytes().is_err(),
        "it has no actual Source admission"
    );
    assert!(detached.declaration_bytes().is_err());
    assert!(detached.source().owner().is_none());
    assert_eq!(received.system_bytes().unwrap(), b"\xff");
}

#[cfg(feature = "std")]
#[test]
fn projection_alias_system_fact_is_same_source_only_and_keeps_its_last_owner() {
    use crate::ast::MySqlTextRole;
    let source = SqlSourceOwner::adopt(
        b"'\xff\\0x'".to_vec(),
        SqlSourceAdmission::new(Arc::new(())),
    );
    let semantic =
        SqlSourceOwner::adopt(b"\xff\0x".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let original = MySqlText::from_occurrence(
        source.occurrence(1..source.bytes().len() - 1).unwrap(),
        source.occurrence(0..source.bytes().len()).unwrap(),
        "utf8mb3".into(),
        '\'',
        None,
    )
    .unwrap()
    .with_role(MySqlTextRole::Alias);
    assert!(
        original.alias_system_bytes().is_err(),
        "role does not issue accepted bytes"
    );
    let foreign = SqlSourceOwner::adopt(
        source.bytes().to_vec(),
        SqlSourceAdmission::new(Arc::new(())),
    );
    let mut rejected = original.clone();
    assert!(rejected
        .bind_alias_system(semantic.occurrence(0..3).unwrap(), &foreign)
        .is_err());
    assert!(rejected.alias_system_bytes().is_err());
    let mut wrong_role = original.clone().with_role(MySqlTextRole::System);
    assert!(wrong_role
        .bind_alias_system(semantic.occurrence(0..3).unwrap(), &source)
        .is_err());
    let source_weak = Arc::downgrade(&source);
    let semantic_weak = Arc::downgrade(&semantic);
    let mut received = original.clone();
    received
        .bind_alias_system(semantic.occurrence(0..3).unwrap(), &source)
        .unwrap();
    assert_eq!(received.alias_system_bytes().unwrap(), b"\xff\0x");
    assert!(received.system_bytes().is_err());
    assert!(received.declaration_bytes().is_err());
    assert!(received.canonical_identifier_occurrence().is_none());
    assert!(received
        .clone()
        .with_role(MySqlTextRole::Literal)
        .alias_system_bytes()
        .is_err());
    drop((original, rejected, wrong_role, source, semantic, foreign));
    let clone = received.clone();
    drop(received);
    assert!(source_weak.upgrade().is_some());
    assert!(semantic_weak.upgrade().is_some());
    assert_eq!(clone.alias_system_bytes().unwrap(), b"\xff\0x");
    drop(clone);
    assert!(source_weak.upgrade().is_none());
    assert!(semantic_weak.upgrade().is_none());
}

#[cfg(all(feature = "std", feature = "serde"))]
#[test]
fn alias_transport_and_reattachment_do_not_reissue_accepted_alias_or_ident() {
    use crate::ast::MySqlTextRole;
    let source = SqlSourceOwner::adopt(b"'\xff'".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let semantic = SqlSourceOwner::adopt(b"\xff".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let mut original = MySqlText::from_occurrence(
        source.occurrence(1..2).unwrap(),
        source.occurrence(0..3).unwrap(),
        "utf8mb3".into(),
        '\'',
        None,
    )
    .unwrap()
    .with_role(MySqlTextRole::Alias);
    original
        .bind_alias_system(semantic.occurrence(0..1).unwrap(), &source)
        .unwrap();
    let mut detached: MySqlText =
        serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
    assert_eq!(detached.role(), MySqlTextRole::Alias);
    assert!(detached.alias_system_bytes().is_err());
    assert!(detached.canonical_identifier_occurrence().is_none());
    detached
        .reattach(&source, SqlSourceAdmission::new(Arc::new(())))
        .unwrap();
    assert_eq!(detached.bytes().unwrap(), b"\xff");
    assert!(
        detached.alias_system_bytes().is_err(),
        "admission is not the real alias reduction"
    );
    assert!(detached.canonical_identifier_occurrence().is_none());
    assert!(detached
        .clone()
        .with_role(MySqlTextRole::Literal)
        .canonical_identifier_occurrence()
        .is_none());
    assert_eq!(original.alias_system_bytes().unwrap(), b"\xff");
}
