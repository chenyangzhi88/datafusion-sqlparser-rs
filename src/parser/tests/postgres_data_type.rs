use crate::ast::{DataType, ExactNumberInfo};
use crate::dialect::PostgreSqlDialect;
use crate::parser::{Parser, ParserError};
use crate::tokenizer::Tokenizer;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

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
fn postgres_numeric_rejects_unary_plus_scale() {
    for sql in ["numeric(10,+2)", "pg_catalog.numeric(10,+2)"] {
        let error = Parser::new(&PostgreSqlDialect {})
            .try_with_sql(sql)
            .unwrap()
            .parse_data_type()
            .unwrap_err()
            .to_string();
        assert!(error.contains("found: +"), "{sql}: {error}");
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

#[test]
fn domain_constraints_use_column_nullability_and_named_checks() {
    use crate::ast::{ColumnOption, Statement};
    let sql = "CREATE DOMAIN d AS integer NOT NULL CONSTRAINT positive CHECK(VALUE>0)";
    let Statement::CreateDomain(domain) = Parser::parse_sql(&PostgreSqlDialect {}, sql)
        .unwrap()
        .remove(0)
    else {
        panic!("domain")
    };
    assert_eq!(domain.constraints.len(), 2);
    assert!(matches!(
        domain.constraints[0].option,
        ColumnOption::NotNull
    ));
    assert_eq!(
        domain.constraints[1].name.as_ref().unwrap().value,
        "positive"
    );
    assert!(matches!(
        domain.constraints[1].option,
        ColumnOption::Check(_)
    ));
    assert_eq!(
        domain.to_string(),
        "CREATE DOMAIN d AS INTEGER NOT NULL CONSTRAINT positive CHECK (VALUE > 0)"
    );
    let Statement::CreateDomain(domain) = Parser::parse_sql(
        &PostgreSqlDialect {},
        "CREATE DOMAIN d AS integer CONSTRAINT nullable NULL",
    )
    .unwrap()
    .remove(0) else {
        panic!("domain")
    };
    assert!(matches!(domain.constraints[0].option, ColumnOption::Null));
    for sql in [
        "CREATE DOMAIN d AS integer UNIQUE",
        "CREATE DOMAIN d AS integer CONSTRAINT c",
        "CREATE DOMAIN d AS integer REFERENCES t",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, sql).is_err(),
            "{sql}"
        );
    }
}

#[test]
fn multirange_creation_name_retains_the_type_grammar_ast() {
    use crate::ast::{
        Ident, ObjectName, Statement, UserDefinedTypeRangeOption, UserDefinedTypeRepresentation,
    };
    for (argument, expected) in [
        ("int", DataType::Int(None)),
        ("double precision", DataType::DoublePrecision),
        (
            "\"int\"",
            DataType::Custom(ObjectName::from(Ident::with_quote('"', "int")), vec![]),
        ),
        (
            "public.int",
            DataType::Custom(
                ObjectName::from(vec![Ident::new("public"), Ident::new("int")]),
                vec![],
            ),
        ),
        (
            "'Int'",
            DataType::Custom(ObjectName::from(Ident::with_quote('"', "Int")), vec![]),
        ),
    ] {
        let sql = format!("CREATE TYPE r AS RANGE (SUBTYPE=text, MULTIRANGE_TYPE_NAME={argument})");
        let statements = Parser::parse_sql(&PostgreSqlDialect {}, &sql).unwrap();
        let Statement::CreateType {
            representation: Some(UserDefinedTypeRepresentation::Range { options }),
            ..
        } = &statements[0]
        else {
            panic!("range definition expected");
        };
        assert_eq!(
            options[1],
            UserDefinedTypeRangeOption::MultirangeTypeName(expected),
            "{sql}"
        );
    }
    let statements = Parser::parse_sql(
        &PostgreSqlDialect {},
        "CREATE TYPE r AS RANGE (SUBTYPE=text, MULTIRANGE_TYPE_NAME=numeric(9,2)[])",
    )
    .unwrap();
    let Statement::CreateType {
        representation: Some(UserDefinedTypeRepresentation::Range { options }),
        ..
    } = &statements[0]
    else {
        panic!("range definition expected");
    };
    let UserDefinedTypeRangeOption::MultirangeTypeName(DataType::Array(
        crate::ast::ArrayElemTypeDef::SquareBracket(inner, _),
    )) = &options[1]
    else {
        panic!("definition argument must retain array decoration");
    };
    assert_eq!(
        inner.as_ref(),
        &DataType::Numeric(ExactNumberInfo::PrecisionAndScale(9, 2))
    );
}

