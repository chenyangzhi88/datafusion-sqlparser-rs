// The existing owning visitor unit tests; production APIs remain private.
use super::super::*;
use crate::ast::{CreateTable, CreateTableElement, Value};
use core::ops::ControlFlow;

#[test]
fn match_against_original_expression_child_is_visited_once_without_literal_narrowing() {
    let mut expression = Expr::MatchAgainst {
        columns: vec![crate::ast::ObjectName::from(vec![crate::ast::Ident::new(
            "body",
        )])],
        match_value: Box::new(Expr::BinaryOp {
            left: Box::new(Expr::Value(Value::Number("11".into(), false).into())),
            op: crate::ast::BinaryOperator::Plus,
            right: Box::new(Expr::Value(Value::Number("22".into(), false).into())),
        }),
        opt_search_modifier: Some(crate::ast::SearchModifier::InBooleanMode),
    };
    let mut reader = SourceOrderVisitor::default();
    assert!(Visit::visit(&expression, &mut reader).is_continue());
    assert_eq!(reader.numbers, ["11", "22"]);
    struct ReplaceActualChild;
    impl VisitorMut for ReplaceActualChild {
        type Break = ();
        fn post_visit_expr(&mut self, expression: &mut Expr) -> ControlFlow<()> {
            if let Expr::Value(value) = expression {
                if matches!(&value.value, Value::Number(number, _) if number == "22") {
                    *expression = Expr::Value(Value::Number("33".into(), false).into());
                }
            }
            ControlFlow::Continue(())
        }
    }
    assert!(VisitMut::visit(&mut expression, &mut ReplaceActualChild).is_continue());
    let mut reader = SourceOrderVisitor::default();
    assert!(Visit::visit(&expression, &mut reader).is_continue());
    assert_eq!(reader.numbers, ["11", "33"]);
}

#[test]
fn tuple_children_are_visited_in_order_and_break_stops_mutation() {
    let literal = |value: &str| Expr::Value(Value::Number(value.into(), false).into());
    let mut pair = (literal("11"), literal("22"));
    let mut reader = SourceOrderVisitor::default();
    assert!(Visit::visit(&pair, &mut reader).is_continue());
    assert_eq!(reader.numbers, ["11", "22"]);

    struct StopAfterFirst;
    impl VisitorMut for StopAfterFirst {
        type Break = &'static str;
        fn pre_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
            *expr = Expr::Value(Value::Number("33".into(), false).into());
            ControlFlow::Break("first child")
        }
    }
    assert_eq!(
        VisitMut::visit(&mut pair, &mut StopAfterFirst),
        ControlFlow::Break("first child")
    );
    assert_eq!(pair.0, literal("33"));
    assert_eq!(
        pair.1,
        literal("22"),
        "a break cannot reach the next tuple child"
    );

    let mut body = crate::ast::MySqlProgramBody {
        ordering: Some((true, crate::ast::Ident::with_quote('`', "earlier.trigger"))),
        comment: Some(literal("44")),
        ..Default::default()
    };
    let ordering = body.ordering.clone();
    let mut reader = SourceOrderVisitor::default();
    assert!(Visit::visit(&body, &mut reader).is_continue());
    assert_eq!(reader.numbers, ["44"]);
    assert_eq!(
        VisitMut::visit(&mut body, &mut StopAfterFirst),
        ControlFlow::Break("first child")
    );
    assert_eq!(
        body.ordering, ordering,
        "ordering remains the actual bool/Ident pair"
    );
    // Existing LOAD IGNORE grammar facts still use the same tuple API.
    assert!(Visit::visit(&(7u64, true), &mut reader).is_continue());
}

fn source_order_table() -> CreateTable {
    let mut statements = crate::parser::Parser::parse_sql(
        &crate::dialect::PostgreSqlDialect {},
        "CREATE TABLE ordered (a INT DEFAULT 11, CHECK (22), LIKE parent, b INT DEFAULT 33)",
    )
    .unwrap();
    let Statement::CreateTable(table) = statements.remove(0) else {
        panic!("table")
    };
    table
}

#[derive(Default)]
struct SourceOrderVisitor {
    numbers: Vec<String>,
    relations: Vec<String>,
}

