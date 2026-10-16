use crate::{
    ast::{PostgresTargetInheritance, Statement},
    dialect::{GenericDialect, PostgreSqlDialect},
    parser::Parser,
};

fn parse_one(sql: &str) -> Statement {
    Parser::parse_sql(&PostgreSqlDialect {}, sql)
        .unwrap()
        .remove(0)
}

#[test]
fn explicit_from_descendants_matches_implicit_relation_ast() {
    for (explicit, implicit) in [
        (
            "SELECT * FROM public.parent*",
            "SELECT * FROM public.parent",
        ),
        (
            "SELECT x.id FROM parent* AS x JOIN child* y ON x.id=y.id",
            "SELECT x.id FROM parent AS x JOIN child y ON x.id=y.id",
        ),
        (
            "SELECT count(*) FROM \"雪\" /* boundary */ * p",
            "SELECT count(*) FROM \"雪\" p",
        ),
    ] {
        assert_eq!(parse_one(explicit), parse_one(implicit));
    }
    assert!(Parser::parse_sql(&GenericDialect {}, "SELECT * FROM parent*").is_err());
    for invalid in [
        "SELECT * FROM parent*(1)",
        "SELECT * FROM parent p*",
        "SELECT * FROM parent**",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn dml_relation_targets_keep_only_and_explicit_descendants() {
    for (sql, expected) in [
        (
            "UPDATE parent* AS p SET id=1",
            PostgresTargetInheritance::Descendants,
        ),
        (
            "DELETE FROM public.parent* p WHERE id=1",
            PostgresTargetInheritance::Descendants,
        ),
        (
            "UPDATE ONLY(parent) AS p SET id=1",
            PostgresTargetInheritance::Only,
        ),
        (
            "DELETE FROM ONLY parent AS p WHERE id=1",
            PostgresTargetInheritance::Only,
        ),
        ("UPDATE parent SET id=1", PostgresTargetInheritance::Default),
        (
            "DELETE FROM parent WHERE id=1",
            PostgresTargetInheritance::Default,
        ),
    ] {
        let statement = parse_one(sql);
        let inheritance = match &statement {
            Statement::Update(update) => update.postgres_target_inheritance,
            Statement::Delete(delete) => delete.postgres_target_inheritance,
            _ => panic!("{statement:?}"),
        };
        assert_eq!(inheritance, expected, "{sql}");
        assert_eq!(parse_one(&statement.to_string()), statement);
    }
    for invalid in [
        "UPDATE ONLY parent* SET id=1",
        "DELETE FROM ONLY(parent)*",
        "UPDATE parent p* SET id=1",
        "DELETE FROM parent*(1)",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn alter_table_explicit_descendants_reuses_recursive_definition() {
    for operation in ["RENAME COLUMN id TO renamed", "ADD COLUMN extra integer"] {
        let statement = parse_one(&format!("ALTER TABLE public.parent* {operation}"));
        let Statement::AlterTable(alter) = &statement else {
            panic!("{statement:?}");
        };
        assert!(!alter.only);
        assert_eq!(
            statement.to_string(),
            parse_one(&format!("ALTER TABLE public.parent {operation}")).to_string()
        );
    }
    assert!(Parser::parse_sql(
        &PostgreSqlDialect {},
        "ALTER TABLE ONLY parent* ADD COLUMN x integer"
    )
    .is_err());
}

#[test]
fn alter_inheritance_parent_actions_preserve_typed_names_and_source_spans() {
    use crate::ast::{AlterTableOperation, Spanned};
    let sql = "ALTER TABLE ONLY child INHERIT app.\"MixedParent\", NO INHERIT other_parent";
    let statement = parse_one(sql);
    let Statement::AlterTable(alter) = &statement else {
        panic!("expected ALTER TABLE");
    };
    assert!(alter.only);
    let [AlterTableOperation::Inherit {
        parent: first,
        remove: false,
    }, AlterTableOperation::Inherit {
        parent: second,
        remove: true,
    }] = alter.operations.as_slice()
    else {
        panic!("expected direct inheritance actions");
    };
    assert_eq!(first.to_string(), "app.\"MixedParent\"");
    assert_eq!(second.to_string(), "other_parent");
    assert_eq!(alter.operations[0].span(), first.span());
    assert_eq!(alter.operations[1].span(), second.span());
    assert_eq!(statement.to_string(), sql);
    assert_eq!(parse_one(&statement.to_string()), statement);
}
