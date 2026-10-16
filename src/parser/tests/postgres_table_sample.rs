use crate::{
    ast::{Expr, SetExpr, Statement, TableFactor, TableSampleKind},
    dialect::PostgreSqlDialect,
    parser::Parser,
};

#[test]
fn repeatable_retains_the_actual_signed_and_function_seed_expression() {
    for source in [
        "SELECT * FROM t TABLESAMPLE SYSTEM(10) REPEATABLE(-1)",
        "SELECT * FROM t TABLESAMPLE BERNOULLI(10) REPEATABLE(abs(-1) + 2)",
    ] {
        let parsed = Parser::parse_sql(&PostgreSqlDialect {}, source).unwrap();
        let Statement::Query(query) = &parsed[0] else {
            panic!("actual source query lost");
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("source SELECT lost");
        };
        let TableFactor::Table {
            sample: Some(kind), ..
        } = &select.from[0].relation
        else {
            panic!("sample clause lost");
        };
        let sample = match kind {
            TableSampleKind::BeforeTableAlias(sample)
            | TableSampleKind::AfterTableAlias(sample) => sample,
        };
        assert!(matches!(
            sample.seed.as_ref().unwrap().value,
            Expr::UnaryOp { .. } | Expr::BinaryOp { .. }
        ));
        let again = Parser::parse_sql(&PostgreSqlDialect {}, &parsed[0].to_string()).unwrap();
        assert_eq!(parsed[0], again[0]);
    }
}