impl Visitor for SourceOrderVisitor {
    type Break = ();
    fn pre_visit_expr(&mut self, value: &Expr) -> ControlFlow<()> {
        if let Expr::Value(value) = value {
            if let Value::Number(number, _) = &value.value {
                self.numbers.push(number.clone());
            }
        }
        ControlFlow::Continue(())
    }
    fn pre_visit_relation(&mut self, value: &ObjectName) -> ControlFlow<()> {
        self.relations.push(value.to_string());
        ControlFlow::Continue(())
    }
}

#[test]
fn create_table_visits_canonical_definitions_once_in_interleaved_source_order() {
    let table = source_order_table();
    assert!(table.validate_source_order().is_ok());
    let mut visitor = SourceOrderVisitor::default();
    assert!(Visit::visit(&table, &mut visitor).is_continue());
    assert_eq!(visitor.numbers, ["11", "22", "33"]);
    assert_eq!(
        visitor.relations.first().map(String::as_str),
        Some("ordered")
    );

    struct Mutate(Vec<String>);
    impl VisitorMut for Mutate {
        type Break = ();
        fn pre_visit_expr(&mut self, value: &mut Expr) -> ControlFlow<()> {
            if let Expr::Value(value) = value {
                if let Value::Number(number, _) = &mut value.value {
                    self.0.push(number.clone());
                    number.push('0');
                }
            }
            ControlFlow::Continue(())
        }
    }
    let mut changed = table;
    let mut mutate = Mutate(Vec::new());
    assert!(VisitMut::visit(&mut changed, &mut mutate).is_continue());
    assert_eq!(mutate.0, ["11", "22", "33"]);
    let rendered = changed.to_string();
    assert!(rendered.find("110").unwrap() < rendered.find("220").unwrap());
    assert!(rendered.find("220").unwrap() < rendered.find("LIKE parent").unwrap());
    assert!(rendered.find("LIKE parent").unwrap() < rendered.find("330").unwrap());
    let reparsed =
        crate::parser::Parser::parse_sql(&crate::dialect::PostgreSqlDialect {}, &rendered).unwrap();
    assert_eq!(reparsed, [Statement::CreateTable(changed)]);
}

#[test]
fn create_table_bad_source_ordinals_are_rejected_but_direct_visitors_never_panic() {
    let table = source_order_table();
    for elements in [
        vec![
            CreateTableElement::Column(2),
            CreateTableElement::Constraint(0),
            CreateTableElement::Column(1),
        ],
        vec![
            CreateTableElement::Column(0),
            CreateTableElement::Column(0),
            CreateTableElement::Constraint(0),
        ],
        vec![
            CreateTableElement::Column(0),
            CreateTableElement::Constraint(1),
            CreateTableElement::Column(1),
        ],
        vec![
            CreateTableElement::Column(0),
            CreateTableElement::Constraint(0),
            CreateTableElement::Constraint(0),
            CreateTableElement::Column(1),
        ],
        vec![
            CreateTableElement::Column(1),
            CreateTableElement::Constraint(0),
            CreateTableElement::Column(0),
        ],
        vec![CreateTableElement::Column(0), CreateTableElement::Column(1)],
        vec![
            CreateTableElement::Column(0),
            CreateTableElement::Constraint(0),
        ],
        vec![],
    ] {
        let mut invalid = table.clone();
        invalid.elements = elements;
        assert!(invalid.validate_source_order().is_err());
        let mut output = String::new();
        assert!(core::fmt::write(&mut output, format_args!("{invalid}")).is_err());
        assert!(
            output.is_empty(),
            "invalid authority emits no partial fake SQL"
        );
        let mut fallback = SourceOrderVisitor::default();
        assert!(Visit::visit(&invalid, &mut fallback).is_continue());
        assert_eq!(
            fallback.numbers,
            ["11", "33", "22"],
            "fallback visits canonical values, not invalid claimed order"
        );
        struct FallbackMut(Vec<String>);
        impl VisitorMut for FallbackMut {
            type Break = ();
            fn pre_visit_expr(&mut self, value: &mut Expr) -> ControlFlow<()> {
                if let Expr::Value(value) = value {
                    if let Value::Number(number, _) = &mut value.value {
                        self.0.push(number.clone());
                        number.push('0');
                    }
                }
                ControlFlow::Continue(())
            }
        }
        let mut mutate = FallbackMut(Vec::new());
        assert!(VisitMut::visit(&mut invalid, &mut mutate).is_continue());
        assert_eq!(mutate.0, ["11", "33", "22"]);
        let mut changed = SourceOrderVisitor::default();
        assert!(Visit::visit(&invalid, &mut changed).is_continue());
        assert_eq!(changed.numbers, ["110", "330", "220"]);
        assert!(invalid.validate_source_order().is_err());
    }
}

