use super::super::*;
use crate::ast::{SqlSourceAdmission, SqlSourceOwner};
use core::cell::Cell;
use std::sync::Arc;

// The adapter delegates the same finite lexical fixtures above; it adds only
// observation configuration. HashService unavailable is intentional here.
// Native SHA256 and real charset/budget ownership are tested by the MySQL
// dialect mirror with its existing actual native adapter.
struct CanonicalFacts {
    inner: CommentFacts,
    config: MySqlDigestConfig,
    hash_calls: Cell<usize>,
}
impl MySqlSourceAdapter for CanonicalFacts {
    fn client_charset(&self) -> &str {
        self.inner.client_charset()
    }
    fn client_min_width(&self) -> u8 {
        self.inner.client_min_width()
    }
    fn lexical_unit(&self, b: &[u8]) -> Result<MySqlLexicalUnit, MySqlSourceError> {
        self.inner.lexical_unit(b)
    }
    fn system_identifier(
        &self,
        b: &[u8],
        q: bool,
    ) -> Result<(String, SqlSourceAdmission), MySqlSourceError> {
        self.inner.system_identifier(b, q)
    }
    fn system_text(&self, t: &MySqlText) -> Result<(String, SqlSourceAdmission), MySqlSourceError> {
        self.inner.system_text(t)
    }
    fn admit(&self, n: usize) -> Result<SqlSourceAdmission, MySqlSourceError> {
        self.inner.admit(n)
    }
    fn canonical_digest_config(&self) -> Option<MySqlDigestConfig> {
        Some(self.config)
    }
    fn canonical_charset_introducer(&self, name: &str) -> Option<bool> {
        Some(matches!(
            name,
            "utf8mb4" | "utf8mb3" | "utf8" | "latin1" | "binary"
        ))
    }
    fn canonical_digest_hash(&self, _: &[u8]) -> Option<[u8; 32]> {
        self.hash_calls.set(self.hash_calls.get() + 1);
        None
    }
}
fn canonical_facts(capacity: usize) -> CanonicalFacts {
    CanonicalFacts {
        inner: first_error_facts(),
        config: MySqlDigestConfig {
            capacity,
            charset_number: 45,
            ignore_space: false,
            high_not_precedence: false,
            pipes_as_concat: false,
        },
        hash_calls: Cell::new(0),
    }
}
fn canonical_tokens(bytes: &[u8], facts: &CanonicalFacts) -> Arc<MySqlCanonicalDigest> {
    let dialect = crate::dialect::MySqlDialect {};
    let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..bytes.len()).unwrap(),
        facts,
        None,
    )
    .unwrap();
    cursor.begin_canonical_digest().unwrap();
    while cursor.next_token().unwrap().is_some() {}
    let MySqlCanonicalDigestReceipt::Collected(receipt) =
        cursor.finish_canonical_digest().unwrap().unwrap()
    else {
        panic!("fixture has actual observation admission");
    };
    receipt
}
fn digest_wire(tokens: &[u16]) -> Vec<u8> {
    tokens
        .iter()
        .flat_map(|token| token.to_le_bytes())
        .collect()
}
#[test]
fn canonical_mapping_is_exact_fixed_native_and_not_rust_discriminants() {
    assert_eq!(mysql_digest_symbol(b"select"), Some((748, false)));
    assert_eq!(mysql_digest_symbol(b"COUNT"), Some((352, true)));
    assert_eq!(mysql_digest_symbol(b"not_a_native_keyword"), None);
    assert!(
        MYSQL_DIGEST_SYMBOLS
            .windows(2)
            .all(|pair| pair[0].0 < pair[1].0)
    );
    for (word, token, function) in MYSQL_DIGEST_SYMBOLS {
        assert_eq!(mysql_digest_symbol(word), Some((*token, *function)));
    }
    for token in [40, 44, 748, 415] {
        assert_eq!(
            mysql_digest_start_expr(token),
            matches!(token, 40 | 44 | 748)
        );
    }
}
#[test]
fn canonical_once_cursor_numeric_list_row_in_and_unary_native_reductions() {
    for (sql, tokens) in [
        (b"SELECT 1".as_slice(), vec![748, 1100]),
        (b"SELECT -+-1, 2, 3", vec![748, 1101]),
        (b"SELECT (1)", vec![748, 1102]),
        (b"SELECT (1,2)", vec![748, 1104]),
        (b"SELECT 1 IN (2,3)", vec![748, 1100, 1110]),
        (b"VALUES (1),(2),(3)", vec![875, 1103]),
        (b"VALUES (1,2),(3,4)", vec![875, 1105]),
    ] {
        let facts = canonical_facts(1024);
        let receipt = canonical_tokens(sql, &facts);
        assert_eq!(receipt.token_bytes(), digest_wire(&tokens), "{:?}", sql);
        assert_eq!(
            receipt.unavailable(),
            Some(MySqlDigestUnavailable::HashService)
        );
        assert_eq!(facts.hash_calls.get(), 1);
        assert!(!receipt.is_full());
    }
}
#[test]
fn canonical_native_mode_function_adjacency_and_with_rollup_are_source_facts() {
    for (sql, ignore, pipes, high, expected) in [
        (
            b"SELECT COUNT(1)".as_slice(),
            false,
            false,
            false,
            vec![748, 352, 1102],
        ),
        (
            b"SELECT NOT 1 || 2",
            false,
            false,
            false,
            vec![748, 622, 1100, 642, 1100],
        ),
        (
            b"SELECT NOT 1 || 2",
            false,
            true,
            true,
            vec![748, 621, 1100, 644, 1100],
        ),
        (b"WITH ROLLUP", false, false, false, vec![894]),
    ] {
        let mut facts = canonical_facts(1024);
        facts.config.ignore_space = ignore;
        facts.config.pipes_as_concat = pipes;
        facts.config.high_not_precedence = high;
        assert_eq!(
            canonical_tokens(sql, &facts).token_bytes(),
            digest_wire(&expected),
            "{:?}",
            sql
        );
    }
    let spaced = canonical_facts(1024);
    let mut ignored = canonical_facts(1024);
    ignored.config.ignore_space = true;
    assert_ne!(
        canonical_tokens(b"SELECT COUNT (1)", &spaced).token_bytes(),
        canonical_tokens(b"SELECT COUNT (1)", &ignored).token_bytes()
    );
}
#[test]
fn canonical_native_identifier_domain_atomic_capacity_and_full_parse_are_distinct() {
    let facts = canonical_facts(1024);
    let unquoted = canonical_tokens(b"SELECT a", &facts);
    let quoted = canonical_tokens(b"SELECT `a`", &facts);
    assert_eq!(unquoted.token_bytes(), quoted.token_bytes());
    let mut expected = digest_wire(&[748, 1106]);
    expected.extend_from_slice(&1u16.to_le_bytes());
    expected.push(b'a');
    assert_eq!(unquoted.token_bytes(), expected);
    for (capacity, used) in [(1, 0), (2, 2), (3, 2), (6, 2), (7, 7)] {
        let facts = canonical_facts(capacity);
        let receipt = canonical_tokens(b"SELECT a", &facts);
        assert_eq!(receipt.token_bytes().len(), used);
        assert_eq!(receipt.is_full(), capacity < 7);
        assert_eq!(
            facts.hash_calls.get(),
            1,
            "configured full is not unsupported"
        );
    }
}
#[test]
fn canonical_comments_cpp_versions_and_real_unsupported_frontiers_never_raw_hash() {
    let plain = canonical_facts(1024);
    assert_eq!(
        canonical_tokens(b"SELECT 1", &plain).token_bytes(),
        canonical_tokens(b"/* ordinary */ SELECT /*!80000 1 */", &plain).token_bytes()
    );
    for (sql, why) in [
        (
            b"SELECT /*+ MAX_EXECUTION_TIME(2) */ 1".as_slice(),
            MySqlDigestUnavailable::Hint,
        ),
        (b"SELECT @@", MySqlDigestUnavailable::VariableRole),
        (b"SELECT ^@a", MySqlDigestUnavailable::Token),
    ] {
        let facts = canonical_facts(1024);
        let receipt = canonical_tokens(sql, &facts);
        assert_eq!(receipt.unavailable(), Some(why));
        assert!(receipt.hash().is_none());
        assert_eq!(
            facts.hash_calls.get(),
            0,
            "unsupported tokens never hash a raw fallback"
        );
    }
}
#[test]
fn canonical_disabled_has_no_new_admission_and_control_failure_precedes_publication() {
    let dialect = crate::dialect::MySqlDialect {};
    let bytes = b"SELECT 1";
    let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let facts = canonical_facts(0);
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..bytes.len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    let calls = facts.inner.admission_calls.get();
    cursor.begin_canonical_digest().unwrap();
    assert_eq!(facts.inner.admission_calls.get(), calls);
    assert!(cursor.digest.is_none());
    assert!(cursor.finish_canonical_digest().unwrap().is_none());
    let facts = canonical_facts(1024);
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..bytes.len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    cursor.begin_canonical_digest().unwrap();
    facts
        .inner
        .fail_lexical
        .set(Some(MySqlSourceErrorKind::DeadlineExceeded));
    let first = cursor.next_token().err().unwrap();
    assert_eq!(first.source_kind, MySqlSourceErrorKind::DeadlineExceeded);
    assert_eq!(cursor.finish_canonical_digest().err().unwrap(), first);
    assert_eq!(facts.hash_calls.get(), 0);
}
#[test]
fn canonical_observation_oom_is_typed_unavailable_without_poisoning_original_cursor() {
    let dialect = crate::dialect::MySqlDialect {};
    let bytes = b"SELECT 1";
    let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let facts = canonical_facts(1024);
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..bytes.len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    facts.inner.refuse_allocations.set(true);
    cursor.begin_canonical_digest().unwrap();
    facts.inner.refuse_allocations.set(false);
    while cursor.next_token().unwrap().is_some() {}
    assert!(matches!(
        cursor.finish_canonical_digest().unwrap(),
        Some(MySqlCanonicalDigestReceipt::Unavailable(
            MySqlDigestUnavailable::Resource
        ))
    ));
    assert_eq!(facts.hash_calls.get(), 0);
}

