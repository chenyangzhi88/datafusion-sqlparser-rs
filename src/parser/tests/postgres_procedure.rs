use crate::ast::{AlterFunctionKind, ArgMode, CreateProcedureBody, Statement};
use crate::dialect::{GenericDialect, MsSqlDialect, MySqlDialect, PostgreSqlDialect};
use crate::parser::Parser;

fn parse_one<D: crate::dialect::Dialect>(dialect: &D, sql: &str) -> Statement {
    let statements = Parser::parse_sql(dialect, sql)
        .unwrap_or_else(|error| panic!("failed to parse `{sql}`: {error}"));
    let [statement] = statements.as_slice() else {
        panic!("expected one statement for `{sql}`: {statements:?}");
    };
    statement.clone()
}

#[test]
fn postgres_create_procedure_has_typed_ast_and_round_trips() {
    let dialect = PostgreSqlDialect {};
    let sql = "CREATE OR REPLACE PROCEDURE public.p(IN n integer, INOUT label text DEFAULT 'x', VARIADIC rest text[]) LANGUAGE plpgsql SECURITY DEFINER SET search_path TO public AS $$BEGIN label := label || n::text; END$$";
    let statement = parse_one(&dialect, sql);
    let Statement::CreateProcedure(procedure) = &statement else {
        panic!("{statement:?}");
    };
    assert!(procedure.or_replace);
    assert!(!procedure.or_alter);
    assert_eq!(procedure.args.len(), 3);
    assert_eq!(procedure.args[1].mode, Some(ArgMode::InOut));
    assert!(procedure.args[1].default_expr.is_some());
    assert_eq!(procedure.args[2].mode, Some(ArgMode::Variadic));
    assert!(matches!(
        procedure.body,
        Some(CreateProcedureBody::PostgresAs { .. })
    ));
    assert_eq!(parse_one(&dialect, &statement.to_string()), statement);
}

#[test]
fn postgres_procedure_accepts_unnamed_arguments_and_option_order() {
    let dialect = PostgreSqlDialect {};
    for sql in [
        "CREATE PROCEDURE p(INTEGER, INOUT TEXT) LANGUAGE sql AS 'SELECT 1'",
        "CREATE PROCEDURE p(IN value INTEGER) EXTERNAL SECURITY INVOKER SET work_mem TO '4MB' LANGUAGE plpgsql AS $$BEGIN NULL; END$$",
        "CREATE PROCEDURE p(IN value INTEGER) SET work_mem FROM CURRENT SECURITY DEFINER LANGUAGE plpgsql AS $$BEGIN NULL; END$$",
    ] {
        let statement = parse_one(&dialect, sql);
        assert_eq!(parse_one(&dialect, &statement.to_string()), statement);
    }
}

#[test]
fn postgres_begin_atomic_keeps_every_statement() {
    let dialect = PostgreSqlDialect {};
    let statement = parse_one(
        &dialect,
        "CREATE PROCEDURE p(IN value INTEGER) LANGUAGE sql BEGIN ATOMIC INSERT INTO t VALUES (value); UPDATE t SET n = n + 1; END",
    );
    let Statement::CreateProcedure(procedure) = &statement else {
        panic!("{statement:?}");
    };
    let Some(CreateProcedureBody::PostgresBeginAtomic(body)) = &procedure.body else {
        panic!("expected BEGIN ATOMIC body: {procedure:?}");
    };
    assert_eq!(body.statements.len(), 2);
    assert_eq!(parse_one(&dialect, &statement.to_string()), statement);
}

#[test]
fn postgres_procedure_reports_duplicate_and_unsupported_options_atomically() {
    for (sql, message) in [
        (
            "CREATE PROCEDURE p() LANGUAGE sql LANGUAGE plpgsql AS 'SELECT 1'",
            "LANGUAGE specified more than once",
        ),
        (
            "CREATE PROCEDURE p() TRANSFORM FOR TYPE integer LANGUAGE sql AS 'SELECT 1'",
            "TRANSFORM",
        ),
    ] {
        let error = Parser::parse_sql(&PostgreSqlDialect {}, sql)
            .unwrap_err()
            .to_string();
        assert!(error.contains(message), "{error}");
        assert!(error.contains("Line: 1, Column:"), "{error}");
    }
}

#[test]
fn postgres_alter_procedure_has_typed_kind_and_round_trips() {
    let dialect = PostgreSqlDialect {};
    let statement = parse_one(
        &dialect,
        "ALTER PROCEDURE public.p(integer, text) RENAME TO renamed",
    );
    let Statement::AlterFunction(alter) = &statement else {
        panic!("{statement:?}");
    };
    assert_eq!(alter.kind, AlterFunctionKind::Procedure);
    assert_eq!(
        alter.to_string(),
        "ALTER PROCEDURE public.p(INTEGER, TEXT) RENAME TO renamed"
    );
    assert_eq!(parse_one(&dialect, &alter.to_string()), statement);
}

