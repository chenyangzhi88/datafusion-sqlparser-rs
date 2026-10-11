use crate::{
    ast::{AlterTypeOperation, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};

#[test]
fn alter_type_schema_preserves_quoted_identifier_and_round_trips() {
    for sql in [
        "ALTER TYPE public.t SET SCHEMA destination",
        "ALTER TYPE \"S\".\"T\" SET SCHEMA \"New S\"",
    ] {
        let statements = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
        let [Statement::AlterType(alter)] = statements.as_slice() else {
            panic!("ALTER TYPE")
        };
        let AlterTypeOperation::SetSchema { schema_name } = &alter.operation else {
            panic!("SET SCHEMA")
        };
        assert_eq!(
            schema_name.value,
            if sql.contains("New S") {
                "New S"
            } else {
                "destination"
            }
        );
        assert_eq!(
            Parser::parse_sql(&PostgreSqlDialect {}, &statements[0].to_string()).unwrap(),
            statements
        );
    }
    for sql in ["ALTER TYPE t SET SCHEMA", "ALTER TYPE t SET SCHEMA a.b"] {
        assert!(Parser::parse_sql(&PostgreSqlDialect {}, sql).is_err());
    }
}
