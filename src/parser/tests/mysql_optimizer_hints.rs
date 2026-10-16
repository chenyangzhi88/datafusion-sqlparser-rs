use crate::ast::{OptimizerHintStyle, SetExpr, Statement};
use crate::dialect::MySqlDialect;
use crate::parser::Parser;

#[test]
fn optimizer_hint_carrier_does_not_turn_other_comments_into_native_hints() {
    for (sql, expected) in [
        ("SELECT /*+ SET_VAR(sql_buffer_result=ON) */ 1", 1),
        ("SELECT /*abc+ SET_VAR(sql_buffer_result=ON) */ 1", 0),
        ("SELECT /*ordinary SET_VAR(sql_buffer_result=ON) */ 1", 0),
    ] {
        let statement = Parser::parse_sql(&MySqlDialect {}, sql).unwrap().remove(0);
        let Statement::Query(query) = statement else {
            panic!("query");
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("select");
        };
        assert_eq!(select.optimizer_hints.len(), expected);
        for hint in &select.optimizer_hints {
            assert!(matches!(hint.style, OptimizerHintStyle::MultiLine));
        }
    }
}