#[test]
fn canonical_introducer_borrows_registry_and_comment_does_not_create_function_adjacency() {
    let facts = canonical_facts(1024);
    assert_eq!(
        canonical_tokens(b"SELECT _utf8mb4'a'", &facts).token_bytes(),
        digest_wire(&[748, 852, 1100])
    );
    let unknown = canonical_tokens(b"SELECT _unknown", &facts);
    assert_eq!(unknown.token_bytes()[2..4], 1106u16.to_le_bytes());
    let mut ignored = canonical_facts(1024);
    ignored.config.ignore_space = true;
    assert_eq!(
        canonical_tokens(b"SELECT COUNT/**/(1)", &ignored).token_bytes(),
        canonical_tokens(b"SELECT `COUNT`(1)", &ignored).token_bytes()
    );
    let mut qualified = digest_wire(&[748]);
    digest_identifier(&mut qualified, b"SELECT");
    qualified.extend(digest_wire(&[46]));
    digest_identifier(&mut qualified, b"a");
    assert_eq!(
        canonical_tokens(b"SELECT SELECT.a", &facts).token_bytes(),
        qualified
    );
}

fn digest_identifier(wire: &mut Vec<u8>, bytes: &[u8]) {
    wire.extend(digest_wire(&[1106]));
    wire.extend((bytes.len() as u16).to_le_bytes());
    wire.extend(bytes);
}

#[test]
fn canonical_original_at_tokens_and_bundled_words_use_native_variable_roles_once() {
    // sql_lex.cc2034–2093 + sql_digest.cc LEX_HOSTNAME reduction. This tests
    // the actual cursor, including the shared @name Word packing, not AST text.
    for (sql, tokens) in [
        (b"SELECT @a".as_slice(), vec![748, 64, 1100]),
        (b"SELECT @", vec![748, 64, 1100]),
        (b"SELECT @ 1", vec![748, 64, 1100, 1100]),
        (b"SELECT @a.b", vec![748, 64, 1100]),
        (b"SELECT @a..b", vec![748, 64, 1100]),
        (b"SELECT @123", vec![748, 64, 1100]),
        (b"SELECT @'a.b'", vec![748, 64, 1100]),
        (b"SELECT @''", vec![748, 64, 1100]),
        (b"SELECT @NULL", vec![748, 64, 1100]),
        (b"SET @a:=1", vec![756, 64, 1100, 757, 1100]),
    ] {
        let facts = canonical_facts(1024);
        let actual = canonical_tokens(sql, &facts);
        assert_eq!(actual.token_bytes(), digest_wire(&tokens), "{:?}", sql);
        assert_eq!(
            actual.unavailable(),
            Some(MySqlDigestUnavailable::HashService)
        );
        assert_eq!(facts.hash_calls.get(), 1);
    }
    for sql in [b"SELECT @`a``b`".as_slice(), b"SELECT @@`a``b`"] {
        let facts = canonical_facts(1024);
        let mut expected = digest_wire(&[748, 64]);
        if sql.starts_with(b"SELECT @@") {
            expected.extend(digest_wire(&[64]));
        }
        digest_identifier(&mut expected, b"a`b");
        assert_eq!(canonical_tokens(sql, &facts).token_bytes(), expected);
        assert_eq!(
            facts.hash_calls.get(),
            1,
            "quoted token follows its original prefix once"
        );
    }
    for sql in [b"SELECT @@".as_slice(), b"SELECT @@ `a``b`"] {
        let facts = canonical_facts(1024);
        let result = canonical_tokens(sql, &facts);
        assert_eq!(
            result.unavailable(),
            Some(MySqlDigestUnavailable::VariableRole)
        );
        assert_eq!(
            facts.hash_calls.get(),
            0,
            "missing/foreign-frontier identifier cannot mint a complete digest"
        );
    }
    let facts = canonical_facts(1024);
    let mut expected = digest_wire(&[748, 64, 64]);
    digest_identifier(&mut expected, b"warning_count");
    assert_eq!(
        canonical_tokens(b"SELECT @@warning_count", &facts).token_bytes(),
        expected
    );
    let mut scoped = digest_wire(&[748, 64, 64]);
    scoped.extend(digest_wire(&[
        mysql_digest_symbol(b"SESSION").unwrap().0,
        46,
    ]));
    digest_identifier(&mut scoped, b"warning_count");
    assert_eq!(
        canonical_tokens(b"SELECT @@SESSION.warning_count", &facts).token_bytes(),
        scoped
    );
    let mut function = digest_wire(&[748, 64, 64]);
    digest_identifier(&mut function, b"COUNT");
    function.extend(digest_wire(&[1102]));
    assert_eq!(
        canonical_tokens(b"SELECT @@COUNT(1)", &facts).token_bytes(),
        function
    );
    // Charset introducer lookup is not an authority for @@ option names.
    let mut charset = digest_wire(&[748, 64, 64]);
    digest_identifier(&mut charset, b"_utf8mb4");
    assert_eq!(
        canonical_tokens(b"SELECT @@_utf8mb4", &facts).token_bytes(),
        charset
    );
}

#[test]
fn canonical_qualified_identifier_is_decided_by_original_physical_token_window() {
    for (sql, left_keyword, right_keyword) in [
        (b"SELECT SELECT.SELECT".as_slice(), false, false),
        (b"SELECT SELECT. SELECT", true, true),
        (b"SELECT SELECT./**/SELECT", true, true),
        (b"SELECT SELECT .SELECT", true, false),
        (b"SELECT SELECT . SELECT", true, true),
        (b"SELECT SELECT.`SELECT`", true, false),
    ] {
        let facts = canonical_facts(1024);
        let mut expected = digest_wire(&[748]);
        if left_keyword {
            expected.extend(digest_wire(&[748]));
        } else {
            digest_identifier(&mut expected, b"SELECT");
        }
        expected.extend(digest_wire(&[46]));
        if right_keyword {
            expected.extend(digest_wire(&[748]));
        } else {
            digest_identifier(&mut expected, b"SELECT");
        }
        let actual = canonical_tokens(sql, &facts);
        assert_eq!(actual.token_bytes(), expected, "{:?}", sql);
        assert_eq!(
            actual.unavailable(),
            Some(MySqlDigestUnavailable::HashService)
        );
    }
    let facts = canonical_facts(1024);
    assert_eq!(
        canonical_tokens(b"SELECT SELECT.1", &facts).unavailable(),
        Some(MySqlDigestUnavailable::GrammarLookahead)
    );
    assert_eq!(facts.hash_calls.get(), 0);
}

#[test]
fn canonical_hint_position_and_empty_hint_do_not_require_a_second_hint_parser() {
    for sql in [
        b"/*+ MAX_EXECUTION_TIME(1) */ SELECT 1".as_slice(),
        b"SELECT 1 /*+ MAX_EXECUTION_TIME(1) */",
        b"SELECT /**/ /*+ MAX_EXECUTION_TIME(1) */ 1",
        b"SELECT /*+ \t\r\n */ /*+ MAX_EXECUTION_TIME(1) */ 1",
        b"SELECT /*+ \x0b\x0c */ 1",
        b"SELECT /*!80000 */ /*+ MAX_EXECUTION_TIME(1) */ 1",
        b"SELECT /*!90000 ignored */ /*+ MAX_EXECUTION_TIME(1) */ 1",
    ] {
        let facts = canonical_facts(1024);
        let actual = canonical_tokens(sql, &facts);
        assert_eq!(actual.token_bytes(), digest_wire(&[748, 1100]), "{:?}", sql);
        assert_eq!(facts.hash_calls.get(), 1);
    }
    for keyword in ["SELECT", "INSERT", "UPDATE", "DELETE", "REPLACE"] {
        let sql = format!("{keyword} /*+ MAX_EXECUTION_TIME(1) */ 1");
        let facts = canonical_facts(1024);
        let actual = canonical_tokens(sql.as_bytes(), &facts);
        assert_eq!(actual.unavailable(), Some(MySqlDigestUnavailable::Hint));
        assert_eq!(facts.hash_calls.get(), 0);
    }
    // No parser recovery or identifier conversion is guessed for other bodies.
    for body in ["unknown", "@qb", "`a`", "'a'", "MAX_EXECUTION_TIME("] {
        let facts = canonical_facts(1024);
        let sql = format!("SELECT /*+ {body} */ 1");
        assert_eq!(
            canonical_tokens(sql.as_bytes(), &facts).unavailable(),
            Some(MySqlDigestUnavailable::Hint)
        );
        assert_eq!(facts.hash_calls.get(), 0);
    }
}

