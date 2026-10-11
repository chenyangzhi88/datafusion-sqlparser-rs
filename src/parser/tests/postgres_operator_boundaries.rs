use crate::{
    ast::{CommentObject, OperatorOption, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};

#[test]
fn operator_comment_operand_slots_unknown_attributes_and_setof_round_trip() {
    for source in [
        "COMMENT ON OPERATOR public.####(NONE,integer) IS 'unary'",
        "COMMENT ON OPERATOR ####(integer,NONE) IS 'postfix'",
        "CREATE OPERATOR === (leftarg=SETOF bigint,rightarg=integer,function=some_fn,sort1,invalid_att=integer,\"Commutator\" = ===,\"Hashes\")",
    ] {
        let parsed = Parser::parse_sql(&PostgreSqlDialect {}, source).unwrap();
        assert_eq!(
            Parser::parse_sql(&PostgreSqlDialect {}, &parsed[0].to_string()).unwrap(),
            parsed
        );
        match &parsed[0] {
            Statement::Comment {
                object_type,
                routine_arguments,
                ..
            } => {
                assert_eq!(*object_type, CommentObject::Operator);
                assert_eq!(routine_arguments.as_ref().unwrap().len(), 2);
            }
            Statement::CreateOperator(create) => {
                assert!(create.left_arg_setof);
                assert!(
                    create
                        .options
                        .iter()
                        .any(|option| matches!(option, OperatorOption::Unrecognized { .. }))
                );
            }
            _ => panic!("unexpected statement"),
        }
    }
}

#[test]
fn reserved_named_argument_arrow_is_rejected_as_an_operator_name() {
    for source in [
        "CREATE OPERATOR =>(rightarg=integer,function=some_fn)",
        "CREATE OPERATOR public.=>(rightarg=integer,function=some_fn)",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, source).is_err(),
            "{source}"
        );
    }
}

#[test]
fn distinct_from_does_not_consume_following_postgres_boolean_operators() {
    use crate::ast::{BinaryOperator, Expr};
    for (source, operator, negate_first) in [
        (
            "a IS DISTINCT FROM b OR c IS DISTINCT FROM d",
            BinaryOperator::Or,
            false,
        ),
        (
            "a IS NOT DISTINCT FROM b AND c IS DISTINCT FROM d",
            BinaryOperator::And,
            true,
        ),
    ] {
        let expression = Parser::new(&PostgreSqlDialect {})
            .try_with_sql(source)
            .unwrap()
            .parse_expr()
            .unwrap();
        let Expr::BinaryOp { left, op, right } = expression else {
            panic!("{source}")
        };
        assert_eq!(op, operator);
        let (a, b) = match *left {
            Expr::IsDistinctFrom(a, b) if !negate_first => (a, b),
            Expr::IsNotDistinctFrom(a, b) if negate_first => (a, b),
            expression => panic!("incorrect left predicate: {expression:?}"),
        };
        assert_eq!(*a, Expr::Identifier(crate::ast::Ident::new("a")));
        assert_eq!(*b, Expr::Identifier(crate::ast::Ident::new("b")));
        let Expr::IsDistinctFrom(c, d) = *right else {
            panic!("incorrect right predicate")
        };
        assert_eq!(*c, Expr::Identifier(crate::ast::Ident::new("c")));
        assert_eq!(*d, Expr::Identifier(crate::ast::Ident::new("d")));
    }
}

