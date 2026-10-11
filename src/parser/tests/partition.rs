use crate::{
    ast::{ForValues, PartitionBoundValue, Spanned, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
    tokenizer::Location,
};

fn bounds(sql: &str) -> ForValues {
    let mut statements = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
    let Statement::CreateTable(create) = statements.remove(0) else {
        panic!("expected CREATE TABLE")
    };
    create.for_values.unwrap()
}

#[test]
fn partition_spec_starts_at_actual_native_bound_keyword() {
    for (clause, keyword) in [
        ("FOR VALUES IN ('FROM', 12)", "IN"),
        ("FOR VALUES FROM (MINVALUE) TO (MAXVALUE)", "FROM"),
        ("FOR VALUES WITH (MODULUS 8, REMAINDER 3)", "WITH"),
        ("DEFAULT", "DEFAULT"),
    ] {
        let sql = format!("CREATE TABLE child PARTITION OF parent {clause}");
        let value = bounds(&sql);
        assert_eq!(
            value.span().start,
            Location::new(1, (sql.find(keyword).unwrap() + 1) as u64)
        );
        assert_eq!(value.to_string(), clause);
        // Attached source tokens do not alter the semantic AST identity.
        assert_eq!(value, bounds(&format!("\n\n{sql}")));
    }
}

#[test]
fn range_sentinels_and_expressions_retain_separate_actual_tokens() {
    let sql =
        "CREATE TABLE child PARTITION OF parent FOR VALUES FROM (MINVALUE, 12) TO (MAXVALUE, 99)";
    let ForValues::From { from, to, token } = bounds(sql) else {
        panic!("range bounds")
    };
    assert_eq!(
        token.span().start.column,
        (sql.find("FROM").unwrap() + 1) as u64
    );
    assert!(matches!(&from[0], PartitionBoundValue::MinValue(_)));
    assert!(matches!(&to[0], PartitionBoundValue::MaxValue(_)));
    for (value, spelling) in [
        (&from[0], "MINVALUE"),
        (&from[1], "12"),
        (&to[0], "MAXVALUE"),
        (&to[1], "99"),
    ] {
        assert_eq!(
            value.span().start.column,
            (sql.find(spelling).unwrap() + 1) as u64
        );
    }
}

#[test]
fn direct_partition_bound_parser_keeps_multiline_token_locations() {
    let dialect = PostgreSqlDialect {};
    let mut parser = Parser::new(&dialect)
        .try_with_sql("\n FOR VALUES FROM (\n MINVALUE) TO (MAXVALUE)")
        .unwrap();
    let ForValues::From { from, to, token } = parser.parse_partition_for_values().unwrap() else {
        panic!("range bounds")
    };
    assert_eq!(token.span().start, Location::new(2, 13));
    assert_eq!(from[0].span().start, Location::new(3, 2));
    assert_eq!(to[0].span().start, Location::new(3, 16));
}