#[test]
fn canonical_packed_at_word_is_not_an_ident_sep_unit_or_a_function_keyword() {
    let facts = canonical_facts(1024);
    assert_eq!(
        canonical_tokens(b"SELECT SELECT.@a", &facts).token_bytes(),
        digest_wire(&[748, 748, 46, 64, 1100])
    );
    let facts = canonical_facts(1024);
    let mut expected = digest_wire(&[748, 64, 64]);
    digest_identifier(&mut expected, b"COUNT");
    expected.extend(digest_wire(&[46]));
    digest_identifier(&mut expected, b"a");
    assert_eq!(
        canonical_tokens(b"SELECT @@COUNT.a", &facts).token_bytes(),
        expected
    );
}

#[test]
fn canonical_version_comment_barrier_keeps_original_function_lookahead() {
    let mut facts = canonical_facts(1024);
    facts.config.ignore_space = true;
    let expected = canonical_tokens(b"SELECT `COUNT`(1)", &facts)
        .token_bytes()
        .to_vec();
    for sql in [
        b"SELECT COUNT/*!80000 */(1)".as_slice(),
        b"SELECT COUNT/*!90000 ignored */(1)",
    ] {
        assert_eq!(canonical_tokens(sql, &facts).token_bytes(), expected);
    }
}

#[test]
fn canonical_original_ascii_character_tokens_are_native_not_foreign_extensions() {
    let facts = canonical_facts(1024);
    let actual = canonical_tokens(b"SELECT [a]", &facts);
    let mut expected = digest_wire(&[748, 91]);
    digest_identifier(&mut expected, b"a");
    expected.extend(digest_wire(&[93]));
    assert_eq!(actual.token_bytes(), expected);
    assert_eq!(
        actual.unavailable(),
        Some(MySqlDigestUnavailable::HashService)
    );
    let facts = canonical_facts(1024);
    assert_eq!(
        canonical_tokens(b"SELECT ^@a", &facts).unavailable(),
        Some(MySqlDigestUnavailable::Token)
    );
    assert_eq!(facts.hash_calls.get(), 0);
}

#[test]
fn canonical_role_capacity_is_atomic_and_original_token_demand_still_finishes() {
    for capacity in 1..=8 {
        let facts = canonical_facts(capacity);
        let actual = canonical_tokens(b"SELECT @a.b", &facts);
        let expected = digest_wire(&[748, 64, 1100]);
        assert_eq!(actual.token_bytes(), &expected[..capacity.min(6) & !1]);
        assert_eq!(actual.is_full(), capacity < 6);
        assert_eq!(facts.hash_calls.get(), 1);
    }
    for capacity in [6, 7, 8, 9] {
        let facts = canonical_facts(capacity);
        let actual = canonical_tokens(b"SELECT @@a", &facts);
        assert_eq!(actual.is_full(), capacity < 11);
        assert_eq!(actual.token_bytes(), digest_wire(&[748, 64, 64]));
    }
    let facts = canonical_facts(11);
    let actual = canonical_tokens(b"SELECT @@a", &facts);
    let mut expected = digest_wire(&[748, 64, 64]);
    digest_identifier(&mut expected, b"a");
    assert_eq!(actual.token_bytes(), expected);
    assert!(!actual.is_full());
    for (sql, capacity, expected) in [
        (b"SELECT @@".as_slice(), 2, vec![748]),
        (b"SELECT @@", 4, vec![748, 64]),
    ] {
        let facts = canonical_facts(capacity);
        let actual = canonical_tokens(sql, &facts);
        assert!(actual.is_full());
        assert_eq!(actual.token_bytes(), digest_wire(&expected));
        assert_eq!(
            actual.unavailable(),
            Some(MySqlDigestUnavailable::HashService)
        );
        assert_eq!(facts.hash_calls.get(), 1);
    }
}

#[test]
fn canonical_role_lookahead_does_not_probe_more_native_units_than_original_cursor() {
    let dialect = crate::dialect::MySqlDialect {};
    for bytes in [
        b"SELECT SELECT.a".as_slice(),
        b"SELECT SELECT. SELECT",
        b"SELECT @@SESSION.warning_count",
        b"SELECT @a.b",
        b"SELECT /*+ \x0b\x0c */ 1",
    ] {
        let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
        let enabled = canonical_facts(1024);
        let disabled = canonical_facts(0);
        for facts in [&enabled, &disabled] {
            let mut cursor = MySqlTokenCursor::new(
                &dialect,
                owner.occurrence(0..bytes.len()).unwrap(),
                facts,
                None,
            )
            .unwrap();
            cursor.begin_canonical_digest().unwrap();
            while cursor.next_token().unwrap().is_some() {}
            assert_eq!(cursor.consumed(), bytes.len());
            let receipt = cursor.finish_canonical_digest().unwrap();
            assert_eq!(receipt.is_some(), facts.config.capacity != 0);
        }
        assert_eq!(
            enabled.inner.lexical_calls.get(),
            disabled.inner.lexical_calls.get(),
            "{:?}",
            bytes
        );
        assert_eq!(
            enabled.inner.conversion_calls.get(),
            disabled.inner.conversion_calls.get(),
            "{:?}",
            bytes
        );
    }
}

#[test]
fn canonical_pending_native_role_preserves_first_cancel_and_deadline_not_a_hash() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let dialect = crate::dialect::MySqlDialect {};
    for why in [
        MySqlSourceErrorKind::Cancelled,
        MySqlSourceErrorKind::DeadlineExceeded,
    ] {
        let bytes = b"SELECT SELECT.a";
        let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
        let facts = canonical_facts(1024);
        let cancel = Arc::new(AtomicBool::new(false));
        let probe = cancel.clone();
        let mut cursor = MySqlTokenCursor::new(
            &dialect,
            owner.occurrence(0..bytes.len()).unwrap(),
            &facts,
            Some(Arc::new(move || probe.load(Ordering::Acquire))),
        )
        .unwrap();
        cursor.begin_canonical_digest().unwrap();
        loop {
            let token = cursor.next_token().unwrap().unwrap();
            if matches!(token.token.token, Token::Period) {
                break;
            }
        }
        assert!(cursor.digest.as_ref().unwrap().pending_period.is_some());
        if why == MySqlSourceErrorKind::Cancelled {
            cancel.store(true, Ordering::Release);
        } else {
            facts.inner.fail_lexical.set(Some(why));
        }
        let first = cursor.next_token().err().unwrap();
        assert_eq!(first.source_kind, why);
        let calls = facts.inner.lexical_calls.get();
        assert_eq!(cursor.next_token().err().unwrap(), first);
        assert_eq!(cursor.finish_canonical_digest().err().unwrap(), first);
        assert_eq!(facts.inner.lexical_calls.get(), calls);
        assert_eq!(facts.hash_calls.get(), 0);
    }
}

#[test]
fn canonical_identifier_overflow_is_unavailable_and_never_a_wrapped_length() {
    let facts = canonical_facts(1 << 17);
    let mut bytes = b"SELECT ".to_vec();
    bytes.resize(bytes.len() + u16::MAX as usize + 1, b'a');
    let actual = canonical_tokens(&bytes, &facts);
    assert_eq!(
        actual.unavailable(),
        Some(MySqlDigestUnavailable::Identifier)
    );
    assert_eq!(actual.token_bytes(), digest_wire(&[748]));
    assert!(!actual.is_full());
    assert_eq!(facts.hash_calls.get(), 0);
}

#[test]
fn canonical_null_original_close_is_appended_once_after_real_commit() {
    // Fixed generated state 819 default-reduces NULL before demanding ')'.
    // Exercise both original demand timings without running another lexer.
    let dialect = crate::dialect::MySqlDialect {};
    for after_close in [false, true] {
        let bytes = b"SELECT (NULL)";
        let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
        let facts = canonical_facts(1024);
        let mut cursor = MySqlTokenCursor::new(
            &dialect,
            owner.occurrence(0..bytes.len()).unwrap(),
            &facts,
            None,
        )
        .unwrap();
        cursor.begin_canonical_digest().unwrap();
        let mut literal = None;
        while let Some(token) = cursor.next_token().unwrap() {
            if matches!(token.token.token, Token::Word(ref word) if word.keyword == Keyword::NULL) {
                literal = Some(token.source.range());
                if !after_close {
                    cursor.reduce_canonical_null(literal.clone().unwrap());
                }
            }
            if after_close && matches!(token.token.token, Token::RParen) {
                assert_eq!(token.source.bytes().unwrap(), b")");
                let pending = cursor.digest.as_ref().unwrap();
                assert!(pending.pending_word.as_ref().unwrap().closing_parenthesis);
                assert_eq!(pending.tokens, digest_wire(&[748, 40]));
                let admissions = facts.inner.admission_calls.get();
                let probes = facts.inner.lexical_calls.get();
                cursor.reduce_canonical_null(literal.take().unwrap());
                assert_eq!(facts.inner.admission_calls.get(), admissions);
                assert_eq!(facts.inner.lexical_calls.get(), probes);
                assert_eq!(
                    cursor.digest.as_ref().unwrap().tokens,
                    digest_wire(&[748, 1102])
                );
            }
        }
        let MySqlCanonicalDigestReceipt::Collected(receipt) =
            cursor.finish_canonical_digest().unwrap().unwrap()
        else {
            panic!("same admitted Source");
        };
        assert_eq!(receipt.token_bytes(), digest_wire(&[748, 1102]));
        assert_eq!(
            receipt.unavailable(),
            Some(MySqlDigestUnavailable::HashService)
        );
        assert_eq!(facts.hash_calls.get(), 1);
    }
}

