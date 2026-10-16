use crate::ast::{
    AlterOperatorOperation, Expr, OperatorOption, SelectItem, SetExpr, Statement, UnaryOperator,
};
use crate::dialect::PostgreSqlDialect;
use crate::parser::Parser;

fn parse_one(sql: &str) -> Statement {
    let statements = Parser::parse_sql(&PostgreSqlDialect {}, sql)
        .unwrap_or_else(|error| panic!("failed to parse `{sql}`: {error}"));
    let [statement] = statements.as_slice() else {
        panic!("expected one statement for `{sql}`: {statements:?}");
    };
    statement.clone()
}

#[test]
fn create_operator_preserves_boolean_options_and_last_restrict() {
    let statement = parse_one(
        "CREATE OPERATOR public.+ (FUNCTION = pg_catalog.int4pl, LEFTARG = integer, RIGHTARG = integer, HASHES = FALSE, MERGES = FALSE, RESTRICT = public.first_sel, RESTRICT = public.last_sel)",
    );
    let Statement::CreateOperator(create) = &statement else {
        panic!("{statement:?}");
    };
    assert!(create.function.is_some());
    assert!(create.options.contains(&OperatorOption::Hashes(false)));
    assert!(create.options.contains(&OperatorOption::Merges(false)));
    let restrict: Vec<_> = create
        .options
        .iter()
        .filter_map(|option| match option {
            OperatorOption::Restrict(Some(name)) => Some(name.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(restrict, ["public.first_sel", "public.last_sel"]);
    assert_eq!(parse_one(&statement.to_string()), statement);
}

#[test]
fn create_operator_without_function_reaches_semantic_validation() {
    let statement = parse_one("CREATE OPERATOR public.@@ (RIGHTARG = integer)");
    let Statement::CreateOperator(create) = &statement else {
        panic!("{statement:?}");
    };
    assert!(create.function.is_none());
    assert_eq!(parse_one(&statement.to_string()), statement);
}

#[test]
fn alter_operator_boolean_options_round_trip() {
    let statement = parse_one(
        "ALTER OPERATOR public.+ (integer, integer) SET (HASHES = FALSE, MERGES = FALSE)",
    );
    let Statement::AlterOperator(alter) = &statement else {
        panic!("{statement:?}");
    };
    let AlterOperatorOperation::Set { options } = &alter.operation else {
        panic!("{alter:?}");
    };
    assert_eq!(
        options,
        &[OperatorOption::Hashes(false), OperatorOption::Merges(false)]
    );
    assert_eq!(parse_one(&statement.to_string()), statement);
}

#[test]
fn custom_prefix_operators_keep_bare_and_qualified_names() {
    for (sql, expected_parts) in [
        ("SELECT &% 3", vec!["&%"]),
        ("SELECT OPERATOR(public.&%) 3", vec!["public", "&%"]),
        ("SELECT & 3", vec!["&"]),
        ("SELECT OPERATOR(public.&) 3", vec!["public", "&"]),
    ] {
        let statement = parse_one(sql);
        let Statement::Query(query) = &statement else {
            panic!("{statement:?}");
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("{query:?}");
        };
        let [SelectItem::UnnamedExpr(Expr::UnaryOp {
            op: UnaryOperator::PGCustomUnaryOperator(parts),
            ..
        })] = select.projection.as_slice()
        else {
            panic!("{select:?}");
        };
        assert_eq!(parts, &expected_parts);
        assert_eq!(parse_one(&statement.to_string()), statement);
    }
}

#[test]
fn ddl_operator_names_join_adjacent_symbol_tokens() {
    for (sql, expected) in [
        (
            "CREATE OPERATOR custom.=== (FUNCTION = custom.equal, LEFTARG = int4, RIGHTARG = int4)",
            "custom.===",
        ),
        ("DROP OPERATOR custom.!== (int4, int4)", "custom.!=="),
    ] {
        let statement = parse_one(sql);
        let name = match &statement {
            Statement::CreateOperator(create) => &create.name,
            Statement::DropOperator(drop) => &drop.operators[0].name,
            other => panic!("unexpected statement: {other:?}"),
        };
        assert_eq!(name.to_string(), expected);
    }
}

#[test]
fn postgres_pattern_escape_keeps_the_expression_inside_the_system_call() {
    use crate::ast::{BinaryOperator, FunctionArg, FunctionArgExpr, FunctionArguments};
    for (sql, operator, function) in [
        (
            "SELECT 'a_c'::bytea NOT LIKE 'a$__'::bytea ESCAPE '$'::bytea",
            "!~~",
            "pg_catalog.like_escape",
        ),
        (
            "SELECT 'Ab_c' NOT ILIKE 'a!_%' ESCAPE lower('!')",
            "!~~*",
            "pg_catalog.like_escape",
        ),
        (
            "SELECT 'abcd%' NOT SIMILAR TO '_bcd#%' ESCAPE '#'::text",
            "!~",
            "pg_catalog.similar_to_escape",
        ),
    ] {
        let Statement::Query(query) = parse_one(sql) else {
            panic!("query")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("select")
        };
        let SelectItem::UnnamedExpr(Expr::BinaryOp { op, right, .. }) = &select.projection[0]
        else {
            panic!("ESCAPE must not cast the boolean result")
        };
        assert_eq!(op, &BinaryOperator::Custom(operator.to_owned()));
        let Expr::Function(call) = right.as_ref() else {
            panic!("system escape routine")
        };
        assert_eq!(call.name.to_string(), function);
        assert_eq!(call.origin, crate::ast::FunctionOrigin::Source);
        let FunctionArguments::List(args) = &call.args else {
            panic!("arguments")
        };
        assert_eq!(args.args.len(), 2);
        let FunctionArg::Unnamed(FunctionArgExpr::Expr(escape)) = &args.args[1] else {
            panic!("escape operand")
        };
        assert!(
            matches!(escape, Expr::Cast { .. } | Expr::Function(_)),
            "{escape:?}"
        );
    }
}

#[test]
fn postgres_escape_preserves_boolean_precedence_and_other_dialect_shapes() {
    use crate::ast::BinaryOperator;
    let Statement::Query(query) = parse_one("SELECT 'x' LIKE 'x' ESCAPE '' AND false") else {
        panic!("query")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("select")
    };
    let SelectItem::UnnamedExpr(Expr::BinaryOp {
        op: BinaryOperator::And,
        left,
        ..
    }) = &select.projection[0]
    else {
        panic!("AND belongs outside ESCAPE")
    };
    assert!(
        matches!(left.as_ref(), Expr::BinaryOp { op: BinaryOperator::Custom(operator), .. } if operator == "~~")
    );
    let generic = Parser::parse_sql(
        &crate::dialect::GenericDialect {},
        "SELECT 'x' LIKE 'x' ESCAPE ''",
    )
    .unwrap();
    let Statement::Query(query) = &generic[0] else {
        panic!("query")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("select")
    };
    assert!(matches!(
        &select.projection[0],
        SelectItem::UnnamedExpr(Expr::Like {
            escape_char: Some(_),
            ..
        })
    ));
    assert!(Parser::parse_sql(
        &PostgreSqlDialect {},
        "SELECT 'x' LIKE ANY(ARRAY['x']) ESCAPE ''"
    )
    .is_err());
}

#[test]
fn postgres_insert_parenthesized_query_is_not_a_target_column_list() {
    for sql in [
        "INSERT INTO t (SELECT '3')",
        "INSERT INTO t (VALUES('4'))",
        "INSERT INTO t ((SELECT '3'))",
        "INSERT INTO t (WITH q AS (SELECT '3') SELECT * FROM q)",
        "INSERT INTO t (TABLE other_table)",
        "INSERT INTO t ((TABLE public.other_table))",
        "INSERT INTO t (TABLE other_table UNION ALL TABLE public.other_table)",
        "INSERT INTO t (TABLE other_table ORDER BY value LIMIT 1)",
    ] {
        let Statement::Insert(insert) = parse_one(sql) else {
            panic!("insert")
        };
        assert!(insert.columns.is_empty(), "{sql}");
        assert!(insert.source.is_some(), "{sql}");
        assert_eq!(
            parse_one(&Statement::Insert(insert.clone()).to_string()),
            Statement::Insert(insert)
        );
    }
    for (sql, name) in [
        ("INSERT INTO t (value) VALUES('3')", "value"),
        (
            "INSERT INTO t (value) (TABLE other_table) RETURNING value",
            "value",
        ),
        ("INSERT INTO t (\"SELECT\") (SELECT '3')", "SELECT"),
        ("INSERT INTO t (\"VALUES\") VALUES('4')", "VALUES"),
    ] {
        let Statement::Insert(insert) = parse_one(sql) else {
            panic!("insert")
        };
        assert_eq!(insert.columns.len(), 1, "{sql}");
        assert_eq!(
            insert.columns[0].to_string().trim_matches('"'),
            name,
            "{sql}"
        );
        assert!(insert.source.is_some(), "{sql}");
    }
    assert!(
        Parser::parse_sql(&PostgreSqlDialect {}, "INSERT INTO t (value,) VALUES('3')").is_err()
    );
    assert!(Parser::parse_sql(&PostgreSqlDialect {}, "INSERT INTO t ('3')").is_err());
}
