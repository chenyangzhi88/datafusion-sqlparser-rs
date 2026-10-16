use crate::ast::{
    Expr, JsonTableColumn, JsonTableColumnErrorHandling, SetExpr, Statement, TableFactor, Value,
    Visit, VisitMut, Visitor, VisitorMut,
};
use core::ops::ControlFlow;

#[test]
fn native_scoped_range_facts_share_qb_payload_and_exclude_lease_from_logical_identity() {
    use crate::ast::{MySqlHintDirective, OptimizerHintStyle, SqlSourceAdmission};
    use std::hash::{Hash, Hasher};
    use std::sync::Arc;
    let payload = Arc::new(vec![
        MySqlHintDirective::QueryBlockName("Qé".into()),
        MySqlHintDirective::NoRangeOptimization {
            query_block: Some("qé".into()),
            table: "r".into(),
            indexes: vec!["ix".into()],
        },
    ]);
    let style = OptimizerHintStyle::MySqlNoRangeOptimization {
        query_block_name: Some((Arc::clone(&payload), 0)),
        source_admissions: (Some(SqlSourceAdmission::new(Arc::new(()))), None),
        table: "r".into(),
        indexes: vec!["ix".into()],
        resolve_conditions: true,
    };
    let mut detached = style.clone();
    let OptimizerHintStyle::MySqlNoRangeOptimization {
        source_admissions,
        query_block_name,
        ..
    } = &mut detached
    else {
        panic!("range")
    };
    *source_admissions = (None, None);
    assert!(Arc::ptr_eq(&query_block_name.as_ref().unwrap().0, &payload));
    assert_eq!(
        style, detached,
        "physical allocation custody is not a second logical name"
    );
    let hash = |value: &OptimizerHintStyle| {
        let mut state = std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut state);
        state.finish()
    };
    assert_eq!(hash(&style), hash(&detached));
    assert_eq!(
        style.partial_cmp(&detached),
        Some(std::cmp::Ordering::Equal)
    );
    struct Leaves;
    impl VisitorMut for Leaves {
        type Break = ();
    }
    let _ = VisitMut::visit(&mut detached, &mut Leaves);
    assert!(
        Arc::ptr_eq(
            match &detached {
                OptimizerHintStyle::MySqlNoRangeOptimization {
                    query_block_name: Some((owner, _)),
                    ..
                } => owner,
                _ => panic!("range"),
            },
            &payload,
        ),
        "ordinary AST visitors cannot rebind immutable native hint declarations"
    );
    let mut different = detached.clone();
    let OptimizerHintStyle::MySqlNoRangeOptimization {
        query_block_name, ..
    } = &mut different
    else {
        panic!("range")
    };
    *query_block_name = Some((
        Arc::new(vec![MySqlHintDirective::QueryBlockName("other".into())]),
        0,
    ));
    assert_ne!(style, different);
}

#[test]
fn json_table_native_declaration_attributes_order_and_literal_grammar_are_not_erased() {
    let dialect = crate::dialect::MySqlDialect {};
    let sql = "SELECT * FROM JSON_TABLE('{}','$' COLUMNS(v VARCHAR(10) CHARSET utf8mb4 COLLATE utf8mb4_bin PATH '$' ERROR ON ERROR NULL ON EMPTY)) jt";
    let statements = crate::parser::Parser::parse_sql(&dialect, sql).unwrap();
    let Statement::Query(query) = &statements[0] else {
        panic!("query");
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("select");
    };
    let TableFactor::JsonTable { columns, .. } = &select.from[0].relation else {
        panic!("table");
    };
    let JsonTableColumn::Named(column) = &columns[0] else {
        panic!("column");
    };
    assert_eq!(column.character_set.as_ref().unwrap().value, "utf8mb4");
    assert_eq!(
        column.collation.as_ref().unwrap().to_string(),
        "utf8mb4_bin"
    );
    assert!(column.on_error_before_empty);
    assert!(column.to_string().contains("ERROR ON ERROR NULL ON EMPTY"));
    for clause in [
        "DEFAULT NULL ON EMPTY",
        "DEFAULT NULL ON ERROR",
        "NULL ON EMPTY NULL ON EMPTY",
        "ERROR ON ERROR ERROR ON ERROR",
        "DEFAULT CONCAT('0','0') ON EMPTY",
    ] {
        assert!(
            crate::parser::Parser::parse_sql(
                &dialect,
                &format!(
                    "SELECT * FROM JSON_TABLE('{{}}','$' COLUMNS(v INT PATH '$' {clause})) jt"
                )
            )
            .is_err(),
            "{clause}"
        );
    }
    for clause in [
        "DEFAULT 0 ON EMPTY",
        "DEFAULT -1 ON ERROR",
        "DEFAULT DATE'2020-01-01' ON EMPTY",
    ] {
        assert!(
            crate::parser::Parser::parse_sql(
                &dialect,
                &format!(
                    "SELECT * FROM JSON_TABLE('{{}}','$' COLUMNS(v INT PATH '$' {clause})) jt"
                )
            )
            .is_ok(),
            "grammar accepts unsupported nonstring literal; setup must reject: {clause}"
        );
    }
}