#[test]
fn canonical_null_store_restores_only_the_genuine_already_appended_following_token() {
    let dialect = crate::dialect::MySqlDialect {};
    let bytes = b"SELECT NULL,1";
    let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let facts = canonical_facts(1024);
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..bytes.len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    cursor.begin_canonical_digest().unwrap();
    let mut literal = None;
    while let Some(token) = cursor.next_token().unwrap() {
        if matches!(token.token.token, Token::Word(ref word) if word.keyword == Keyword::NULL) {
            literal = Some(token.source.range());
        }
        if matches!(token.token.token, Token::Comma) {
            assert_eq!(
                cursor.digest.as_ref().unwrap().tokens,
                digest_wire(&[748, 627, 44])
            );
            cursor.reduce_canonical_null(literal.take().unwrap());
            assert_eq!(
                cursor.digest.as_ref().unwrap().tokens,
                digest_wire(&[748, 1100, 44])
            );
        }
    }
    let MySqlCanonicalDigestReceipt::Collected(receipt) =
        cursor.finish_canonical_digest().unwrap().unwrap()
    else {
        panic!("actual admitted token prefix");
    };
    assert_eq!(receipt.token_bytes(), digest_wire(&[748, 1101]));
    assert_eq!(facts.hash_calls.get(), 1);
    assert_eq!(
        canonical_tokens(b"SELECT (1 IS NULL)", &facts).token_bytes(),
        digest_wire(&[748, 40, 1100, 509, 627, 41]),
        "uncommitted NULL remains a native keyword, not a row literal"
    );
}

#[test]
fn canonical_null_pending_close_preserves_unknown_rollback_and_first_controls() {
    let dialect = crate::dialect::MySqlDialect {};
    for control in [
        None,
        Some(MySqlSourceErrorKind::Cancelled),
        Some(MySqlSourceErrorKind::DeadlineExceeded),
    ] {
        let bytes = b"SELECT (NULL)+1";
        let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
        let facts = canonical_facts(1024);
        let mut cursor = MySqlTokenCursor::new(
            &dialect,
            owner.occurrence(0..bytes.len()).unwrap(),
            &facts,
            None,
        )
        .unwrap();
        cursor.begin_canonical_digest().unwrap();
        let mut literal = None;
        loop {
            let token = cursor.next_token().unwrap().unwrap();
            if matches!(token.token.token, Token::Word(ref word) if word.keyword == Keyword::NULL) {
                literal = Some(token.source.range());
            }
            if matches!(token.token.token, Token::RParen) {
                break;
            }
        }
        if let Some(control) = control {
            facts.inner.fail_lexical.set(Some(control));
            // An already-admitted cached unit does not become a new adapter
            // call. Observe the first real failing demand, never rescan it.
            let first = loop {
                match cursor.next_token() {
                    Err(error) => break error,
                    Ok(Some(_)) => {}
                    Ok(None) => panic!("the remaining original units require the adapter"),
                }
            };
            assert_eq!(first.source_kind, control);
            let probes = facts.inner.lexical_calls.get();
            assert_eq!(cursor.next_token().err().unwrap(), first);
            assert_eq!(cursor.finish_canonical_digest().err().unwrap(), first);
            assert_eq!(facts.inner.lexical_calls.get(), probes);
        } else {
            assert!(matches!(
                cursor.next_token().unwrap().unwrap().token.token,
                Token::Plus
            ));
            cursor.reduce_canonical_null(literal.unwrap());
            let MySqlCanonicalDigestReceipt::Collected(receipt) =
                cursor.finish_canonical_digest().unwrap().unwrap()
            else {
                panic!("original observation admission");
            };
            assert_eq!(
                receipt.unavailable(),
                Some(MySqlDigestUnavailable::GrammarLookahead)
            );
            assert!(receipt.hash().is_none());
        }
        assert_eq!(facts.hash_calls.get(), 0);
    }
}

#[test]
fn canonical_null_held_close_rollback_and_actual_cancellation_never_publish_a_hash() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let dialect = crate::dialect::MySqlDialect {};
    for cancelled in [false, true] {
        let bytes = b"SELECT (NULL)";
        let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
        let facts = canonical_facts(1024);
        let cancel = Arc::new(AtomicBool::new(false));
        let probe = Arc::clone(&cancel);
        let mut cursor = MySqlTokenCursor::new(
            &dialect,
            owner.occurrence(0..bytes.len()).unwrap(),
            &facts,
            Some(Arc::new(move || probe.load(Ordering::Acquire))),
        )
        .unwrap();
        cursor.begin_canonical_digest().unwrap();
        let mut literal = None;
        loop {
            let token = cursor.next_token().unwrap().unwrap();
            if matches!(token.token.token, Token::Word(ref word) if word.keyword == Keyword::NULL) {
                literal = Some(token.source.range());
            }
            if matches!(token.token.token, Token::RParen) {
                break;
            }
        }
        if cancelled {
            cancel.store(true, Ordering::Release);
            let probes = facts.inner.lexical_calls.get();
            let first = cursor.next_token().err().unwrap();
            assert_eq!(first.source_kind, MySqlSourceErrorKind::Cancelled);
            assert_eq!(cursor.finish_canonical_digest().err().unwrap(), first);
            assert_eq!(facts.inner.lexical_calls.get(), probes);
        } else {
            let literal = literal.unwrap();
            cursor.reduce_canonical_null(literal.clone());
            assert_eq!(
                cursor.digest.as_ref().unwrap().tokens,
                digest_wire(&[748, 1102])
            );
            cursor.rewind_canonical_digest(literal.start);
            let MySqlCanonicalDigestReceipt::Collected(receipt) =
                cursor.finish_canonical_digest().unwrap().unwrap()
            else {
                panic!("same original admission");
            };
            assert_eq!(
                receipt.unavailable(),
                Some(MySqlDigestUnavailable::GrammarLookahead)
            );
            assert!(receipt.hash().is_none());
        }
        assert_eq!(facts.hash_calls.get(), 0);
    }
}

#[test]
fn canonical_null_speculation_rollback_refuses_hash_without_a_second_ledger() {
    let dialect = crate::dialect::MySqlDialect {};
    let bytes = b"SELECT NULL";
    let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let facts = canonical_facts(1024);
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..bytes.len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    cursor.begin_canonical_digest().unwrap();
    cursor.next_token().unwrap();
    cursor.next_token().unwrap();
    let null = cursor.next_token().unwrap().unwrap();
    cursor.reduce_canonical_null(null.source.range());
    assert_eq!(cursor.canonical_reduction_boundary(), Some(7));
    cursor.rewind_canonical_digest(7);
    let MySqlCanonicalDigestReceipt::Collected(receipt) =
        cursor.finish_canonical_digest().unwrap().unwrap()
    else {
        panic!("same admission");
    };
    assert_eq!(
        receipt.unavailable(),
        Some(MySqlDigestUnavailable::GrammarLookahead)
    );
    assert_eq!(facts.hash_calls.get(), 0);
}

#[test]
fn canonical_native_full_stops_later_unknown_productions_without_business_truncation() {
    let facts = canonical_facts(2);
    let receipt = canonical_tokens(b"SELECT 1, @a /*+ unknown hint */", &facts);
    assert!(receipt.is_full());
    assert_eq!(receipt.token_bytes(), 748u16.to_le_bytes());
    assert_eq!(
        receipt.unavailable(),
        Some(MySqlDigestUnavailable::HashService)
    );
    assert_eq!(
        facts.hash_calls.get(),
        1,
        "native full prefix still has one token hash"
    );
    let facts = canonical_facts(1024);
    for sql in [
        b"SELECT X'FF',B'01',0xFF,0b01".as_slice(),
        b"SELECT 'a','b'",
    ] {
        assert_eq!(
            canonical_tokens(sql, &facts).token_bytes(),
            digest_wire(&[748, 1101])
        );
    }
    let receipt = canonical_tokens(b"SELECT TRUE,FALSE", &facts);
    assert_eq!(
        receipt.token_bytes(),
        digest_wire(&[748, 844, 44, 434]),
        "native booleans are keywords, not normalized numeric constants"
    );
}

struct ConnectionTextFacts {
    units: CommentFacts,
    calls: Cell<usize>,
    failure: Cell<Option<MySqlSourceErrorKind>>,
    live_bytes: Arc<std::sync::atomic::AtomicUsize>,
}

struct TextStringLease {
    _admission: SqlSourceAdmission,
    live_bytes: Arc<std::sync::atomic::AtomicUsize>,
    bytes: usize,
}

impl Drop for TextStringLease {
    fn drop(&mut self) {
        self.live_bytes
            .fetch_sub(self.bytes, std::sync::atomic::Ordering::AcqRel);
    }
}

