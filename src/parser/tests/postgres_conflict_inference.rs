use crate::{
    ast::{ConflictTarget, Expr, OnInsert, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};

#[test]
fn inference_keeps_expression_collation_class_predicate_and_round_trip() {
    for sql in [
        "INSERT INTO t VALUES (1,'Apple') ON CONFLICT (lower(fruit), key, lower(fruit)) WHERE key < 5 DO UPDATE SET fruit = excluded.fruit",
        "INSERT INTO t VALUES (1,'Apple') ON CONFLICT (fruit COLLATE \"C\" text_pattern_ops, key) WHERE fruit LIKE '%berry' DO NOTHING",
        "INSERT INTO t VALUES (1) ON CONFLICT ((coalesce(a, 0))) WHERE coalesce(a, 0) > 5 DO NOTHING",
    ] {
        let parsed = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
        let [Statement::Insert(insert)] = parsed.as_slice() else {
            panic!("INSERT");
        };
        let Some(OnInsert::OnConflict(conflict)) = &insert.on else {
            panic!("ON CONFLICT");
        };
        let Some(ConflictTarget::Inference {
            elements,
            predicate,
        }) = &conflict.conflict_target
        else {
            panic!("inference");
        };
        assert!(!elements.is_empty());
        assert!(predicate.is_some());
        let rendered = parsed[0].to_string();
        let reparsed = Parser::parse_sql(&PostgreSqlDialect {}, &rendered).unwrap();
        assert_eq!(reparsed[0].to_string(), rendered);
    }
}

#[test]
fn inference_requires_parentheses_for_non_function_expressions() {
    for sql in [
        "INSERT INTO t VALUES(1) ON CONFLICT (a+1) DO NOTHING",
        "INSERT INTO t VALUES(1) ON CONFLICT (t.a) DO NOTHING",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, sql).is_err(),
            "{sql}"
        );
    }
    for sql in [
        "INSERT INTO t VALUES(1) ON CONFLICT ((a+1)) DO NOTHING",
        "INSERT INTO t VALUES(1) ON CONFLICT ((t.a)) DO NOTHING",
        "INSERT INTO t VALUES(1) ON CONFLICT (a DESC NULLS LAST) DO NOTHING",
        "INSERT INTO t VALUES(1) ON CONFLICT ON CONSTRAINT pkey DO NOTHING",
    ] {
        Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
    }
}

#[test]
fn insert_targets_retain_each_actual_field_and_subscript_step() {
    let statements = Parser::parse_sql(&PostgreSqlDialect {}, "INSERT INTO t (f2[1], f2[2], f3.if1, f3.if2[1]) VALUES(1,2,3,'foo') ON CONFLICT ((coalesce(a,0))) DO NOTHING").unwrap();
    let [Statement::Insert(insert)] = statements.as_slice() else {
        panic!("INSERT");
    };
    assert_eq!(
        insert
            .columns
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["f2[1]", "f2[2]", "f3.if1", "f3.if2[1]"]
    );
    assert!(matches!(&insert.columns[2], Expr::CompoundIdentifier(parts) if parts.len()==2));
    assert!(
        matches!(&insert.columns[3], Expr::CompoundFieldAccess { access_chain, .. } if access_chain.len()==2)
    );
}

#[test]
fn insert_compiler_receipt_is_not_public_syntax_or_source_span() {
    use crate::ast::Spanned;
    let sql = "INSERT INTO t(a[2].n,b) VALUES(7,'x') ON CONFLICT ((coalesce(k,0))) WHERE k > 1 DO NOTHING RETURNING b";
    let mut statements = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
    let [Statement::Insert(insert)] = statements.as_mut_slice() else {
        panic!("INSERT");
    };
    assert!(insert.postgres_insert_bindings.is_none());
    let source_span = insert.span();
    let displayed = insert.to_string();
    insert.postgres_insert_bindings =
        Some("selected native Query receipt; never SQL syntax".into());
    assert_eq!(insert.span(), source_span);
    assert_eq!(insert.to_string(), displayed);
    let reparsed = Parser::parse_sql(&PostgreSqlDialect {}, &displayed).unwrap();
    let [Statement::Insert(reparsed)] = reparsed.as_slice() else {
        panic!("INSERT");
    };
    assert!(reparsed.postgres_insert_bindings.is_none());
    assert_eq!(reparsed.to_string(), displayed);
    // The compiler receipt participates in the complete AST's serde contract.
    // Serde's production feature does not require a new JSON dependency.
    #[cfg(feature = "serde")]
    {
        fn serialized_ast<T: serde::Serialize + for<'a> serde::Deserialize<'a>>() {}
        serialized_ast::<crate::ast::Insert>();
    }
}

#[cfg(feature = "visitor")]
#[test]
fn insert_compiler_receipt_does_not_add_or_revisit_sql_expressions() {
    use crate::ast::visit_expressions;
    use core::ops::ControlFlow;
    let mut statements = Parser::parse_sql(
        &PostgreSqlDialect {},
        "INSERT INTO t(a[2]) VALUES(7) ON CONFLICT (a) DO UPDATE SET a = excluded.a RETURNING a",
    )
    .unwrap();
    let expressions = |statement: &Statement| {
        let mut selected = Vec::new();
        let _ = visit_expressions(statement, |expression| {
            selected.push(expression.to_string());
            ControlFlow::<()>::Continue(())
        });
        selected
    };
    let before = expressions(&statements[0]);
    let [Statement::Insert(insert)] = statements.as_mut_slice() else {
        panic!("INSERT");
    };
    insert.postgres_insert_bindings = Some("a[$99] SELECT volatile_input()".into());
    assert_eq!(expressions(&statements[0]), before);
}
