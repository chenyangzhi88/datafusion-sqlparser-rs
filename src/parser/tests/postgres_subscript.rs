use crate::ast::{AccessExpr, Expr, SelectItem, SetExpr, Statement, Subscript};
use crate::dialect::{GenericDialect, PostgreSqlDialect};
use crate::parser::Parser;

#[test]
fn postgres_slice_rejects_the_second_colon_even_without_a_stride() {
    for suffix in ["[1:2:1]", "[1:2:]", "[1:2:NULL]"] {
        let sql = format!("SELECT a{suffix}");
        let error = Parser::parse_sql(&PostgreSqlDialect {}, &sql).unwrap_err();
        assert!(
            error.to_string().contains("Expected: ], found: :"),
            "{sql}: {error}"
        );
    }
    for suffix in ["[:][3]", "[1:2]", "[1:]", "[:2]", "[1][2:3]"] {
        Parser::parse_sql(&PostgreSqlDialect {}, &format!("SELECT a{suffix}")).unwrap();
    }
}

#[test]
fn generic_dialect_keeps_its_stride_ast() {
    let statements = Parser::parse_sql(&GenericDialect {}, "SELECT a[1:2:1]").unwrap();
    let [Statement::Query(query)] = statements.as_slice() else {
        panic!("SELECT")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("SELECT body")
    };
    let [SelectItem::UnnamedExpr(Expr::CompoundFieldAccess { access_chain, .. })] =
        select.projection.as_slice()
    else {
        panic!("array access")
    };
    let [AccessExpr::Subscript(Subscript::Slice {
        stride: Some(_), ..
    })] = access_chain.as_slice()
    else {
        panic!("Generic stride must be retained")
    };
}

#[test]
fn postgres_composite_projection_star_keeps_the_actual_expression() {
    use crate::ast::{SelectItemQualifiedWildcardKind, Spanned};
    use crate::tokenizer::Location;
    for source in [
        "SELECT (d.r).* FROM (SELECT record_origin() AS r) AS d",
        "SELECT (record_origin()).*",
        "SELECT (collated_record('x'::text COLLATE \"C\")).*",
        "SELECT (arr[1]).* FROM t",
        "SELECT\n  (arr[1]). /* projection marker */ * FROM t",
    ] {
        let statements = Parser::parse_sql(&PostgreSqlDialect {}, source).unwrap();
        let [Statement::Query(query)] = statements.as_slice() else {
            panic!("original SELECT");
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("SELECT body");
        };
        let [SelectItem::QualifiedWildcard(
            SelectItemQualifiedWildcardKind::Expr(expression),
            options,
        )] = select.projection.as_slice()
        else {
            panic!("composite expression wildcard: {source}");
        };
        assert!(matches!(expression, Expr::Nested(_)), "{source}");
        assert_eq!(options.wildcard_token.0.token, crate::tokenizer::Token::Mul);
        let star_offset = source.rfind('*').unwrap();
        let prefix = &source[..star_offset];
        let line = prefix.split('\n').count() as u64;
        let column = prefix.rsplit('\n').next().unwrap().chars().count() as u64 + 1;
        let start = Location::new(line, column);
        let end = Location::new(line, column + 1);
        assert_eq!(options.wildcard_token.0.span.start, start, "{source}");
        assert_eq!(options.wildcard_token.0.span.end, end, "{source}");
        assert_eq!(select.projection[0].span().end, end, "{source}");
        let rendered = statements[0].to_string();
        let reparsed = Parser::parse_sql(&PostgreSqlDialect {}, &rendered).unwrap();
        assert_eq!(reparsed[0].to_string(), rendered);
    }
}

#[test]
fn postgres_composite_star_in_atomic_body_and_ident_whole_row_are_distinct() {
    use crate::ast::{
        CreateFunctionBody, FunctionArg, FunctionArgExpr, FunctionArguments,
        SelectItemQualifiedWildcardKind,
    };
    let source = "CREATE FUNCTION derived_fields() RETURNS TABLE(a int,b text) LANGUAGE SQL BEGIN ATOMIC SELECT (d.r).* FROM(SELECT record_origin() AS r) AS d; END";
    let statements = Parser::parse_sql(&PostgreSqlDialect {}, source).unwrap();
    let [Statement::CreateFunction(function)] = statements.as_slice() else {
        panic!("CREATE FUNCTION");
    };
    let Some(CreateFunctionBody::BeginAtomic(body)) = &function.function_body else {
        panic!("actual atomic body");
    };
    let [Statement::Query(query)] = body.statements.as_slice() else {
        panic!("original body statement");
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("SELECT body");
    };
    assert!(matches!(
        &select.projection[0],
        SelectItem::QualifiedWildcard(SelectItemQualifiedWildcardKind::Expr(_), _)
    ));
    let statements = Parser::parse_sql(
        &PostgreSqlDialect {},
        "SELECT d.*, record_send(d.*) FROM t AS d",
    )
    .unwrap();
    let Statement::Query(query) = &statements[0] else {
        panic!("SELECT");
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("SELECT body");
    };
    assert!(matches!(
        &select.projection[0],
        SelectItem::QualifiedWildcard(SelectItemQualifiedWildcardKind::ObjectName(_), _)
    ));
    let SelectItem::UnnamedExpr(Expr::Function(call)) = &select.projection[1] else {
        panic!("whole row argument");
    };
    let FunctionArguments::List(arguments) = &call.args else {
        panic!("function arguments");
    };
    assert!(matches!(
        &arguments.args[0],
        FunctionArg::Unnamed(FunctionArgExpr::QualifiedWildcard(_))
    ));
}
