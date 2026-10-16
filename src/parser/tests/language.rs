use crate::{
    ast::Statement,
    dialect::{MySqlDialect, PostgreSqlDialect},
    parser::Parser,
};
#[test]
fn explicit_language_callbacks_and_lifetime_commands_round_trip() {
    for source in [
        "CREATE OR REPLACE TRUSTED PROCEDURAL LANGUAGE \"RenamedPL\" HANDLER pg_catalog.plpgsql_call_handler INLINE pg_catalog.plpgsql_inline_handler VALIDATOR pg_catalog.plpgsql_validator",
        "ALTER PROCEDURAL LANGUAGE \"RenamedPL\" RENAME TO final_pl",
        "DROP PROCEDURAL LANGUAGE IF EXISTS final_pl,absent_pl CASCADE",
        "COMMENT ON LANGUAGE final_pl IS 'language comment'",
    ] {
        let parsed = Parser::parse_sql(&PostgreSqlDialect {}, source).unwrap();
        assert_eq!(
            Parser::parse_sql(&PostgreSqlDialect {}, &parsed[0].to_string()).unwrap(),
            parsed
        );
    }
    let parsed = Parser::parse_sql(
        &PostgreSqlDialect {},
        "CREATE OR REPLACE TRUSTED PROCEDURAL LANGUAGE plpgsql",
    )
    .unwrap();
    let Statement::CreateExtension(create) = &parsed[0] else {
        panic!("handlerless CREATE is native extension alias")
    };
    assert!(create.if_not_exists);
    assert_eq!(create.name.value, "plpgsql");
}
#[test]
fn language_prefix_does_not_capture_other_dialects_or_incomplete_callbacks() {
    for source in [
        "CREATE LANGUAGE plpgsql HANDLER",
        "CREATE LANGUAGE x HANDLER callback INLINE",
        "DROP LANGUAGE",
        "ALTER LANGUAGE x RENAME",
        "CREATE PROCEDURAL TRUSTED LANGUAGE x HANDLER callback",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, source).is_err(),
            "{source}"
        );
    }
    assert!(
        Parser::parse_sql(&MySqlDialect {}, "CREATE LANGUAGE plpgsql HANDLER callback").is_err()
    );
}