impl MySqlSourceAdapter for ConnectionTextFacts {
    fn client_charset(&self) -> &str {
        self.units.client_charset()
    }
    fn client_min_width(&self) -> u8 {
        self.units.client_min_width()
    }
    fn lexical_unit(&self, bytes: &[u8]) -> Result<MySqlLexicalUnit, MySqlSourceError> {
        self.units.lexical_unit(bytes)
    }
    fn system_identifier(
        &self,
        bytes: &[u8],
        quoted: bool,
    ) -> Result<(String, SqlSourceAdmission), MySqlSourceError> {
        self.units.system_identifier(bytes, quoted)
    }
    fn system_text(
        &self,
        text: &MySqlText,
    ) -> Result<(String, SqlSourceAdmission), MySqlSourceError> {
        self.units.system_text(text)
    }
    fn text_string(
        &self,
        text: &MySqlText,
    ) -> Result<(Vec<u8>, SqlSourceAdmission), MySqlSourceError> {
        self.calls.set(self.calls.get() + 1);
        if let Some(kind) = self.failure.get() {
            return Err(MySqlSourceError {
                kind,
                message: "original connection-byte conversion failure".into(),
            });
        }
        let bytes = text.bytes()?;
        let length = bytes
            .len()
            .checked_add(1)
            .ok_or("fixture conversion bound overflow")?;
        let actual_admission = self.units.admit(length)?;
        self.live_bytes
            .fetch_add(length, std::sync::atomic::Ordering::AcqRel);
        let lease = SqlSourceAdmission::new(Arc::new(TextStringLease {
            _admission: actual_admission,
            live_bytes: self.live_bytes.clone(),
            bytes: length,
        }));
        // This fixture tests forwarding opaque non-UTF8 conversion bytes and
        // admission lifetime, not a native charset conversion implementation.
        let mut converted = Vec::with_capacity(length);
        converted.extend_from_slice(bytes);
        converted.push(0xff);
        Ok((converted, lease))
    }
    fn admit(&self, bytes: usize) -> Result<SqlSourceAdmission, MySqlSourceError> {
        self.units.admit(bytes)
    }
}

fn connection_text_facts() -> ConnectionTextFacts {
    ConnectionTextFacts {
        units: first_error_facts(),
        calls: Cell::new(0),
        failure: Cell::new(None),
        live_bytes: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    }
}

#[test]
fn text_string_borrows_actual_consumed_source_and_retains_conversion_until_bundle_drop() {
    use std::sync::atomic::Ordering;
    let dialect = crate::dialect::MySqlDialect {};
    let facts = connection_text_facts();
    for source in [b"''".as_slice(), b"'a\\0z'".as_slice()] {
        let owner = SqlSourceOwner::adopt(source.to_vec(), SqlSourceAdmission::new(Arc::new(())));
        let mut cursor = MySqlTokenCursor::new(
            &dialect,
            owner.occurrence(0..source.len()).unwrap(),
            &facts,
            None,
        )
        .unwrap();
        let Token::MySqlText(text) = cursor.next_token().unwrap().unwrap().token.token else {
            panic!("actual text token");
        };
        let before = cursor.consumed();
        let calls = facts.calls.get();
        let converted = cursor.text_string(&text).unwrap();
        let mut expected = text.bytes().unwrap().to_vec();
        expected.push(0xff);
        assert_eq!(converted, expected);
        assert!(String::from_utf8(converted.clone()).is_err());
        assert_eq!(
            cursor.consumed(),
            before,
            "conversion does not tokenize a suffix"
        );
        assert_eq!(facts.calls.get(), calls + 1);
        assert_eq!(
            facts.units.conversion_calls.get(),
            0,
            "no system charset fallback"
        );
        assert_eq!(facts.live_bytes.load(Ordering::Acquire), converted.len());
        assert!(cursor.next_token().unwrap().is_none());
        let bundle = cursor.into_cpp();
        drop(converted);
        assert_eq!(facts.live_bytes.load(Ordering::Acquire), expected.len());
        drop(bundle);
        assert_eq!(facts.live_bytes.load(Ordering::Acquire), 0);
        assert_eq!(owner.bytes(), source);
    }
}

