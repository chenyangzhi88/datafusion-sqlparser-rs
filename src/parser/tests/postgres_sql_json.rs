use crate::{
    ast::{Expr, JsonArraySource, SelectItem, SetExpr, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};
#[test]
fn standard_json_grammar_retains_values_query_options_and_predicates() {
    for sql in [
        "SELECT JSON_ARRAY(1, 'x' FORMAT JSON NULL ON NULL RETURNING jsonb)",
        "SELECT JSON_ARRAYAGG(x FORMAT JSON NULL ON NULL RETURNING jsonb) FROM t",
        "SELECT JSON_ARRAY(SELECT x FROM (VALUES (1),(2)) q(x) RETURNING bytea FORMAT JSON ENCODING UTF8)",
        "SELECT JSON_OBJECT('a' VALUE 1, 'b': NULL ABSENT ON NULL WITH UNIQUE KEYS RETURNING text)",
        "SELECT JSON_OBJECT(RETURNING jsonb)",
        "SELECT '[]' IS JSON ARRAY WITH UNIQUE KEYS, 'null' IS NOT JSON SCALAR WITHOUT UNIQUE",
    ] {
        let parsed = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
        let displayed = parsed[0].to_string();
        assert_eq!(
            parsed,
            Parser::parse_sql(&PostgreSqlDialect {}, &displayed).unwrap(),
            "{sql}"
        );
    }
    let parsed =
        Parser::parse_sql(&PostgreSqlDialect {}, "SELECT JSON_ARRAY(VALUES (1),(2))").unwrap();
    let Statement::Query(query) = &parsed[0] else {
        panic!("query")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("select")
    };
    let SelectItem::UnnamedExpr(Expr::JsonArray(array)) = &select.projection[0] else {
        panic!("constructor")
    };
    assert!(matches!(array.source, JsonArraySource::Query { .. }));
}
#[test]
fn legacy_json_object_and_quoted_names_remain_ordinary_routine_calls() {
    for sql in [
        "SELECT JSON_OBJECT(ARRAY['a','1'])",
        "SELECT pg_catalog.json_object(ARRAY['a','1'])",
        "SELECT \"json_array\"(1)",
    ] {
        let parsed = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
        let Statement::Query(query) = &parsed[0] else {
            panic!("query")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("select")
        };
        assert!(matches!(
            &select.projection[0],
            SelectItem::UnnamedExpr(Expr::Function(_))
        ));
    }
    for sql in [
        "SELECT JSON_ARRAY(1 WITH UNIQUE KEYS)",
        "SELECT JSON_ARRAY(SELECT 1 NULL ON NULL)",
        "SELECT JSON_OBJECT('a' VALUE 1,)",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, sql).is_err(),
            "{sql}"
        );
    }
}