#[test]
fn create_table_source_order_honors_visitor_break_at_the_actual_constraint() {
    struct Stop(Vec<String>);
    impl Visitor for Stop {
        type Break = &'static str;
        fn pre_visit_expr(&mut self, value: &Expr) -> ControlFlow<Self::Break> {
            if let Expr::Value(value) = value {
                if let Value::Number(number, _) = &value.value {
                    if number == "22" {
                        return ControlFlow::Break("constraint");
                    }
                    self.0.push(number.clone());
                }
            }
            ControlFlow::Continue(())
        }
    }
    let mut stop = Stop(Vec::new());
    assert_eq!(
        Visit::visit(&source_order_table(), &mut stop),
        ControlFlow::Break("constraint")
    );
    assert_eq!(stop.0, ["11"]);
}

#[cfg(feature = "serde")]
#[test]
fn create_table_serializes_only_ordinals_and_rejects_corrupt_order_after_detach() {
    let table = source_order_table();
    let encoded = serde_json::to_value(&table).unwrap();
    assert_eq!(encoded["elements"][0]["Column"], serde_json::json!(0));
    assert_eq!(encoded["elements"][1]["Constraint"], serde_json::json!(0));
    let restored: CreateTable = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(restored, table);
    assert!(restored.validate_source_order().is_ok());
    let mut corrupt = encoded;
    corrupt["elements"][0]["Column"] = serde_json::json!(usize::MAX);
    let restored: CreateTable = serde_json::from_value(corrupt).unwrap();
    assert!(restored.validate_source_order().is_err());
    assert!(core::fmt::write(&mut String::new(), format_args!("{restored}")).is_err());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Statement;
    use crate::dialect::GenericDialect;
    use crate::parser::Parser;
    use crate::tokenizer::Tokenizer;

    #[derive(Default)]
    struct TestVisitor {
        visited: Vec<String>,
    }

    impl Visitor for TestVisitor {
        type Break = ();

        /// Invoked for any queries that appear in the AST before visiting children
        fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<Self::Break> {
            self.visited.push(format!("PRE: QUERY: {query}"));
            ControlFlow::Continue(())
        }

        /// Invoked for any queries that appear in the AST after visiting children
        fn post_visit_query(&mut self, query: &Query) -> ControlFlow<Self::Break> {
            self.visited.push(format!("POST: QUERY: {query}"));
            ControlFlow::Continue(())
        }

        fn pre_visit_select(&mut self, select: &Select) -> ControlFlow<Self::Break> {
            self.visited.push(format!("PRE: SELECT: {select}"));
            ControlFlow::Continue(())
        }

        fn post_visit_select(&mut self, select: &Select) -> ControlFlow<Self::Break> {
            self.visited.push(format!("POST: SELECT: {select}"));
            ControlFlow::Continue(())
        }

        fn pre_visit_relation(&mut self, relation: &ObjectName) -> ControlFlow<Self::Break> {
            self.visited.push(format!("PRE: RELATION: {relation}"));
            ControlFlow::Continue(())
        }

        fn post_visit_relation(&mut self, relation: &ObjectName) -> ControlFlow<Self::Break> {
            self.visited.push(format!("POST: RELATION: {relation}"));
            ControlFlow::Continue(())
        }

        fn pre_visit_table_factor(
            &mut self,
            table_factor: &TableFactor,
        ) -> ControlFlow<Self::Break> {
            self.visited
                .push(format!("PRE: TABLE FACTOR: {table_factor}"));
            ControlFlow::Continue(())
        }

        fn post_visit_table_factor(
            &mut self,
            table_factor: &TableFactor,
        ) -> ControlFlow<Self::Break> {
            self.visited
                .push(format!("POST: TABLE FACTOR: {table_factor}"));
            ControlFlow::Continue(())
        }

        fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
            self.visited.push(format!("PRE: EXPR: {expr}"));
            ControlFlow::Continue(())
        }

        fn post_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
            self.visited.push(format!("POST: EXPR: {expr}"));
            ControlFlow::Continue(())
        }

        fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<Self::Break> {
            self.visited.push(format!("PRE: STATEMENT: {statement}"));
            ControlFlow::Continue(())
        }

        fn post_visit_statement(&mut self, statement: &Statement) -> ControlFlow<Self::Break> {
            self.visited.push(format!("POST: STATEMENT: {statement}"));
            ControlFlow::Continue(())
        }
    }

    fn do_visit<V: Visitor<Break = ()>>(sql: &str, visitor: &mut V) -> Statement {
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();
        let s = Parser::new(&dialect)
            .with_tokens(tokens)
            .parse_statement()
            .unwrap();

        let flow = s.visit(visitor);
        assert_eq!(flow, ControlFlow::Continue(()));
        s
    }

    #[test]
    fn test_sql() {
        let tests = vec![
            (
                "SELECT * from table_name as my_table",
                vec![
                    "PRE: STATEMENT: SELECT * FROM table_name AS my_table",
                    "PRE: QUERY: SELECT * FROM table_name AS my_table",
                    "PRE: SELECT: SELECT * FROM table_name AS my_table",
                    "PRE: TABLE FACTOR: table_name AS my_table",
                    "PRE: RELATION: table_name",
                    "POST: RELATION: table_name",
                    "POST: TABLE FACTOR: table_name AS my_table",
                    "POST: SELECT: SELECT * FROM table_name AS my_table",
                    "POST: QUERY: SELECT * FROM table_name AS my_table",
                    "POST: STATEMENT: SELECT * FROM table_name AS my_table",
                ],
            ),
            (
                "SELECT * from t1 join t2 on t1.id = t2.t1_id",
                vec![
                    "PRE: STATEMENT: SELECT * FROM t1 JOIN t2 ON t1.id = t2.t1_id",
                    "PRE: QUERY: SELECT * FROM t1 JOIN t2 ON t1.id = t2.t1_id",
                    "PRE: SELECT: SELECT * FROM t1 JOIN t2 ON t1.id = t2.t1_id",
                    "PRE: TABLE FACTOR: t1",
                    "PRE: RELATION: t1",
                    "POST: RELATION: t1",
                    "POST: TABLE FACTOR: t1",
                    "PRE: TABLE FACTOR: t2",
                    "PRE: RELATION: t2",
                    "POST: RELATION: t2",
                    "POST: TABLE FACTOR: t2",
                    "PRE: EXPR: t1.id = t2.t1_id",
                    "PRE: EXPR: t1.id",
                    "POST: EXPR: t1.id",
                    "PRE: EXPR: t2.t1_id",
                    "POST: EXPR: t2.t1_id",
                    "POST: EXPR: t1.id = t2.t1_id",
                    "POST: SELECT: SELECT * FROM t1 JOIN t2 ON t1.id = t2.t1_id",
                    "POST: QUERY: SELECT * FROM t1 JOIN t2 ON t1.id = t2.t1_id",
                    "POST: STATEMENT: SELECT * FROM t1 JOIN t2 ON t1.id = t2.t1_id",
                ],
            ),
            (
                "SELECT * from t1 where EXISTS(SELECT column from t2)",
                vec![
                    "PRE: STATEMENT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                    "PRE: QUERY: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                    "PRE: SELECT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                    "PRE: TABLE FACTOR: t1",
                    "PRE: RELATION: t1",
                    "POST: RELATION: t1",
                    "POST: TABLE FACTOR: t1",
                    "PRE: EXPR: EXISTS (SELECT column FROM t2)",
                    "PRE: QUERY: SELECT column FROM t2",
                    "PRE: SELECT: SELECT column FROM t2",
                    "PRE: EXPR: column",
                    "POST: EXPR: column",
                    "PRE: TABLE FACTOR: t2",
                    "PRE: RELATION: t2",
                    "POST: RELATION: t2",
                    "POST: TABLE FACTOR: t2",
                    "POST: SELECT: SELECT column FROM t2",
                    "POST: QUERY: SELECT column FROM t2",
                    "POST: EXPR: EXISTS (SELECT column FROM t2)",
                    "POST: SELECT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                    "POST: QUERY: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                    "POST: STATEMENT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                ],
            ),
            (
                "SELECT * from t1 where EXISTS(SELECT column from t2)",
                vec![
                    "PRE: STATEMENT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                    "PRE: QUERY: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                    "PRE: SELECT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                    "PRE: TABLE FACTOR: t1",
                    "PRE: RELATION: t1",
                    "POST: RELATION: t1",
                    "POST: TABLE FACTOR: t1",
                    "PRE: EXPR: EXISTS (SELECT column FROM t2)",
                    "PRE: QUERY: SELECT column FROM t2",
                    "PRE: SELECT: SELECT column FROM t2",
                    "PRE: EXPR: column",
                    "POST: EXPR: column",
                    "PRE: TABLE FACTOR: t2",
                    "PRE: RELATION: t2",
                    "POST: RELATION: t2",
                    "POST: TABLE FACTOR: t2",
                    "POST: SELECT: SELECT column FROM t2",
                    "POST: QUERY: SELECT column FROM t2",
                    "POST: EXPR: EXISTS (SELECT column FROM t2)",
                    "POST: SELECT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                    "POST: QUERY: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                    "POST: STATEMENT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                ],
            ),
            (
                "SELECT * from t1 where EXISTS(SELECT column from t2) UNION SELECT * from t3",
                vec![
                    "PRE: STATEMENT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2) UNION SELECT * FROM t3",
                    "PRE: QUERY: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2) UNION SELECT * FROM t3",
                    "PRE: SELECT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                    "PRE: TABLE FACTOR: t1",
                    "PRE: RELATION: t1",
                    "POST: RELATION: t1",
                    "POST: TABLE FACTOR: t1",
                    "PRE: EXPR: EXISTS (SELECT column FROM t2)",
                    "PRE: QUERY: SELECT column FROM t2",
                    "PRE: SELECT: SELECT column FROM t2",
                    "PRE: EXPR: column",
                    "POST: EXPR: column",
                    "PRE: TABLE FACTOR: t2",
                    "PRE: RELATION: t2",
                    "POST: RELATION: t2",
                    "POST: TABLE FACTOR: t2",
                    "POST: SELECT: SELECT column FROM t2",
                    "POST: QUERY: SELECT column FROM t2",
                    "POST: EXPR: EXISTS (SELECT column FROM t2)",
                    "POST: SELECT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
                    "PRE: SELECT: SELECT * FROM t3",
                    "PRE: TABLE FACTOR: t3",
                    "PRE: RELATION: t3",
                    "POST: RELATION: t3",
                    "POST: TABLE FACTOR: t3",
                    "POST: SELECT: SELECT * FROM t3",
                    "POST: QUERY: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2) UNION SELECT * FROM t3",
                    "POST: STATEMENT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2) UNION SELECT * FROM t3",
                ],
            ),
            (
                concat!(
                    "SELECT * FROM monthly_sales ",
                    "PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d) ",
                    "ORDER BY EMPID"
                ),
                vec![
                    "PRE: STATEMENT: SELECT * FROM monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d) ORDER BY EMPID",
                    "PRE: QUERY: SELECT * FROM monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d) ORDER BY EMPID",
                    "PRE: SELECT: SELECT * FROM monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d)",
                    "PRE: TABLE FACTOR: monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d)",
                    "PRE: TABLE FACTOR: monthly_sales",
                    "PRE: RELATION: monthly_sales",
                    "POST: RELATION: monthly_sales",
                    "POST: TABLE FACTOR: monthly_sales",
                    "PRE: EXPR: SUM(a.amount)",
                    "PRE: EXPR: a.amount",
                    "POST: EXPR: a.amount",
                    "POST: EXPR: SUM(a.amount)",
                    "PRE: EXPR: a.MONTH",
                    "POST: EXPR: a.MONTH",
                    "PRE: EXPR: 'JAN'",
                    "POST: EXPR: 'JAN'",
                    "PRE: EXPR: 'FEB'",
                    "POST: EXPR: 'FEB'",
                    "PRE: EXPR: 'MAR'",
                    "POST: EXPR: 'MAR'",
                    "PRE: EXPR: 'APR'",
                    "POST: EXPR: 'APR'",
                    "POST: TABLE FACTOR: monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d)",
                    "POST: SELECT: SELECT * FROM monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d)",
                    "PRE: EXPR: EMPID",
                    "POST: EXPR: EMPID",
                    "POST: QUERY: SELECT * FROM monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d) ORDER BY EMPID",
                    "POST: STATEMENT: SELECT * FROM monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d) ORDER BY EMPID",
                ],
            ),
            (
                "SHOW COLUMNS FROM t1",
                vec![
                    "PRE: STATEMENT: SHOW COLUMNS FROM t1",
                    "PRE: RELATION: t1",
                    "POST: RELATION: t1",
                    "POST: STATEMENT: SHOW COLUMNS FROM t1",
                ],
            ),
        ];
        for (sql, expected) in tests {
            let mut visitor = TestVisitor::default();
            let _ = do_visit(sql, &mut visitor);
            let actual: Vec<_> = visitor.visited.iter().map(|x| x.as_str()).collect();
            assert_eq!(actual, expected)
        }
    }

    struct QuickVisitor; // [`TestVisitor`] is too slow to iterate over thousands of nodes

    impl Visitor for QuickVisitor {
        type Break = ();
    }

    #[test]
    fn overflow() {
        let cond = (0..1000)
            .map(|n| format!("X = {n}"))
            .collect::<Vec<_>>()
            .join(" OR ");
        let sql = format!("SELECT x where {cond}");

        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, sql.as_str()).tokenize().unwrap();
        let s = Parser::new(&dialect)
            .with_tokens(tokens)
            .parse_statement()
            .unwrap();

        let mut visitor = QuickVisitor {};
        let flow = s.visit(&mut visitor);
        assert_eq!(flow, ControlFlow::Continue(()));
    }
}