#[test]
fn text_string_missing_capability_and_operational_failure_keep_original_first_error() {
    let dialect = crate::dialect::MySqlDialect {};
    let owner = SqlSourceOwner::adopt(b"'a' 1".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let absent = first_error_facts();
    let mut cursor =
        MySqlTokenCursor::new(&dialect, owner.occurrence(0..5).unwrap(), &absent, None).unwrap();
    let Token::MySqlText(text) = cursor.next_token().unwrap().unwrap().token.token else {
        panic!("actual text token");
    };
    let first = cursor.text_string(&text).err().unwrap();
    assert_eq!(
        first.message,
        "TEXT_STRING requires the actual connection-byte source adapter"
    );
    assert_eq!(absent.conversion_calls.get(), 0);
    assert_eq!(cursor.text_string(&text).err().unwrap(), first);
    assert_same_first_error(&mut cursor, &absent, &first);

    for kind in [
        MySqlSourceErrorKind::ResourceExhausted,
        MySqlSourceErrorKind::Cancelled,
        MySqlSourceErrorKind::DeadlineExceeded,
    ] {
        let facts = connection_text_facts();
        let mut cursor =
            MySqlTokenCursor::new(&dialect, owner.occurrence(0..5).unwrap(), &facts, None).unwrap();
        let Token::MySqlText(text) = cursor.next_token().unwrap().unwrap().token.token else {
            panic!("actual text token");
        };
        facts.failure.set(Some(kind));
        let first = cursor.text_string(&text).err().unwrap();
        assert_eq!(first.source_kind, kind);
        assert_eq!(first.message, "original connection-byte conversion failure");
        facts.failure.set(None);
        assert_eq!(cursor.text_string(&text).err().unwrap(), first);
        assert_same_first_error(&mut cursor, &facts.units, &first);
        assert_eq!(facts.calls.get(), 1);
        assert_eq!(
            facts.live_bytes.load(std::sync::atomic::Ordering::Acquire),
            0
        );
    }
}

#[test]
fn text_string_rejects_foreign_future_and_introduced_source_before_conversion() {
    let dialect = crate::dialect::MySqlDialect {};
    let owner = SqlSourceOwner::adopt(b"'a' 'b'".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let other = SqlSourceOwner::adopt(b"'a' 'b'".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    for source in [
        other.occurrence(0..3).unwrap(),
        owner.occurrence(4..7).unwrap(),
    ] {
        let facts = connection_text_facts();
        let mut cursor =
            MySqlTokenCursor::new(&dialect, owner.occurrence(0..7).unwrap(), &facts, None).unwrap();
        assert!(cursor.next_token().unwrap().is_some());
        let text = MySqlText::from_occurrence(
            source.slice(1..2).unwrap(),
            source,
            "utf8mb4".into(),
            '\'',
            None,
        )
        .unwrap();
        let first = cursor.text_string(&text).err().unwrap();
        assert_eq!(
            first.message,
            "TEXT_STRING is not the current consumed literal Source"
        );
        assert_eq!(facts.calls.get(), 0);
        assert_same_first_error(&mut cursor, &facts.units, &first);
    }
    let facts = connection_text_facts();
    let mut cursor =
        MySqlTokenCursor::new(&dialect, owner.occurrence(0..7).unwrap(), &facts, None).unwrap();
    assert!(cursor.next_token().unwrap().is_some());
    let source = owner.occurrence(0..3).unwrap();
    let text = MySqlText::from_occurrence(
        source.slice(1..2).unwrap(),
        source,
        "utf8mb4".into(),
        '\'',
        Some("utf8mb4".into()),
    )
    .unwrap();
    assert!(cursor.text_string(&text).is_err());
    assert_eq!(
        facts.calls.get(),
        0,
        "introducer is a different grammar production"
    );
}

#[test]
fn text_string_conversion_admission_fails_before_output_allocation() {
    let dialect = crate::dialect::MySqlDialect {};
    let facts = connection_text_facts();
    let owner = SqlSourceOwner::adopt(b"'a'".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let mut cursor =
        MySqlTokenCursor::new(&dialect, owner.occurrence(0..3).unwrap(), &facts, None).unwrap();
    let Token::MySqlText(text) = cursor.next_token().unwrap().unwrap().token.token else {
        panic!("actual token");
    };
    facts.units.refuse_allocations.set(true);
    let first = cursor.text_string(&text).err().unwrap();
    assert_eq!(first.source_kind, MySqlSourceErrorKind::ResourceExhausted);
    assert_eq!(first.message, "actual fixture admission refusal");
    assert_eq!(
        facts.live_bytes.load(std::sync::atomic::Ordering::Acquire),
        0
    );
    facts.units.refuse_allocations.set(false);
    assert_eq!(cursor.text_string(&text).err().unwrap(), first);
    assert_eq!(facts.calls.get(), 1);
    assert_same_first_error(&mut cursor, &facts.units, &first);
}

struct CommentFacts {
    refuse_allocations: Cell<bool>,
    lexical_calls: Cell<usize>,
    admission_calls: Cell<usize>,
    conversion_calls: Cell<usize>,
    fail_lexical: Cell<Option<MySqlSourceErrorKind>>,
    multibyte_slash: bool,
}

struct CharacterGrammarFacts {
    units: CommentFacts,
    conditions: core::cell::RefCell<Vec<String>>,
}

impl MySqlSourceAdapter for CharacterGrammarFacts {
    fn client_charset(&self) -> &str {
        self.units.client_charset()
    }
    fn client_min_width(&self) -> u8 {
        self.units.client_min_width()
    }
    fn lexical_unit(&self, bytes: &[u8]) -> Result<MySqlLexicalUnit, MySqlSourceError> {
        self.units.lexical_unit(bytes)
    }
    fn system_identifier(
        &self,
        bytes: &[u8],
        quoted: bool,
    ) -> Result<(String, SqlSourceAdmission), MySqlSourceError> {
        self.units.system_identifier(bytes, quoted)
    }
    fn system_text(
        &self,
        value: &MySqlText,
    ) -> Result<(String, SqlSourceAdmission), MySqlSourceError> {
        self.units.system_text(value)
    }
    fn admit(&self, bytes: usize) -> Result<SqlSourceAdmission, MySqlSourceError> {
        self.units.admit(bytes)
    }
    fn deprecated_character_set(&self, charset: &str) {
        self.conditions.borrow_mut().push(charset.into());
    }
}

#[test]
fn charset_token_conditions_follow_demand_and_identifier_role_not_future_tail() {
    let dialect = crate::dialect::MySqlDialect {};
    let facts = CharacterGrammarFacts {
        units: CommentFacts {
            refuse_allocations: Cell::new(false),
            lexical_calls: Cell::new(0),
            admission_calls: Cell::new(0),
            conversion_calls: Cell::new(0),
            fail_lexical: Cell::new(None),
            multibyte_slash: false,
        },
        conditions: core::cell::RefCell::new(Vec::new()),
    };
    // The real registry remains the adapter's responsibility; this test
    // observes only calls made by the one demanded token cursor.
    let bytes = b"_utf8 'a' 'b' t. _utf8 `_utf8` '_utf8' /* _utf8 */ _ucs2";
    let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..bytes.len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    assert!(
        facts.conditions.borrow().is_empty(),
        "constructing a cursor cannot scan its tail"
    );
    let first = cursor.next_token().unwrap().unwrap();
    assert!(matches!(&first.token.token, Token::Word(word) if word.value == "_utf8"));
    assert_eq!(&*facts.conditions.borrow(), &["utf8"]);
    while cursor.next_token().unwrap().is_some() {}
    assert_eq!(
        &*facts.conditions.borrow(),
        &["utf8", "ucs2"],
        "qualified/quoted/string/comment occurrences cannot emit charset conditions"
    );
    assert!(cursor.next_token().unwrap().is_none());
    assert_eq!(
        facts.conditions.borrow().len(),
        2,
        "EOF does not replay tokens"
    );
    drop(cursor);
    facts.conditions.borrow_mut().clear();
    facts.units.refuse_allocations.set(true);
    let mut refused = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..bytes.len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    assert_eq!(
        refused
            .next_token()
            .err()
            .expect("original admission error")
            .source_kind,
        MySqlSourceErrorKind::ResourceExhausted
    );
    assert!(
        facts.conditions.borrow().is_empty(),
        "failed identifier admission precedes lexical condition issuance"
    );
}
impl MySqlSourceAdapter for CommentFacts {
    fn client_charset(&self) -> &str {
        "utf8mb4"
    }
    fn client_min_width(&self) -> u8 {
        1
    }
    fn lexical_unit(&self, bytes: &[u8]) -> Result<MySqlLexicalUnit, MySqlSourceError> {
        self.lexical_calls.set(self.lexical_calls.get() + 1);
        if let Some(kind) = self.fail_lexical.get() {
            return Err(MySqlSourceError {
                kind,
                message: "original demanded unit failure".into(),
            });
        }
        let first = *bytes.first().ok_or("fixture has no byte")?;
        let width = if self.multibyte_slash && bytes.starts_with(&[0x81, b'/']) {
            2
        } else {
            1
        };
        Ok(MySqlLexicalUnit {
            width,
            expected_width: width,
            state: 0,
            valid: first.is_ascii() || width == 2,
            ascii: first.is_ascii() && width == 1,
            space: matches!(first, 9..=13 | 32),
            control: first < 32 || first == 127,
            identifier: first.is_ascii_alphanumeric() || first == b'_' || width == 2,
        })
    }
    fn system_identifier(
        &self,
        bytes: &[u8],
        _: bool,
    ) -> Result<(String, SqlSourceAdmission), MySqlSourceError> {
        self.conversion_calls.set(self.conversion_calls.get() + 1);
        Ok((
            String::from_utf8(bytes.to_vec()).map_err(|_| "fixture invalid identifier")?,
            self.admit(bytes.len())?,
        ))
    }
    fn system_text(&self, _: &MySqlText) -> Result<(String, SqlSourceAdmission), MySqlSourceError> {
        self.conversion_calls.set(self.conversion_calls.get() + 1);
        Err("this cursor matrix never demands a converted text getter".into())
    }
    fn admit(&self, _: usize) -> Result<SqlSourceAdmission, MySqlSourceError> {
        self.admission_calls.set(self.admission_calls.get() + 1);
        if self.refuse_allocations.get() {
            Err(MySqlSourceError {
                kind: MySqlSourceErrorKind::ResourceExhausted,
                message: "actual fixture admission refusal".into(),
            })
        } else {
            Ok(SqlSourceAdmission::new(Arc::new(())))
        }
    }
}

#[test]
fn comment_terminal_fact_is_issued_only_by_the_same_cursor_fsm() {
    let dialect = crate::dialect::MySqlDialect {};
    for (bytes, expected) in [
        (&b""[..], false),
        (&b" \t\r\n"[..], false),
        (&b"1"[..], false),
        (&b"'/* inside a literal */'"[..], false),
        (&b"`# inside an identifier`"[..], false),
        (&b"--not_a_comment"[..], false),
        (&b"# actual comment"[..], true),
        (&b"-- actual comment"[..], true),
        (&b"--\x01actual control comment"[..], true),
        (&b"/* actual comment */"[..], true),
        (&b"/*!99999 ignored */"[..], true),
        (&b"/*!80046 1 */"[..], true),
        (&b"/*! 1 */"[..], true),
    ] {
        let facts = CommentFacts {
            refuse_allocations: Cell::new(false),
            lexical_calls: Cell::new(0),
            admission_calls: Cell::new(0),
            conversion_calls: Cell::new(0),
            fail_lexical: Cell::new(None),
            multibyte_slash: false,
        };
        let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
        let mut cursor = MySqlTokenCursor::new(
            &dialect,
            owner.occurrence(0..bytes.len()).unwrap(),
            &facts,
            None,
        )
        .unwrap();
        assert!(
            !cursor.has_comment(),
            "unread source is not a terminal: {bytes:?}"
        );
        assert_eq!(facts.lexical_calls.get(), 0);
        while cursor.next_token().unwrap().is_some() {}
        assert_eq!(cursor.has_comment(), expected, "{bytes:?}");
        let calls = facts.lexical_calls.get();
        assert_eq!(cursor.has_comment(), expected);
        assert_eq!(facts.lexical_calls.get(), calls, "fact reads cannot prelex");
        assert_eq!(owner.bytes(), bytes);
    }
}

#[test]
fn comment_fact_is_local_to_the_actual_selected_occurrence_even_on_the_same_packet_owner() {
    let dialect = crate::dialect::MySqlDialect {};
    let facts = CommentFacts {
        refuse_allocations: Cell::new(false),
        lexical_calls: Cell::new(0),
        admission_calls: Cell::new(0),
        conversion_calls: Cell::new(0),
        fail_lexical: Cell::new(None),
        multibyte_slash: false,
    };
    let bytes = b"/* comment in first invocation */1; \t";
    let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let mut first = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..bytes.len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    loop {
        let token = first
            .next_token()
            .unwrap()
            .expect("the first real delimiter");
        if token.token.token == Token::SemiColon {
            break;
        }
    }
    assert!(first.has_comment());
    let delimiter_end = first.consumed();
    assert_eq!(&bytes[delimiter_end..], b" \t");
    let mut next = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(delimiter_end..bytes.len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    assert!(
        !next.has_comment(),
        "same packet does not inherit prior terminal evidence"
    );
    while next.next_token().unwrap().is_some() {}
    assert!(
        !next.has_comment(),
        "true whitespace EOF cannot borrow a prior comment"
    );
    assert!(
        first.has_comment(),
        "a sibling cursor cannot overwrite the original fact"
    );
    assert_eq!(owner.bytes(), bytes);
}

#[test]
fn recognized_comment_does_not_hide_unterminated_or_admission_failure() {
    let dialect = crate::dialect::MySqlDialect {};
    for bytes in [&b"/* unterminated"[..], &b"/*!80046 1"[..]] {
        let facts = CommentFacts {
            refuse_allocations: Cell::new(false),
            lexical_calls: Cell::new(0),
            admission_calls: Cell::new(0),
            conversion_calls: Cell::new(0),
            fail_lexical: Cell::new(None),
            multibyte_slash: false,
        };
        let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
        let mut cursor = MySqlTokenCursor::new(
            &dialect,
            owner.occurrence(0..bytes.len()).unwrap(),
            &facts,
            None,
        )
        .unwrap();
        let error = loop {
            match cursor.next_token() {
                Ok(Some(_)) => {}
                Ok(None) => panic!("unterminated comment must not become EOF"),
                Err(error) => break error,
            }
        };
        assert!(
            cursor.has_comment(),
            "recognized marker, not successful parse"
        );
        assert!(error.message.contains("unterminated"), "{error:?}");
        assert!(
            cursor.next_token().is_err(),
            "same error ledger, not empty OK"
        );
    }
    let facts = CommentFacts {
        refuse_allocations: Cell::new(true),
        lexical_calls: Cell::new(0),
        admission_calls: Cell::new(0),
        conversion_calls: Cell::new(0),
        fail_lexical: Cell::new(None),
        multibyte_slash: false,
    };
    let owner = SqlSourceOwner::adopt(
        b"/* admitted syntax */".to_vec(),
        SqlSourceAdmission::new(Arc::new(())),
    );
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..owner.bytes().len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    let error = cursor.next_token().err().expect("real CPP admission fails");
    assert_eq!(error.source_kind, MySqlSourceErrorKind::ResourceExhausted);
    assert!(cursor.has_comment());
}

#[test]
fn cancellation_and_multibyte_shadow_do_not_fabricate_comment_fact() {
    let dialect = crate::dialect::MySqlDialect {};
    let facts = CommentFacts {
        refuse_allocations: Cell::new(false),
        lexical_calls: Cell::new(0),
        admission_calls: Cell::new(0),
        conversion_calls: Cell::new(0),
        fail_lexical: Cell::new(None),
        multibyte_slash: false,
    };
    let owner = SqlSourceOwner::adopt(
        b"/* unread */".to_vec(),
        SqlSourceAdmission::new(Arc::new(())),
    );
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..owner.bytes().len()).unwrap(),
        &facts,
        Some(Arc::new(|| true)),
    )
    .unwrap();
    assert_eq!(
        cursor.next_token().err().unwrap().source_kind,
        MySqlSourceErrorKind::Cancelled
    );
    assert!(!cursor.has_comment());
    assert_eq!(facts.lexical_calls.get(), 0);

    let facts = CommentFacts {
        refuse_allocations: Cell::new(false),
        lexical_calls: Cell::new(0),
        admission_calls: Cell::new(0),
        conversion_calls: Cell::new(0),
        fail_lexical: Cell::new(None),
        multibyte_slash: true,
    };
    let owner = SqlSourceOwner::adopt(
        vec![0x81, b'/', b'*', b'x'],
        SqlSourceAdmission::new(Arc::new(())),
    );
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..owner.bytes().len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    // The supplied native unit consumes the slash as a trail byte. A later
    // fixture identifier conversion may reject it, but cannot issue a comment.
    while matches!(cursor.next_token(), Ok(Some(_))) {}
    assert!(!cursor.has_comment());
}

struct Adapter {
    warnings: Cell<usize>,
    lexical_calls: Cell<usize>,
}

#[test]
fn delimiter_does_not_preissue_a_comment_fact_from_the_unread_tail() {
    let dialect = crate::dialect::MySqlDialect {};
    let facts = CommentFacts {
        refuse_allocations: Cell::new(false),
        lexical_calls: Cell::new(0),
        admission_calls: Cell::new(0),
        conversion_calls: Cell::new(0),
        fail_lexical: Cell::new(None),
        multibyte_slash: false,
    };
    let owner = SqlSourceOwner::adopt(
        b"1;/* real but unread tail */".to_vec(),
        SqlSourceAdmission::new(Arc::new(())),
    );
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..owner.bytes().len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    assert!(matches!(
        cursor.next_token().unwrap().unwrap().token.token,
        Token::Number(_, _)
    ));
    assert_eq!(
        cursor.next_token().unwrap().unwrap().token.token,
        Token::SemiColon
    );
    assert_eq!(cursor.consumed(), 2);
    assert!(
        !cursor.has_comment(),
        "the next invocation has not lexed its comment"
    );
    let calls = facts.lexical_calls.get();
    assert!(!cursor.has_comment());
    assert_eq!(facts.lexical_calls.get(), calls);
    assert!(matches!(
        cursor.next_token().unwrap().unwrap().token.token,
        Token::Whitespace(Whitespace::MySqlComment(_))
    ));
    assert!(cursor.has_comment());
    assert!(cursor.next_token().unwrap().is_none());
    assert_eq!(owner.bytes(), b"1;/* real but unread tail */");
}
impl MySqlSourceAdapter for Adapter {
    fn client_charset(&self) -> &str {
        "utf8mb4"
    }
    fn client_min_width(&self) -> u8 {
        1
    }
    fn lexical_unit(&self, _: &[u8]) -> Result<MySqlLexicalUnit, MySqlSourceError> {
        self.lexical_calls.set(self.lexical_calls.get() + 1);
        Err("no lexical demand in this forwarding test".into())
    }
    fn system_identifier(
        &self,
        _: &[u8],
        _: bool,
    ) -> Result<(String, SqlSourceAdmission), MySqlSourceError> {
        Err("unused".into())
    }
    fn system_text(&self, _: &MySqlText) -> Result<(String, SqlSourceAdmission), MySqlSourceError> {
        Err("unused".into())
    }
    fn admit(&self, _: usize) -> Result<SqlSourceAdmission, MySqlSourceError> {
        Ok(SqlSourceAdmission::new(Arc::new(())))
    }
    fn json_table_on_error_before_empty(&self) {
        self.warnings.set(self.warnings.get() + 1);
    }
}
#[test]
fn grammar_callback_borrows_the_actual_cursor_adapter_without_prelexing() {
    let adapter = Adapter {
        warnings: Cell::new(0),
        lexical_calls: Cell::new(0),
    };
    let owner = SqlSourceOwner::adopt(
        b"unread original".to_vec(),
        SqlSourceAdmission::new(Arc::new(())),
    );
    let dialect = crate::dialect::MySqlDialect {};
    let cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..owner.bytes().len()).unwrap(),
        &adapter,
        None,
    )
    .unwrap();
    assert_eq!(adapter.warnings.get(), 0);
    cursor.json_table_on_error_before_empty();
    assert_eq!(adapter.warnings.get(), 1);
    assert_eq!(adapter.lexical_calls.get(), 0);
    assert_eq!(owner.bytes(), b"unread original");
    cursor.json_table_on_error_before_empty();
    assert_eq!(
        adapter.warnings.get(),
        2,
        "each caller reduction, no hidden queue/cache"
    );
}

fn first_error_facts() -> CommentFacts {
    CommentFacts {
        refuse_allocations: Cell::new(false),
        lexical_calls: Cell::new(0),
        admission_calls: Cell::new(0),
        conversion_calls: Cell::new(0),
        fail_lexical: Cell::new(None),
        multibyte_slash: false,
    }
}

fn first_error_progress(cursor: &MySqlTokenCursor<'_>, facts: &CommentFacts) -> [usize; 10] {
    let raw = cursor.state.raw.as_ref().unwrap();
    [
        cursor.consumed(),
        raw.work.get(),
        facts.lexical_calls.get(),
        facts.admission_calls.get(),
        facts.conversion_calls.get(),
        cursor.cpp.len(),
        cursor.cpp.capacity(),
        raw.allocations.len(),
        raw.allocations.capacity(),
        cursor.retained_admission_count(),
    ]
}

fn assert_same_first_error(
    cursor: &mut MySqlTokenCursor<'_>,
    facts: &CommentFacts,
    error: &TokenizerError,
) {
    let before = first_error_progress(cursor, facts);
    for _ in 0..3 {
        assert_eq!(cursor.next_token().err().expect("not EOF"), *error);
        assert_eq!(
            cursor.admit(16).err().expect("no post-error allocation"),
            *error
        );
        assert_eq!(
            cursor.admit_source(16).err().expect("no post-error source"),
            *error
        );
        assert_eq!(
            cursor
                .admit_retained(16)
                .err()
                .expect("no post-error retained lease"),
            *error
        );
        assert_eq!(
            first_error_progress(cursor, facts),
            before,
            "failed cursor cannot keep doing lexical/allocation work"
        );
    }
}

#[test]
fn all_unterminated_raw_forms_keep_original_kind_message_location_after_later_cancel() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let dialect = crate::dialect::MySqlDialect {};
    for bytes in [
        &b"/* unterminated"[..],
        &b"/*!80046 1"[..],
        &b"/*!99999 ignored"[..],
        &b"/*! unversioned"[..],
        &b"'unterminated"[..],
        &b"\"unterminated"[..],
        &b"`unterminated"[..],
        &b"x'0"[..],
        &b"\n'line two"[..],
    ] {
        let facts = first_error_facts();
        let cancelled = Arc::new(AtomicBool::new(false));
        let probes = Arc::new(AtomicUsize::new(0));
        let probe = {
            let cancelled = cancelled.clone();
            let probes = probes.clone();
            Arc::new(move || {
                probes.fetch_add(1, Ordering::Relaxed);
                cancelled.load(Ordering::Acquire)
            }) as CancellationCheck
        };
        let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
        let mut cursor = MySqlTokenCursor::new(
            &dialect,
            owner.occurrence(0..bytes.len()).unwrap(),
            &facts,
            Some(probe),
        )
        .unwrap();
        let first = loop {
            match cursor.next_token() {
                Ok(Some(_)) => {}
                Ok(None) => panic!("actual unterminated token must fail"),
                Err(error) => break error,
            }
        };
        assert_eq!(first.source_kind, MySqlSourceErrorKind::Message);
        assert!(first.message.contains("unterminated"), "{first:?}");
        let old_probes = probes.load(Ordering::Relaxed);
        cancelled.store(true, Ordering::Release);
        assert_same_first_error(&mut cursor, &facts, &first);
        assert_eq!(
            probes.load(Ordering::Relaxed),
            old_probes,
            "later cancel cannot replace or probe past first"
        );
        assert_eq!(owner.bytes(), bytes);
    }
}

