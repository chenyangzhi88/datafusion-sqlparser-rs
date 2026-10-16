use crate::{
    ast::{Expr, FunctionArguments, JsonArraySource, SelectItem, SetExpr, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};
#[test]
fn standard_json_grammar_retains_values_query_options_and_predicates() {
    for sql in [
        "SELECT JSON_ARRAY(1, 'x' FORMAT JSON NULL ON NULL RETURNING jsonb)",
        "SELECT JSON_ARRAYAGG(x FORMAT JSON NULL ON NULL RETURNING jsonb) FROM t",
        "SELECT JSON_ARRAYAGG(x ORDER BY y DESC NULLS LAST, z ABSENT ON NULL RETURNING bytea FORMAT JSON ENCODING UTF8) FILTER (WHERE x IS NOT NULL) FROM t",
        "SELECT JSON_ARRAYAGG(x NULL ON NULL RETURNING jsonb) FILTER (WHERE y > 0) OVER (PARTITION BY z ORDER BY y ROWS UNBOUNDED PRECEDING) FROM t",
        "SELECT JSON_OBJECTAGG(k VALUE v FORMAT JSON ABSENT ON NULL WITH UNIQUE KEYS RETURNING jsonb) FILTER (WHERE k IS NOT NULL) OVER w FROM t WINDOW w AS (ORDER BY k)",
        "SELECT JSON_OBJECTAGG(k: v NULL ON NULL WITHOUT UNIQUE RETURNING text) FROM t",
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
        "SELECT JSON_OBJECTAGG('a' VALUE 1 ORDER BY 1)",
        "SELECT JSON_ARRAYAGG(DISTINCT x) FROM t",
        "SELECT JSON_ARRAYAGG(x WITH UNIQUE KEYS) FROM t",
        "SELECT JSON_OBJECTAGG('a' VALUE 1, 'b' VALUE 2)",
        "SELECT JSON_ARRAYAGG(x) FILTER (x > 0) FROM t",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, sql).is_err(),
            "{sql}"
        );
    }
}

#[test]
fn postgres_empty_json_constructors_reject_null_options_without_legacy_fallback() {
    for sql in [
        "SELECT JSON_OBJECT(NULL ON NULL)",
        "SELECT JSON_OBJECT(ABSENT ON NULL)",
        "SELECT JSON_OBJECT(NULL ON NULL RETURNING jsonb)",
        "SELECT JSON_OBJECT(ABSENT ON NULL RETURNING jsonb)",
        "SELECT JSON_ARRAY(NULL ON NULL)",
        "SELECT JSON_ARRAY(ABSENT ON NULL)",
        "SELECT JSON_ARRAY(NULL ON NULL RETURNING jsonb)",
        "SELECT JSON_ARRAY(ABSENT ON NULL RETURNING jsonb)",
        "SELECT JSON_OBJECT(RETURNING jsonb NULL ON NULL)",
        "SELECT JSON_ARRAY(RETURNING jsonb ABSENT ON NULL)",
        "SELECT JSON_OBJECT(ARRAY['a','1'] NULL ON NULL)",
        "SELECT JSON_OBJECT(ARRAY['a','1'] RETURNING jsonb)",
        "SELECT JSON_OBJECT('a' VALUE 1 ORDER BY 1)",
        "SELECT JSON_OBJECT('a' VALUE 1) OVER()",
        "SELECT pg_catalog.json_object(NULL ON NULL)",
        "SELECT pg_catalog.json_object(ARRAY['a','1'] RETURNING jsonb)",
        "SELECT pg_catalog.json_array(ABSENT ON NULL)",
        "SELECT \"json_object\"(NULL ON NULL)",
        "SELECT \"json_array\"(1 RETURNING jsonb)",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, sql).is_err(),
            "{sql}"
        );
    }
}

#[test]
fn postgres_json_empty_output_and_value_options_keep_their_constructor_ast() {
    for (sql, object, empty) in [
        ("SELECT JSON_OBJECT()", true, true),
        ("SELECT JSON_OBJECT(RETURNING jsonb)", true, true),
        (
            "SELECT JSON_OBJECT(RETURNING bytea FORMAT JSON ENCODING UTF8)",
            true,
            true,
        ),
        ("SELECT JSON_ARRAY()", false, true),
        ("SELECT JSON_ARRAY(RETURNING jsonb)", false, true),
        (
            "SELECT JSON_ARRAY(RETURNING bytea FORMAT JSON ENCODING UTF8)",
            false,
            true,
        ),
        (
            "SELECT JSON_OBJECT('a' VALUE NULL NULL ON NULL RETURNING jsonb)",
            true,
            false,
        ),
        (
            "SELECT JSON_OBJECT('a' VALUE NULL ABSENT ON NULL RETURNING jsonb)",
            true,
            false,
        ),
        ("SELECT JSON_OBJECT(NULL VALUE 1 NULL ON NULL)", true, false),
        (
            "SELECT JSON_ARRAY(NULL NULL ON NULL RETURNING jsonb)",
            false,
            false,
        ),
        (
            "SELECT JSON_ARRAY(NULL ABSENT ON NULL RETURNING jsonb)",
            false,
            false,
        ),
    ] {
        let parsed = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
        let Statement::Query(query) = &parsed[0] else {
            panic!("query")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("select")
        };
        match &select.projection[0] {
            SelectItem::UnnamedExpr(Expr::JsonObject(value)) if object => {
                assert_eq!(value.entries.is_empty(), empty)
            }
            SelectItem::UnnamedExpr(Expr::JsonArray(value)) if !object => {
                let JsonArraySource::Values(values) = &value.source else {
                    panic!("values")
                };
                assert_eq!(values.is_empty(), empty);
            }
            _ => panic!("constructor AST: {sql}"),
        }
        assert_eq!(
            parsed,
            Parser::parse_sql(&PostgreSqlDialect {}, &parsed[0].to_string()).unwrap(),
            "{sql}"
        );
    }
}

#[test]
fn postgres_legacy_and_qualified_json_functions_keep_ordinary_expression_arguments() {
    for sql in [
        "SELECT JSON_OBJECT(ARRAY['a','1'])",
        "SELECT JSON_OBJECT(ARRAY['a'], ARRAY['1'])",
        "SELECT JSON_OBJECT(NULL)",
        "SELECT pg_catalog.json_object(ARRAY['a','1'])",
        "SELECT pg_catalog.json_object(ARRAY['a'],ARRAY['1'])",
        "SELECT custom_schema.json_object(ARRAY['a','1'])",
        "SELECT custom_schema.json_array(NULL,1)",
        "SELECT \"json_object\"(ARRAY['a','1'])",
        "SELECT \"json_array\"(NULL,1)",
    ] {
        let parsed = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
        let Statement::Query(query) = &parsed[0] else {
            panic!("query")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("select")
        };
        let SelectItem::UnnamedExpr(Expr::Function(call)) = &select.projection[0] else {
            panic!("legacy routine: {sql}")
        };
        let FunctionArguments::List(arguments) = &call.args else {
            panic!("argument list")
        };
        assert!(arguments.clauses.is_empty(), "{sql}");
        assert!(!arguments.args.is_empty(), "{sql}");
        assert_eq!(
            parsed,
            Parser::parse_sql(&PostgreSqlDialect {}, &parsed[0].to_string()).unwrap(),
            "{sql}"
        );
    }
}
