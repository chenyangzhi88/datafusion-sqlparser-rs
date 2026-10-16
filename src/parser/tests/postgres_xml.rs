use crate::{
    ast::{Expr, FunctionArg, FunctionArgExpr, FunctionArguments, SelectItem, SetExpr, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};
#[test]
fn xmlexists_passing_binds_two_source_arguments() {
    for before in ["", "BY REF", "BY VALUE"] {
        for after in ["", "BY REF", "BY VALUE"] {
            let sql = format!("SELECT XMLEXISTS('/a' PASSING {before} '<a/>'::xml {after})");
            let statements = Parser::parse_sql(&PostgreSqlDialect {}, &sql).unwrap();
            let Statement::Query(query) = &statements[0] else {
                panic!()
            };
            let SetExpr::Select(select) = query.body.as_ref() else {
                panic!()
            };
            let SelectItem::UnnamedExpr(Expr::Function(function)) = &select.projection[0] else {
                panic!()
            };
            let FunctionArguments::List(arguments) = &function.args else {
                panic!()
            };
            assert_eq!(arguments.args.len(), 2);
        }
    }
}
#[test]
fn xmlexists_rejects_invalid_passing_mechanism_and_multiple_documents() {
    for sql in [
        "SELECT xmlexists('/a' PASSING BY garbage '<a/>'::xml)",
        "SELECT xmlexists('/a' PASSING '<a/>'::xml, '<b/>'::xml)",
        "SELECT pg_catalog.xmlexists('/a' PASSING '<a/>'::xml)",
        "SELECT \"xmlexists\"('/a' PASSING '<a/>'::xml)",
        "SELECT other_schema.xmlexists('/a' PASSING BY REF '<a/>'::xml BY VALUE)",
        "SELECT pg_catalog.\"xmlexists\"('/a' PASSING '<a/>'::xml)",
        "SELECT \"pg_catalog\".xmlexists('/a' PASSING '<a/>'::xml)",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, sql).is_err(),
            "{sql}"
        );
    }
    for name in [
        "pg_catalog.xmlexists",
        "other_schema.xmlexists",
        "\"xmlexists\"",
    ] {
        let sql = format!("SELECT {name}('/a','<a/>'::xml)");
        let parsed = Parser::parse_sql(&PostgreSqlDialect {}, &sql).unwrap();
        let [Statement::Query(query)] = parsed.as_slice() else {
            panic!("ordinary function SELECT: {sql}")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("ordinary function body: {sql}")
        };
        let [SelectItem::UnnamedExpr(Expr::Function(function))] = select.projection.as_slice()
        else {
            panic!("ordinary function call: {sql}")
        };
        assert_eq!(function.name.to_string(), name);
        let FunctionArguments::List(arguments) = &function.args else {
            panic!("ordinary arguments: {sql}")
        };
        let [FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(path))), FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Cast { .. }))] =
            arguments.args.as_slice()
        else {
            panic!("exact path and XML arguments: {sql}")
        };
        assert_eq!(
            path.value,
            crate::ast::Value::SingleQuotedString("/a".into())
        );
        assert!(arguments.clauses.is_empty());
        assert_eq!(
            Parser::parse_sql(&PostgreSqlDialect {}, &parsed[0].to_string()).unwrap(),
            parsed
        );
    }
}

#[test]
fn xmltable_retains_its_keyword_location_and_explicit_lateral() {
    use crate::ast::{Spanned, TableFactor};
    use crate::tokenizer::Location;
    for (lateral_sql, expected_column) in [("", 15), ("LATERAL ", 23)] {
        let sql = format!(
            "SELECT * FROM {lateral_sql}XMLTABLE('/a' PASSING '<a/>'::xml COLUMNS k int PATH '.') AS x"
        );
        let parsed = Parser::parse_sql(&PostgreSqlDialect {}, &sql).unwrap();
        let Statement::Query(query) = &parsed[0] else {
            panic!()
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!()
        };
        let factor = &select.from[0].relation;
        let TableFactor::XmlTable {
            xmltable_token,
            lateral,
            ..
        } = factor
        else {
            panic!()
        };
        assert_eq!(
            xmltable_token.0.span.start,
            Location::new(1, expected_column)
        );
        assert_eq!(factor.span().start, xmltable_token.0.span.start);
        assert_eq!(*lateral, !lateral_sql.is_empty());
        let rendered = parsed[0].to_string();
        assert_eq!(
            Parser::parse_sql(&PostgreSqlDialect {}, &rendered).unwrap(),
            parsed
        );
    }
}