#[test]
fn cpp_and_related_admission_failures_are_sticky_without_new_leases_or_lexical_work() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let dialect = crate::dialect::MySqlDialect {};
    for site in 0..4 {
        let facts = first_error_facts();
        facts.refuse_allocations.set(true);
        let cancelled = Arc::new(AtomicBool::new(false));
        let probe = {
            let cancelled = cancelled.clone();
            Arc::new(move || cancelled.load(Ordering::Acquire)) as CancellationCheck
        };
        let bytes = b"/* valid recognized comment */";
        let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
        let mut cursor = MySqlTokenCursor::new(
            &dialect,
            owner.occurrence(0..bytes.len()).unwrap(),
            &facts,
            Some(probe),
        )
        .unwrap();
        let first = match site {
            0 => cursor.next_token().err().expect("CPP admission"),
            1 => cursor.admit(16).err().expect("caller admission"),
            2 => cursor.admit_source(16).err().expect("source admission"),
            _ => cursor.admit_retained(16).err().expect("retained admission"),
        };
        assert_eq!(first.source_kind, MySqlSourceErrorKind::ResourceExhausted);
        assert_eq!(first.message, "actual fixture admission refusal");
        facts.refuse_allocations.set(false);
        cancelled.store(true, Ordering::Release);
        assert_same_first_error(&mut cursor, &facts, &first);
        assert_eq!(cursor.has_comment(), site == 0);
        assert_eq!(owner.bytes(), bytes);
    }
}