#[test]
fn generic_and_vendor_procedure_syntax_does_not_regress() {
    let generic = parse_one(
        &GenericDialect {},
        "CREATE OR ALTER PROCEDURE p(IN value integer) LANGUAGE sql AS 'SELECT value'",
    );
    let Statement::CreateProcedure(procedure) = &generic else {
        panic!("{generic:?}");
    };
    assert!(procedure.or_alter);
    assert!(!procedure.or_replace);

    for statement in [
        "CREATE PROCEDURE p(IN value INT) SELECT value",
        "CREATE PROCEDURE p(IN value INT) BEGIN SELECT value; END",
    ] {
        let parsed = Parser::parse_sql(&MySqlDialect {}, statement)
            .unwrap_or_else(|error| panic!("native MySQL procedure syntax regressed: {error}"));
        assert_eq!(parsed.len(), 1);
    }
    assert!(Parser::parse_sql(
        &MySqlDialect {},
        "CREATE PROCEDURE p(IN value INT) AS SELECT value; END"
    )
    .is_err());
    for (statement, dialect) in [(
        "CREATE PROCEDURE p AS SELECT 1; END",
        &MsSqlDialect {} as &dyn crate::dialect::Dialect,
    )] {
        let parsed = Parser::parse_sql(dialect, statement)
            .unwrap_or_else(|error| panic!("vendor procedure syntax regressed: {error}"));
        assert_eq!(parsed.len(), 1);
    }
}

#[test]
fn postgres_argument_modes_accept_either_name_order() {
    let dialect = PostgreSqlDialect {};
    for sql in [
        "CREATE PROCEDURE p(a OUT integer, VARIADIC b integer[]) LANGUAGE sql AS 'SELECT 1'",
        "CREATE FUNCTION f(a IN OUT integer) LANGUAGE sql AS 'SELECT a'",
        "ALTER PROCEDURE p(a OUT integer, b integer[]) RENAME TO q",
        "DROP PROCEDURE p(a OUT integer, b integer[])",
    ] {
        let statement = parse_one(&dialect, sql);
        assert_eq!(parse_one(&dialect, &statement.to_string()), statement);
    }
}

#[test]
fn postgres_routine_ddl_preserves_kind_across_round_trip() {
    let dialect = PostgreSqlDialect {};
    for sql in [
        "ALTER ROUTINE p(integer) RENAME TO q",
        "ALTER ROUTINE p SET SCHEMA destination",
        "ALTER ROUTINE p(integer) SECURITY DEFINER",
        "DROP ROUTINE IF EXISTS p(integer), q CASCADE",
    ] {
        let statement = parse_one(&dialect, sql);
        assert_eq!(parse_one(&dialect, &statement.to_string()), statement);
        assert!(statement.to_string().contains("ROUTINE"));
    }
}

#[test]
fn postgres_create_user_and_group_are_role_aliases() {
    for (sql, login) in [
        ("CREATE USER u", Some(true)),
        ("CREATE USER u NOLOGIN", Some(false)),
        ("CREATE GROUP g", None),
    ] {
        let Statement::CreateRole(role) = parse_one(&PostgreSqlDialect {}, sql) else {
            panic!("{sql}")
        };
        assert_eq!(role.login, login);
    }
    assert!(matches!(
        parse_one(&MySqlDialect {}, "CREATE USER u"),
        Statement::CreateUser(_)
    ));
}

#[test]
fn postgres_encrypted_password_uses_the_existing_password_option() {
    let dialect = PostgreSqlDialect {};
    for (encrypted, ordinary) in [
        (
            "CREATE USER test2 WITH ENCRYPTED PASSWORD 'abc123'",
            "CREATE USER test2 WITH PASSWORD 'abc123'",
        ),
        (
            "CREATE ROLE test2 ENCRYPTED PASSWORD $$abc123$$ LOGIN",
            "CREATE ROLE test2 PASSWORD $$abc123$$ LOGIN",
        ),
        (
            "ALTER ROLE test2 WITH ENCRYPTED PASSWORD 'abc123'",
            "ALTER ROLE test2 WITH PASSWORD 'abc123'",
        ),
    ] {
        assert_eq!(
            parse_one(&dialect, encrypted),
            parse_one(&dialect, ordinary)
        );
    }
    for sql in [
        "CREATE USER u ENCRYPTED PASSWORD NULL",
        "CREATE USER u ENCRYPTED PASSWORD 123",
        "CREATE USER u ENCRYPTED 'x'",
        "CREATE USER u PASSWORD 'x' ENCRYPTED PASSWORD 'y'",
        "ALTER ROLE u ENCRYPTED PASSWORD NULL",
        "ALTER ROLE u ENCRYPTED PASSWORD 123",
    ] {
        assert!(Parser::parse_sql(&dialect, sql).is_err(), "{sql}");
    }
}

#[test]
fn postgres_system_user_is_a_bare_function_and_quoted_names_remain_columns() {
    use crate::ast::{Expr, FunctionArguments, FunctionOrigin, SelectItem, SetExpr};
    let Statement::Query(query) =
        parse_one(&PostgreSqlDialect {}, "SELECT system_user, \"system_user\"")
    else {
        panic!("SELECT");
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("SELECT body")
    };
    let SelectItem::UnnamedExpr(Expr::Function(function)) = &select.projection[0] else {
        panic!("bare function")
    };
    assert_eq!(function.name.to_string(), "system_user");
    assert_eq!(function.origin, FunctionOrigin::Source);
    assert_eq!(function.args, FunctionArguments::None);
    assert!(
        matches!(&select.projection[1], SelectItem::UnnamedExpr(Expr::Identifier(name)) if name.quote_style == Some('"'))
    );
    assert!(Parser::parse_sql(&PostgreSqlDialect {}, "SELECT system_user()").is_err());
    let Statement::Query(query) = parse_one(&GenericDialect {}, "SELECT system_user") else {
        panic!("generic SELECT")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("generic SELECT body")
    };
    assert!(matches!(
        &select.projection[0],
        SelectItem::UnnamedExpr(Expr::Identifier(_))
    ));
}
