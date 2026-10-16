use crate::ast::{AlterTableOperation, Ident, Spanned, Statement};
use crate::dialect::MySqlDialect;
use crate::parser::Parser;
use crate::tokenizer::{Location, Span};

fn constraint_name(operation: &AlterTableOperation) -> &Ident {
    match operation {
        AlterTableOperation::DropCheck { name }
        | AlterTableOperation::AlterCheck { name, enforced: _ }
        | AlterTableOperation::AlterConstraintEnforcement { name, enforced: _ } => name,
        other => panic!("expected a typed CHECK operation, got {other:?}"),
    }
}

#[test]
fn check_operations_span_the_actual_identifier_including_quotes_and_escapes() {
    for (source_prefix, display_prefix, source_name, suffix) in [
        (
            "ALTER TABLE t DROP CHECK ",
            "ALTER TABLE t DROP CHECK ",
            "ck",
            "",
        ),
        (
            "ALTER TABLE t ALTER CHECK ",
            "ALTER TABLE t ALTER CHECK ",
            "`检查`",
            " ENFORCED",
        ),
        (
            "ALTER TABLE t ALTER CHECK /* source */ ",
            "ALTER TABLE t ALTER CHECK ",
            "`检``查`",
            " NOT ENFORCED",
        ),
        (
            "ALTER TABLE t ALTER CONSTRAINT ",
            "ALTER TABLE t ALTER CONSTRAINT ",
            "generic",
            " ENFORCED",
        ),
        (
            "ALTER TABLE t ALTER CONSTRAINT ",
            "ALTER TABLE t ALTER CONSTRAINT ",
            "`通用`",
            " NOT ENFORCED",
        ),
    ] {
        let sql = format!("{source_prefix}{source_name}{suffix}");
        let statements = Parser::parse_sql(&MySqlDialect {}, &sql).unwrap();
        let Statement::AlterTable(alter) = &statements[0] else {
            panic!("ALTER TABLE")
        };
        assert_eq!(alter.operations.len(), 1);
        let operation = &alter.operations[0];
        let start = source_prefix.chars().count() as u64 + 1;
        let expected = Span::new(
            Location::new(1, start),
            Location::new(1, start + source_name.chars().count() as u64),
        );
        assert_ne!(expected, Span::empty());
        assert_eq!(operation.span(), expected, "{sql}");
        assert_eq!(constraint_name(operation).span, expected);
        assert_eq!(
            statements[0].to_string(),
            format!("{display_prefix}{source_name}{suffix}")
        );
    }
}

#[test]
fn multiline_check_operations_keep_each_original_location_and_enforcement_state() {
    let sql = r#"ALTER TABLE `表`
  DROP CHECK `检查`,
  ALTER CHECK `门限` ENFORCED,
  ALTER CONSTRAINT plain NOT ENFORCED"#;
    let statements = Parser::parse_sql(&MySqlDialect {}, sql).unwrap();
    let Statement::AlterTable(alter) = &statements[0] else {
        panic!("ALTER TABLE")
    };
    assert_eq!(alter.operations.len(), 3);
    let expected = [
        Span::new(Location::new(2, 14), Location::new(2, 18)),
        Span::new(Location::new(3, 15), Location::new(3, 19)),
        Span::new(Location::new(4, 20), Location::new(4, 25)),
    ];
    for (operation, expected) in alter.operations.iter().zip(expected) {
        assert_eq!(operation.span(), expected);
        assert_eq!(constraint_name(operation).span, expected);
    }
    assert!(
        matches!(&alter.operations[0], AlterTableOperation::DropCheck { name } if name.value == "检查")
    );
    assert!(
        matches!(&alter.operations[1], AlterTableOperation::AlterCheck { name, enforced: true } if name.value == "门限")
    );
    assert!(
        matches!(&alter.operations[2], AlterTableOperation::AlterConstraintEnforcement { name, enforced: false } if name.value == "plain")
    );
    let whole = statements[0].span();
    assert_eq!(whole.start, alter.name.span().start);
    assert!(whole.end >= expected[2].end);
}

#[cfg(feature = "serde")]
#[test]
fn check_operation_serde_roundtrip_preserves_type_flags_and_real_spans() {
    let sql = "ALTER TABLE t DROP CHECK `检查`, ALTER CHECK enabled ENFORCED, ALTER CONSTRAINT disabled NOT ENFORCED";
    let statements = Parser::parse_sql(&MySqlDialect {}, sql).unwrap();
    let encoded = serde_json::to_vec(&statements).unwrap();
    let decoded: Vec<Statement> = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, statements);
    let Statement::AlterTable(original) = &statements[0] else {
        panic!("ALTER TABLE")
    };
    let Statement::AlterTable(restored) = &decoded[0] else {
        panic!("ALTER TABLE")
    };
    // Ident equality ignores spans, so verify them explicitly after serde.
    for (before, after) in original.operations.iter().zip(&restored.operations) {
        assert_eq!(constraint_name(after).span, constraint_name(before).span);
        assert_eq!(after.span(), before.span());
        assert_eq!(after.to_string(), before.to_string());
    }
}

#[test]
fn engine_attribute_ast_spans_display_and_visitor_use_the_same_ordered_declaration_payload() {
    use crate::ast::{ColumnOption, IndexOption};
    for secondary in [false, true] {
        let value: crate::ast::ValueWithSpan =
            crate::ast::Value::SingleQuotedString("{\"s\":\"a'b\"}".to_owned()).into();
        let column = ColumnOption::EngineAttribute {
            secondary,
            value: value.clone(),
        };
        let index = IndexOption::EngineAttribute { secondary, value };
        assert_eq!(column.span(), Span::empty());
        assert_eq!(column.to_string(), index.to_string());
        assert!(column.to_string().contains("a''b"));
        #[cfg(feature = "serde")]
        {
            assert_eq!(
                serde_json::from_str::<ColumnOption>(&serde_json::to_string(&column).unwrap())
                    .unwrap(),
                column
            );
            assert_eq!(
                serde_json::from_str::<IndexOption>(&serde_json::to_string(&index).unwrap())
                    .unwrap(),
                index
            );
        }
        #[cfg(feature = "visitor")]
        {
            struct NoExpressions(usize);
            impl crate::ast::Visitor for NoExpressions {
                type Break = ();
                fn pre_visit_expr(&mut self, _: &crate::ast::Expr) -> core::ops::ControlFlow<()> {
                    self.0 += 1;
                    core::ops::ControlFlow::Continue(())
                }
            }
            let mut visitor = NoExpressions(0);
            let _ = crate::ast::Visit::visit(&column, &mut visitor);
            let _ = crate::ast::Visit::visit(&index, &mut visitor);
            assert_eq!(visitor.0, 0, "metadata strings cannot execute as VALUE");
        }
    }
}
