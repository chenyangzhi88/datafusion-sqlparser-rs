use crate::ast::{Expr, FunctionArguments, SelectItem, SetExpr, Statement};
use crate::dialect::{GenericDialect, MySqlDialect};
use crate::parser::Parser;

fn projection(sql: &str, mysql: bool) -> Vec<SelectItem> {
    let statements = if mysql {
        Parser::parse_sql(&MySqlDialect {}, sql)
    } else {
        Parser::parse_sql(&GenericDialect {}, sql)
    }
    .unwrap();
    let [Statement::Query(query)] = statements.as_slice() else {
        panic!("expected one SELECT statement: {sql}");
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("expected SELECT body: {sql}");
    };
    select.projection.clone()
}

#[test]
fn mysql_bare_utc_temporal_names_are_functions_without_changing_quoted_names() {
    for name in ["UTC_DATE", "UTC_TIME", "UTC_TIMESTAMP"] {
        for sql in [format!("SELECT {name}"), format!("SELECT {name}()")] {
            let items = projection(&sql, true);
            let [SelectItem::UnnamedExpr(Expr::Function(function))] = items.as_slice() else {
                panic!("expected temporal function: {sql}");
            };
            assert_eq!(function.name.to_string(), name);
            if sql.ends_with("()") {
                assert!(
                    matches!(&function.args, FunctionArguments::List(args) if args.args.is_empty())
                );
            } else {
                assert!(matches!(&function.args, FunctionArguments::None));
            }
        }
        let sql = format!("SELECT `{name}`");
        assert!(matches!(
            projection(&sql, true).as_slice(),
            [SelectItem::UnnamedExpr(Expr::Identifier(identifier))]
                if identifier.value == name && identifier.quote_style == Some('`')
        ));
        assert!(matches!(
            projection(&format!("SELECT {name}"), false).as_slice(),
            [SelectItem::UnnamedExpr(Expr::Identifier(_))]
        ));
    }
}
