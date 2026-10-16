use crate::{
    ast::Statement,
    dialect::{MySqlDialect, PostgreSqlDialect},
    parser::Parser,
};

#[test]
fn postgres_schema_preserves_all_embedded_utility_statements_and_round_trips() {
    let source = "CREATE SCHEMA declared AUTHORIZATION CURRENT_ROLE GRANT SELECT ON t TO PUBLIC CREATE INDEX ON t(id) CREATE VIEW v AS SELECT id FROM t CREATE TABLE t(id integer) CREATE SEQUENCE s CREATE TRIGGER tr BEFORE INSERT ON t EXECUTE FUNCTION public.tr()";
    let statements = Parser::parse_sql(&PostgreSqlDialect {}, source).unwrap();
    let [Statement::CreateSchema {
        schema_elements, ..
    }] = statements.as_slice()
    else {
        panic!("{statements:?}")
    };
    assert_eq!(schema_elements.len(), 6);
    assert!(matches!(schema_elements[0], Statement::Grant(_)));
    assert!(matches!(schema_elements[5], Statement::CreateTrigger(_)));
    assert_eq!(
        Parser::parse_sql(&PostgreSqlDialect {}, &statements[0].to_string()).unwrap(),
        statements
    );
}

#[test]
fn postgres_schema_does_not_admit_arbitrary_ddl_or_separated_statements() {
    for source in [
        "CREATE SCHEMA declared CREATE ROLE bad",
        "CREATE SCHEMA declared CREATE TABLE t AS SELECT 1",
        "CREATE SCHEMA declared CREATE MATERIALIZED VIEW v AS SELECT 1",
        "CREATE SCHEMA declared CREATE SCHEMA nested",
        "CREATE SCHEMA declared CREATE FUNCTION f() RETURNS int LANGUAGE SQL AS 'SELECT 1'",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, source).is_err(),
            "{source}"
        );
    }
    assert_eq!(
        Parser::parse_sql(
            &PostgreSqlDialect {},
            "CREATE SCHEMA declared; CREATE TABLE other(id integer)"
        )
        .unwrap()
        .len(),
        2
    );
    assert!(Parser::parse_sql(
        &MySqlDialect {},
        "CREATE SCHEMA declared CREATE TABLE t(id integer)"
    )
    .is_err());
}