#[test]
fn original_source_control_kind_and_early_cancel_survive_repeated_read_and_admission() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let dialect = crate::dialect::MySqlDialect {};
    for kind in [
        MySqlSourceErrorKind::ResourceExhausted,
        MySqlSourceErrorKind::DeadlineExceeded,
        MySqlSourceErrorKind::Cancelled,
    ] {
        let facts = first_error_facts();
        facts.fail_lexical.set(Some(kind));
        let cancelled = Arc::new(AtomicBool::new(false));
        let probe = {
            let cancelled = cancelled.clone();
            Arc::new(move || cancelled.load(Ordering::Acquire)) as CancellationCheck
        };
        let owner = SqlSourceOwner::adopt(b"1".to_vec(), SqlSourceAdmission::new(Arc::new(())));
        let mut cursor = MySqlTokenCursor::new(
            &dialect,
            owner.occurrence(0..1).unwrap(),
            &facts,
            Some(probe),
        )
        .unwrap();
        let first = cursor.next_token().err().unwrap();
        assert_eq!(first.source_kind, kind);
        assert_eq!(first.message, "original demanded unit failure");
        facts.fail_lexical.set(None);
        cancelled.store(true, Ordering::Release);
        assert_same_first_error(&mut cursor, &facts, &first);
    }
    let facts = first_error_facts();
    let cancelled = Arc::new(AtomicBool::new(true));
    let probe = {
        let cancelled = cancelled.clone();
        Arc::new(move || cancelled.load(Ordering::Acquire)) as CancellationCheck
    };
    let owner = SqlSourceOwner::adopt(b"1".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..1).unwrap(),
        &facts,
        Some(probe),
    )
    .unwrap();
    let first = cursor.next_token().err().unwrap();
    assert_eq!(first.source_kind, MySqlSourceErrorKind::Cancelled);
    assert_eq!(first.location, Location::new(1, 1));
    assert_eq!(facts.lexical_calls.get(), 0);
    cancelled.store(false, Ordering::Release);
    assert_same_first_error(&mut cursor, &facts, &first);
}

#[test]
fn conversion_failure_freezes_original_payload_but_valid_eof_and_siblings_remain_independent() {
    let dialect = crate::dialect::MySqlDialect {};
    let facts = first_error_facts();
    let bytes = b"'valid'";
    let owner = SqlSourceOwner::adopt(bytes.to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..bytes.len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    let Token::MySqlText(text) = cursor.next_token().unwrap().unwrap().token.token else {
        panic!("original text Item");
    };
    let first = cursor.system_text(&text).err().unwrap();
    let before = first_error_progress(&cursor, &facts);
    assert_eq!(cursor.system_text(&text).err().unwrap(), first);
    assert_eq!(first_error_progress(&cursor, &facts), before);
    assert_same_first_error(&mut cursor, &facts, &first);
    let mut sibling = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..bytes.len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    assert!(sibling.next_token().unwrap().is_some());
    assert!(sibling.next_token().unwrap().is_none());
    assert!(
        sibling.next_token().unwrap().is_none(),
        "valid EOF is not a failure"
    );
    assert!(
        sibling.admit(1).is_ok(),
        "a valid exhausted cursor has no poisoned ledger"
    );
    assert_same_first_error(&mut cursor, &facts, &first);
    assert_eq!(owner.bytes(), bytes);
}

#[test]
fn in_flight_raw_control_and_identifier_conversion_keep_their_first_error() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dialect = crate::dialect::MySqlDialect {};
    for prefix in [b"/*".as_slice(), b"'".as_slice(), b"identifier".as_slice()] {
        let facts = first_error_facts();
        let mut bytes = prefix.to_vec();
        bytes.extend(core::iter::repeat_n(b'a', 8192));
        let probes = Arc::new(AtomicUsize::new(0));
        let probe = {
            let probes = probes.clone();
            Arc::new(move || probes.fetch_add(1, Ordering::Relaxed) != 0) as CancellationCheck
        };
        let owner = SqlSourceOwner::adopt(bytes.clone(), SqlSourceAdmission::new(Arc::new(())));
        let mut cursor = MySqlTokenCursor::new(
            &dialect,
            owner.occurrence(0..bytes.len()).unwrap(),
            &facts,
            Some(probe),
        )
        .unwrap();
        let first = cursor
            .next_token()
            .err()
            .expect("actual in-flight cancellation");
        assert_eq!(first.source_kind, MySqlSourceErrorKind::Cancelled);
        assert_eq!(first.message, QUERY_CANCELLED_MESSAGE);
        assert!(cursor.consumed() > 0);
        assert!(cursor.consumed() < bytes.len());
        let before = probes.load(Ordering::Relaxed);
        assert_same_first_error(&mut cursor, &facts, &first);
        assert_eq!(probes.load(Ordering::Relaxed), before);
        assert_eq!(owner.bytes(), bytes);
    }
    let mut facts = first_error_facts();
    facts.multibyte_slash = true;
    let owner = SqlSourceOwner::adopt(
        vec![0x81, b'/', b'a'],
        SqlSourceAdmission::new(Arc::new(())),
    );
    let mut cursor = MySqlTokenCursor::new(
        &dialect,
        owner.occurrence(0..owner.bytes().len()).unwrap(),
        &facts,
        None,
    )
    .unwrap();
    let first = cursor
        .next_token()
        .err()
        .expect("actual IDENT_sys conversion");
    assert_eq!(first.source_kind, MySqlSourceErrorKind::Message);
    assert_eq!(first.message, "fixture invalid identifier");
    assert_eq!(facts.conversion_calls.get(), 1);
    assert_same_first_error(&mut cursor, &facts, &first);
    assert!(
        !cursor.has_comment(),
        "native MB trail slash is not a comment"
    );
}

#[test]
fn canonical_hint_sink_uses_exact_tokens_empty_close_full_and_unary_roles() {
    let facts = canonical_facts(1024);
    let dialect = crate::dialect::MySqlDialect {};
    let owner = SqlSourceOwner::adopt(b"SELECT 1".to_vec(), SqlSourceAdmission::new(Arc::new(())));
    let mut cursor =
        MySqlTokenCursor::new(&dialect, owner.occurrence(0..8).unwrap(), &facts, None).unwrap();
    cursor.begin_canonical_digest().unwrap();
    let digest = cursor.digest.as_mut().unwrap();
    digest.push(748, false);
    let mut started = false;
    digest.hint_token(MySqlHintTokenKind::Close, b"*/", &mut started);
    assert_eq!(
        digest.tokens,
        digest_wire(&[748]),
        "empty does not create hint delimiters"
    );
    digest.hint_token(
        MySqlHintTokenKind::Keyword(MySqlHintKeyword::MaxExecutionTime),
        b"MAX_EXECUTION_TIME",
        &mut started,
    );
    digest.hint_token(MySqlHintTokenKind::Symbol(b'('), b"(", &mut started);
    digest.hint_token(MySqlHintTokenKind::Number, b"25", &mut started);
    digest.hint_token(MySqlHintTokenKind::Symbol(b')'), b")", &mut started);
    digest.hint_token(MySqlHintTokenKind::Close, b"*/", &mut started);
    assert_eq!(digest.tokens, digest_wire(&[748, 1108, 1000, 1102, 1109]));
    let before = digest.tokens.clone();
    digest.hint_token(MySqlHintTokenKind::Error, b"unscanned", &mut started);
    assert_eq!(digest.tokens, before, "native error has no appended token");
    digest.full = true;
    digest.hint_token(
        MySqlHintTokenKind::Keyword(MySqlHintKeyword::SetVar),
        b"SET_VAR",
        &mut started,
    );
    assert_eq!(digest.tokens, before, "FULL keeps its exact prefix");
}

#[test]
fn canonical_hint_keyword_mapping_keeps_fixed_lexical_identity() {
    assert_eq!(
        MySqlHintKeyword::from_word("sEt_VaR"),
        Some(MySqlHintKeyword::SetVar)
    );
    assert_eq!(MySqlHintKeyword::SetVar.digest_token(), 1027);
    assert_eq!(MySqlHintKeyword::NoRangeOptimization.digest_token(), 1013);
    assert_eq!(MySqlHintKeyword::from_word("NO_SUCH_HINT"), None);
    assert_eq!(MySqlHintKeyword::from_word("SET_VAR_suffix"), None);
    assert_eq!(MySqlHintKeyword::from_word("SÉT_VAR"), None);
    let facts = canonical_facts(1024);
    let receipt = canonical_tokens(b"SELECT /*+ SET_VAR(max_error_count=1) */ 1", &facts);
    assert_eq!(
        receipt.unavailable(),
        Some(MySqlDigestUnavailable::Hint),
        "a default adapter has no true hint service; metadata lookup is not authority"
    );
}