#[test]
fn interval_prefix_precision_keeps_full_range_and_literal_origin() {
    use crate::ast::{Expr, SelectItem, SetExpr, Statement, Value};
    for precision in [0, 2, 6, 7] {
        let sql = format!("SELECT INTERVAL({precision}) '1 day 01:23:45.6789'");
        let statements = Parser::parse_sql(&PostgreSqlDialect {}, &sql).unwrap();
        let Statement::Query(query) = &statements[0] else {
            panic!("expected SELECT")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("expected SELECT")
        };
        let SelectItem::UnnamedExpr(Expr::Interval(interval)) = &select.projection[0] else {
            panic!("expected interval literal")
        };
        assert_eq!(interval.leading_field, None);
        assert_eq!(interval.leading_precision, Some(precision));
        assert_eq!(interval.last_field, None);
        assert_eq!(interval.fractional_seconds_precision, None);
        let Expr::Value(value) = interval.value.as_ref() else {
            panic!("expected string")
        };
        assert!(matches!(&value.value, Value::SingleQuotedString(_)));
        assert_eq!(
            value.span.start.column,
            sql.find("'1 day").unwrap() as u64 + 1
        );
        assert_eq!(
            Parser::parse_sql(&PostgreSqlDialect {}, &statements[0].to_string()).unwrap(),
            statements
        );
    }
    // DAY can be an output alias here; it is not a precision range qualifier.
    let statements = Parser::parse_sql(
        &PostgreSqlDialect {},
        "SELECT INTERVAL(2) '1 day 01:02:03.456' DAY",
    )
    .unwrap();
    let Statement::Query(query) = &statements[0] else {
        panic!()
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!()
    };
    let SelectItem::ExprWithAlias {
        expr: Expr::Interval(interval),
        alias,
    } = &select.projection[0]
    else {
        panic!()
    };
    assert_eq!(alias.value, "DAY");
    assert_eq!(interval.leading_field, None);
    assert_eq!(interval.leading_precision, Some(2));
    for sql in [
        "SELECT INTERVAL(2) '1' DAY TO SECOND",
        "SELECT INTERVAL(2) '1' SECOND(3)",
        "SELECT INTERVAL(2) 1",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, sql).is_err(),
            "{sql}"
        );
    }
}

#[test]
fn alter_type_support_options_keep_order_quoting_none_and_round_trip() {
    use crate::ast::{AlterTypeOperation, Expr, SqlOption, Statement};
    let sql = "ALTER TYPE public.support_type SET (SEND=NONE, STORAGE=main, STORAGE=plain, TYPMOD_IN=pg_catalog.varchartypmodin, \"RECEIVE\"='None')";
    let statements = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
    let Statement::AlterType(crate::ast::AlterType {
        operation: AlterTypeOperation::SetDefinition { options },
        ..
    }) = &statements[0]
    else {
        panic!("ALTER TYPE support options")
    };
    assert_eq!(options.len(), 5);
    let SqlOption::KeyValue {
        key,
        value: Expr::Identifier(value),
    } = &options[0]
    else {
        panic!("NONE option")
    };
    assert_eq!(key.value, "SEND");
    assert_eq!(value.value, "NONE");
    assert_eq!(value.quote_style, None);
    let SqlOption::KeyValue { key, .. } = &options[4] else {
        panic!("quoted option")
    };
    assert_eq!(key.quote_style, Some('"'));
    assert_eq!(
        Parser::parse_sql(&PostgreSqlDialect {}, &statements[0].to_string()).unwrap(),
        statements
    );
    for sql in [
        "ALTER TYPE t SET ()",
        "ALTER TYPE t SET (SEND)",
        "ALTER TYPE t SET (SEND=NONE,)",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, sql).is_err(),
            "{sql}"
        );
    }
}

#[test]
fn postgres_type_modifier_names_and_literals_keep_distinct_lexical_values() {
    let mut parser = Parser::new(&PostgreSqlDialect {})
        .try_with_sql("support_text(FOLDED, \"Preserved\", 'LITERAL')")
        .unwrap();
    let DataType::Custom(_, modifiers) = parser.parse_data_type().unwrap() else {
        panic!("custom type");
    };
    assert_eq!(modifiers, ["folded", "Preserved", "LITERAL"]);
}

#[test]
fn postgres_type_modifier_rejects_null_and_boolean_constants_but_keeps_quoted_names() {
    for literal in ["NULL", "TRUE", "FALSE"] {
        let sql = format!("support_text({literal})");
        let error = Parser::new(&PostgreSqlDialect {})
            .try_with_sql(&sql)
            .unwrap()
            .parse_data_type()
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("type modifiers must be simple constants or identifiers"),
            "{sql}: {error}"
        );
    }
    let mut parser = Parser::new(&PostgreSqlDialect {})
        .try_with_sql("support_text(\"NULL\", 'TRUE', \"FALSE\")")
        .unwrap();
    let DataType::Custom(_, modifiers) = parser.parse_data_type().unwrap() else {
        panic!("custom type");
    };
    assert_eq!(modifiers, ["NULL", "TRUE", "FALSE"]);
}
