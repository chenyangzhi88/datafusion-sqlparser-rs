use crate::ast::{BinaryOperator, Expr, SelectItem, SetExpr, Statement, UnaryOperator};
use crate::dialect::{GenericDialect, MySqlDialect};
use crate::parser::Parser;

fn projected_expression<D: crate::dialect::Dialect>(dialect: &D, sql: &str) -> Expr {
    let statements = Parser::parse_sql(dialect, sql).unwrap();
    let [Statement::Query(query)] = statements.as_slice() else {
        panic!("expected one SELECT for {sql}");
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("expected SELECT body for {sql}");
    };
    let SelectItem::UnnamedExpr(expression) = &select.projection[0] else {
        panic!("expected unnamed projection for {sql}");
    };
    expression.clone()
}

#[test]
fn mysql_caret_binds_above_arithmetic_but_below_unary_and_concat_mode() {
    let expression = projected_expression(&MySqlDialect {}, "SELECT 1 + 2 ^ 3");
    assert!(matches!(
        expression,
        Expr::BinaryOp { op: BinaryOperator::Plus, right, .. }
            if matches!(*right, Expr::BinaryOp { op: BinaryOperator::BitwiseXor, .. })
    ));
    let expression = projected_expression(&MySqlDialect {}, "SELECT -1 ^ 2");
    assert!(matches!(
        expression,
        Expr::BinaryOp { op: BinaryOperator::BitwiseXor, left, .. }
            if matches!(*left, Expr::UnaryOp { op: UnaryOperator::Minus, .. })
    ));
    let expression = projected_expression(&MySqlDialect {}, "SELECT 1 << 2 ^ 3");
    assert!(matches!(
        expression,
        Expr::BinaryOp { op: BinaryOperator::PGBitwiseShiftLeft, right, .. }
            if matches!(*right, Expr::BinaryOp { op: BinaryOperator::BitwiseXor, .. })
    ));
}

#[test]
fn generic_dialect_keeps_its_existing_caret_precedence() {
    let expression = projected_expression(&GenericDialect {}, "SELECT 1 + 2 ^ 3");
    assert!(matches!(
        expression,
        Expr::BinaryOp { op: BinaryOperator::BitwiseXor, left, .. }
            if matches!(*left, Expr::BinaryOp { op: BinaryOperator::Plus, .. })
    ));
}

#[test]
fn mysql_like_escape_keeps_its_introduced_literal_as_the_original_child() {
    let expression = projected_expression(&MySqlDialect {},
        "SELECT variable_name LIKE _latin1'Ssl!_%' ESCAPE _latin1'!' AND enabled = 1");
    let Expr::BinaryOp { left, op: BinaryOperator::And, right } = expression else {
        panic!("ESCAPE must not consume the following condition");
    };
    assert!(matches!(*right, Expr::BinaryOp { op: BinaryOperator::Eq, .. }));
    let Expr::Like { pattern, escape_char: Some(escape), .. } = *left else {
        panic!("the original LIKE owns its escape child");
    };
    for child in [pattern, escape] {
        assert!(matches!(*child, Expr::Prefixed { prefix, value }
            if prefix.value == "_latin1" && matches!(*value, Expr::Value(_))));
    }
    let plain = projected_expression(&MySqlDialect {},
        "SELECT variable_name LIKE 'Ssl!_%' ESCAPE '!'");
    assert!(matches!(plain, Expr::Like { escape_char: Some(escape), .. }
        if matches!(*escape, Expr::Value(_))));
    assert!(Parser::parse_sql(&MySqlDialect {},
        "SELECT variable_name LIKE 'x' ESCAPE _latin1").is_err());
}
