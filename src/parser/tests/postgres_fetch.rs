use crate::ast::Statement;
use crate::dialect::PostgreSqlDialect;
use crate::parser::Parser;

#[test]
fn fetch_quantity_accepts_postgres_expressions() {
    for quantity in ["NULLIF(1,1)", "(SELECT NULL::bigint)", "NULL", "2"] {
        let sql = format!(
            "SELECT x FROM (VALUES (1),(2)) v(x) ORDER BY x FETCH FIRST {quantity} ROWS WITH TIES"
        );
        let statements = Parser::parse_sql(&PostgreSqlDialect {}, &sql).unwrap();
        let [Statement::Query(query)] = statements.as_slice() else {
            panic!("expected one FETCH query: {statements:?}")
        };
        assert!(query.fetch.as_ref().unwrap().quantity.is_some(), "{sql}");
    }
}

#[test]
fn fetch_without_quantity_keeps_one_row_default() {
    let statements = Parser::parse_sql(
        &PostgreSqlDialect {},
        "SELECT x FROM (VALUES (1),(2)) v(x) ORDER BY x FETCH FIRST ROW WITH TIES",
    )
    .unwrap();
    let [Statement::Query(query)] = statements.as_slice() else {
        panic!("expected one FETCH query: {statements:?}")
    };
    assert!(query.fetch.as_ref().unwrap().quantity.is_none());
}
