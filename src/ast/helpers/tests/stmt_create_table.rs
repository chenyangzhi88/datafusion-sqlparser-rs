use crate::ast::helpers::stmt_create_table::CreateTableBuilder;
use crate::ast::{Ident, ObjectName, Statement};
use crate::parser::ParserError;

#[test]
pub fn test_from_valid_statement() {
    let builder = CreateTableBuilder::new(ObjectName::from(vec![Ident::new("table_name")]));

    let create_table = builder.clone().build();
    let stmt: Statement = create_table.into();

    assert_eq!(builder, CreateTableBuilder::try_from(stmt).unwrap());
}

#[test]
pub fn test_from_invalid_statement() {
    let stmt = Statement::Commit {
        completion: Default::default(),
        end: false,
        modifier: None,
    };

    assert_eq!(
        CreateTableBuilder::try_from(stmt).unwrap_err(),
        ParserError::ParserError(
            "Expected create table statement, but received: COMMIT".to_owned()
        )
    );
}

#[test]
fn mysql_duplicate_create_builder_moves_only_the_query_policy() {
    use crate::ast::MySqlCreateTableDuplicate;
    use crate::dialect::MySqlDialect;
    use crate::parser::Parser;
    let statement = Parser::parse_sql(&MySqlDialect {}, "CREATE TABLE t REPLACE AS SELECT 1")
        .unwrap()
        .remove(0);
    let builder = CreateTableBuilder::try_from(statement.clone()).unwrap();
    assert_eq!(
        builder.mysql_on_duplicate,
        Some(MySqlCreateTableDuplicate::Replace)
    );
    let rebuilt: Statement = builder.build().into();
    assert_eq!(rebuilt, statement);
    let invalid = CreateTableBuilder::new(ObjectName::from(vec![Ident::new("t")]))
        .mysql_on_duplicate(Some(MySqlCreateTableDuplicate::Ignore))
        .build();
    assert!(invalid.validate_source_order().is_err());
}

#[test]
fn mysql_duplicate_public_ast_export_preserves_builder_serde_and_visitor_consumers() {
    use crate::ast::MySqlCreateTableDuplicate;
    use crate::dialect::MySqlDialect;
    use crate::parser::Parser;
    for policy in [
        MySqlCreateTableDuplicate::Ignore,
        MySqlCreateTableDuplicate::Replace,
    ] {
        let sql = format!("CREATE TABLE dst {policy} AS SELECT id FROM src");
        let statement = Parser::parse_sql(&MySqlDialect {}, &sql).unwrap().remove(0);
        let builder = CreateTableBuilder::try_from(statement.clone()).unwrap();
        assert_eq!(builder.mysql_on_duplicate, Some(policy));
        assert_eq!(Statement::from(builder.build()), statement);
        assert_eq!(
            policy.to_string(),
            if policy == MySqlCreateTableDuplicate::Ignore {
                "IGNORE"
            } else {
                "REPLACE"
            }
        );
        #[cfg(feature = "serde")]
        {
            assert_eq!(
                serde_json::from_str::<MySqlCreateTableDuplicate>(
                    &serde_json::to_string(&policy).unwrap()
                )
                .unwrap(),
                policy
            );
            assert_eq!(
                serde_json::from_str::<Statement>(&serde_json::to_string(&statement).unwrap())
                    .unwrap(),
                statement
            );
        }
        #[cfg(feature = "visitor")]
        {
            let mut relations = Vec::new();
            let result = crate::ast::visit_relations(&statement, |name| {
                relations.push(name.to_string());
                core::ops::ControlFlow::<()>::Continue(())
            });
            assert!(matches!(result, core::ops::ControlFlow::Continue(())));
            assert_eq!(relations, vec!["dst", "src"]);
        }
    }
}