#[test]
fn json_table_literal_slots_visit_the_same_expr_owner_and_preserve_their_source_spans() {
    let sql = "SELECT * FROM JSON_TABLE('[]','$[*]' COLUMNS(v INT PATH '$.v' DEFAULT '7' ON EMPTY DEFAULT '8' ON ERROR,NESTED PATH '$.n[*]' COLUMNS(x INT PATH '$'))) jt";
    let mut statements =
        crate::parser::Parser::parse_sql(&crate::dialect::MySqlDialect {}, sql).unwrap();
    struct Literals(Vec<crate::tokenizer::Span>);
    impl Visitor for Literals {
        type Break = ();
        fn pre_visit_expr(&mut self, value: &Expr) -> ControlFlow<()> {
            if let Expr::Value(value) = value {
                self.0.push(value.span);
            }
            ControlFlow::Continue(())
        }
    }
    let mut literals = Literals(Vec::new());
    let _ = statements.visit(&mut literals);
    assert_eq!(
        literals.0.len(),
        7,
        "source, row path, named path, two defaults, nested path, nested named path are all actual Exprs"
    );
    assert!(
        literals
            .0
            .iter()
            .all(|span| span.start.line == 1 && span.end.column > span.start.column)
    );
    struct Bound;
    impl VisitorMut for Bound {
        type Break = ();
        fn post_visit_expr(&mut self, value: &mut Expr) -> ControlFlow<()> {
            if matches!(value, Expr::Value(_)) {
                *value = Expr::Nested(Box::new(value.clone()));
            }
            ControlFlow::Continue(())
        }
    }
    let _ = VisitMut::visit(&mut statements, &mut Bound);
    let Statement::Query(query) = &statements[0] else {
        panic!("query");
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("select");
    };
    let TableFactor::JsonTable {
        json_path, columns, ..
    } = &select.from[0].relation
    else {
        panic!("JSON_TABLE");
    };
    assert!(matches!(json_path, Expr::Nested(_)));
    let JsonTableColumn::Named(column) = &columns[0] else {
        panic!("named");
    };
    assert!(matches!(column.path, Expr::Nested(_)));
    assert!(matches!(
        &column.on_empty,
        Some(JsonTableColumnErrorHandling::Default(Expr::Nested(_)))
    ));
    let mut bound = Literals(Vec::new());
    let _ = statements.visit(&mut bound);
    assert_eq!(
        bound.0, literals.0,
        "binding preserves original literal spans and positional order"
    );
    assert!(
        crate::parser::Parser::parse_sql(
            &crate::dialect::MySqlDialect {},
            "SELECT * FROM JSON_TABLE('[]',CONCAT('$','[*]') COLUMNS(v INT PATH '$')) jt"
        )
        .is_err(),
        "Expr storage does not admit new grammar"
    );
    assert!(
        crate::parser::Parser::parse_sql(
            &crate::dialect::MySqlDialect {},
            "SELECT * FROM JSON_TABLE('[]','$[*]' COLUMNS(v INT PATH CONCAT('$','.v'))) jt"
        )
        .is_err()
    );
    assert!(
        matches!(column.on_error.as_ref(),Some(JsonTableColumnErrorHandling::Default(Expr::Nested(value))) if matches!(value.as_ref(),Expr::Value(value) if matches!(&value.value,Value::SingleQuotedString(value) if value=="8")))
    );
}
#[test]
fn derived_hint_bound_source_pending_fact_clone_hash_and_payload_are_logical_not_admission() {
    use crate::ast::{MySqlHintDirective,OptimizerHintStyle,SqlSourceAdmission,SqlSourceOwner};
    use std::{sync::Arc,hash::{Hash,Hasher}};
    let owner = SqlSourceOwner::adopt(b"/*+ DERIVED_CONDITION_PUSHDOWN(@q d@other) */".to_vec(),
        SqlSourceAdmission::new(Arc::new(())));
    let source = owner.occurrence(0..owner.bytes().len()).unwrap();
    let directives = Arc::new(vec![MySqlHintDirective::DerivedConditionPushdown {
        query_block:Some("q".into()),tables:vec![("d".into(),Some("other".into()))],enabled:true,
    }]);
    let style = OptimizerHintStyle::MySqlBoundSource {
        source:source.clone(),text:source,client_collation:33,ansi_quotes:false,
        directives:Arc::clone(&directives),resolve_conditions:true,syntax:Arc::new(vec![]),
        admission:Some(SqlSourceAdmission::new(Arc::new(()))),unsupported:None,
    };
    let clone = style.clone();
    let OptimizerHintStyle::MySqlBoundSource {directives:borrowed,..} = &clone else {panic!("bound")};
    assert!(Arc::ptr_eq(&directives,borrowed));
    assert_eq!(clone,style);
    let mut completed = clone;
    let OptimizerHintStyle::MySqlBoundSource {resolve_conditions,admission,..} = &mut completed else {panic!("bound")};
    *resolve_conditions=false;
    *admission=None;
    assert_ne!(completed,style,"one pending logical obligation belongs to the original program");
    fn hash(value:&OptimizerHintStyle)->u64 {
        let mut hasher=std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut hasher);hasher.finish()
    }
    assert_ne!(hash(&completed),hash(&style));
    #[cfg(feature="serde")]
    {
        let detached: OptimizerHintStyle = serde_json::from_str(&serde_json::to_string(&style).unwrap()).unwrap();
        let OptimizerHintStyle::MySqlBoundSource {source,admission,resolve_conditions,..}=detached else {panic!("bound")};
        assert!(source.bytes().is_err());
        assert!(admission.is_none());
        assert!(resolve_conditions,"serialization copies logical facts but cannot reissue Source rights");
    }
}
