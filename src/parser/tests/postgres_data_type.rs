use crate::ast::{DataType, ExactNumberInfo};
use crate::dialect::PostgreSqlDialect;
use crate::parser::{Parser, ParserError};
use crate::tokenizer::Tokenizer;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn qualified_numeric_accepts_signed_scale_modifiers() {
    for (sql, expected_scale) in [
        ("pg_catalog.numeric(10,-2)", -2),
        ("pg_catalog.numeric(10,2)", 2),
    ] {
        let mut parser = Parser::new(&PostgreSqlDialect {})
            .try_with_sql(sql)
            .unwrap();
        let DataType::Custom(_, modifiers) = parser.parse_data_type().unwrap() else {
            panic!("qualified numeric must remain a custom type before catalog binding");
        };
        assert_eq!(modifiers, vec!["10".to_owned(), expected_scale.to_string()]);
    }
}

#[test]
fn qualified_numeric_reports_the_missing_signed_scale_position() {
    let sql = "pg_catalog.numeric(8,-)";
    let error = Parser::new(&PostgreSqlDialect {})
        .try_with_sql(sql)
        .unwrap()
        .parse_data_type()
        .unwrap_err()
        .to_string();
    assert!(error.contains("Expected: number, found: )"), "{error}");
    assert!(error.contains("Line: 1, Column: 23"), "{error}");
}

#[test]
fn unqualified_numeric_keeps_exact_signed_scale_ast() {
    let mut parser = Parser::new(&PostgreSqlDialect {})
        .try_with_sql("numeric(10,-2)")
        .unwrap();
    assert_eq!(
        parser.parse_data_type().unwrap(),
        DataType::Numeric(ExactNumberInfo::PrecisionAndScale(10, -2))
    );
}

#[test]
fn postgres_numeric_accepts_unary_plus_scale() {
    for sql in ["numeric(10,+2)", "pg_catalog.numeric(10,+2)"] {
        let data_type = Parser::new(&PostgreSqlDialect {})
            .try_with_sql(sql)
            .unwrap()
            .parse_data_type()
            .unwrap();
        if sql.starts_with("pg_catalog.") {
            let DataType::Custom(_, modifiers) = data_type else {
                panic!("qualified numeric must remain a custom type before catalog binding");
            };
            assert_eq!(modifiers, vec!["10".to_owned(), "+2".to_owned()]);
        } else {
            assert_eq!(
                data_type,
                DataType::Numeric(ExactNumberInfo::PrecisionAndScale(10, 2))
            );
        }
    }
}

#[test]
fn parser_cancels_during_a_long_non_comma_expression_stream() {
    let dialect = PostgreSqlDialect {};
    let sql = format!("SELECT 1{}", "+1".repeat(8_192));
    let tokens = Tokenizer::new(&dialect, &sql)
        .tokenize_with_location()
        .unwrap();
    let probes = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&probes);
    let mut parser = Parser::new(&dialect)
        .with_tokens_with_locations(tokens)
        .with_cancellation(Arc::new(move || {
            observed.fetch_add(1, Ordering::Relaxed) >= 1_024
        }));
    let error = parser.parse_statements().unwrap_err();
    assert_eq!(error, ParserError::Cancelled);
    assert!(
        parser.get_current_index() > 200,
        "cancellation must occur after the parser enters the long expression stream"
    );
    assert!(probes.load(Ordering::Relaxed) >= 1_025);
}

#[test]
fn parser_cancels_during_a_long_custom_type_modifier_list() {
    let dialect = PostgreSqlDialect {};
    let sql = format!("custom_type({})", vec!["1"; 4_096].join(","));
    let tokens = Tokenizer::new(&dialect, &sql)
        .tokenize_with_location()
        .unwrap();
    let probes = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&probes);
    let mut parser = Parser::new(&dialect)
        .with_tokens_with_locations(tokens)
        .with_cancellation(Arc::new(move || {
            observed.fetch_add(1, Ordering::Relaxed) >= 256
        }));
    let error = parser.parse_data_type().unwrap_err();
    assert_eq!(error, ParserError::Cancelled);
    assert!(
        parser.get_current_index() > 200,
        "cancellation must occur inside the long modifier list"
    );
    assert!(probes.load(Ordering::Relaxed) >= 257);
}