#[cfg(test)]
mod visit_mut_tests {
    use crate::ast::{Statement, Value, ValueWithSpan, VisitMut, VisitorMut};
    use crate::dialect::GenericDialect;
    use crate::parser::Parser;
    use crate::tokenizer::Tokenizer;
    use core::ops::ControlFlow;

    #[derive(Default)]
    struct MutatorVisitor {
        index: u64,
    }

    impl VisitorMut for MutatorVisitor {
        type Break = ();

        fn pre_visit_value(&mut self, value: &mut ValueWithSpan) -> ControlFlow<Self::Break> {
            self.index += 1;
            value.value = Value::SingleQuotedString(format!("REDACTED_{}", self.index));
            ControlFlow::Continue(())
        }

        fn post_visit_value(&mut self, _value: &mut ValueWithSpan) -> ControlFlow<Self::Break> {
            ControlFlow::Continue(())
        }
    }

    fn do_visit_mut<V: VisitorMut<Break = ()>>(sql: &str, visitor: &mut V) -> Statement {
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();
        let mut s = Parser::new(&dialect)
            .with_tokens(tokens)
            .parse_statement()
            .unwrap();

        let flow = s.visit(visitor);
        assert_eq!(flow, ControlFlow::Continue(()));
        s
    }

    #[test]
    fn test_value_redact() {
        let tests = vec![(
            concat!(
                "SELECT * FROM monthly_sales ",
                "PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d) ",
                "ORDER BY EMPID"
            ),
            concat!(
                "SELECT * FROM monthly_sales ",
                "PIVOT(SUM(a.amount) FOR a.MONTH IN ('REDACTED_1', 'REDACTED_2', 'REDACTED_3', 'REDACTED_4')) AS p (c, d) ",
                "ORDER BY EMPID"
            ),
        )];

        for (sql, expected) in tests {
            let mut visitor = MutatorVisitor::default();
            let mutated = do_visit_mut(sql, &mut visitor);
            assert_eq!(mutated.to_string(), expected)
        }
    }
}