#[test]
fn distinct_from_postgres_rhs_preserves_higher_precedence_and_explicit_grouping() {
    use crate::ast::{BinaryOperator, Expr};
    for (source, expected) in [
        ("a IS DISTINCT FROM b + c", BinaryOperator::Plus),
        ("a IS NOT DISTINCT FROM b = c", BinaryOperator::Eq),
    ] {
        let expression = Parser::new(&PostgreSqlDialect {})
            .try_with_sql(source)
            .unwrap()
            .parse_expr()
            .unwrap();
        let rhs = match expression {
            Expr::IsDistinctFrom(_, rhs) | Expr::IsNotDistinctFrom(_, rhs) => rhs,
            expression => panic!("{expression:?}"),
        };
        let Expr::BinaryOp { op, .. } = *rhs else {
            panic!("missing RHS expression")
        };
        assert_eq!(op, expected);
    }
    let expression = Parser::new(&PostgreSqlDialect {})
        .try_with_sql("a IS DISTINCT FROM (b OR c)")
        .unwrap()
        .parse_expr()
        .unwrap();
    let Expr::IsDistinctFrom(_, rhs) = expression else {
        panic!("missing predicate")
    };
    assert!(matches!(*rhs, Expr::Nested(_)));
}

#[test]
fn distinct_from_other_dialects_keep_existing_rhs_contract() {
    use crate::{
        ast::{BinaryOperator, Expr},
        dialect::{Dialect, GenericDialect, MySqlDialect},
    };
    for dialect in [&GenericDialect {} as &dyn Dialect, &MySqlDialect {}] {
        let expression = Parser::new(dialect)
            .try_with_sql("a IS DISTINCT FROM b OR c")
            .unwrap()
            .parse_expr()
            .unwrap();
        let Expr::IsDistinctFrom(_, rhs) = expression else {
            panic!("missing predicate")
        };
        assert!(matches!(
            *rhs,
            Expr::BinaryOp {
                op: BinaryOperator::Or,
                ..
            }
        ));
    }
}

#[test]
fn postgres_named_arguments_preserve_identifier_and_operator_before_expression_parsing() {
    use crate::ast::{Expr, FunctionArg, FunctionArgOperator, FunctionArguments};
    for source in [
        "make_interval(years := 2, secs => 3.5)",
        "make_interval(\"secs\" := 'inf')",
    ] {
        let expr = Parser::new(&PostgreSqlDialect {})
            .try_with_sql(source)
            .unwrap()
            .parse_expr()
            .unwrap();
        let Expr::Function(function) = &expr else {
            panic!("{expr:?}")
        };
        let FunctionArguments::List(args) = &function.args else {
            panic!("{function:?}")
        };
        assert!(args.args.iter().all(|arg| matches!(
            arg,
            FunctionArg::Named {
                operator: FunctionArgOperator::Assignment | FunctionArgOperator::RightArrow,
                ..
            }
        )));
        let reparsed = Parser::new(&PostgreSqlDialect {})
            .try_with_sql(&expr.to_string())
            .unwrap()
            .parse_expr()
            .unwrap();
        assert_eq!(expr, reparsed);
    }
    // Colon and VALUE are the same standard SQL/JSON constructor grammar,
    // rather than expression-named arguments to the legacy text[] function.
    for source in ["json_object('a': 1)", "json_object('a' VALUE 1)"] {
        let expr = Parser::new(&PostgreSqlDialect {})
            .try_with_sql(source)
            .unwrap()
            .parse_expr()
            .unwrap();
        let Expr::JsonObject(object) = &expr else {
            panic!("expected standard JSON object: {expr:?}")
        };
        let [entry] = object.entries.as_slice() else {
            panic!("expected exactly one key/value pair")
        };
        let Expr::Value(key) = &entry.key else {
            panic!("expected actual key literal")
        };
        assert_eq!(key.value, crate::ast::Value::SingleQuotedString("a".into()));
        let Expr::Value(value) = &entry.value.expr else {
            panic!("expected actual value literal")
        };
        assert_eq!(value.value, crate::test_utils::number("1"));
        assert!(entry.value.format.is_none());
        assert!(object.null_clause.is_none());
        assert!(object.unique_keys.is_none());
        assert!(object.returning.is_none());
        let rendered = expr.to_string();
        let reparsed = Parser::new(&PostgreSqlDialect {})
            .try_with_sql(&rendered)
            .unwrap()
            .parse_expr()
            .unwrap();
        assert_eq!(expr, reparsed);
    }
}
