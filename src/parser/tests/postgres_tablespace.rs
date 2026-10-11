use crate::{
    ast::{AlterTableOperation, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};

#[test]
fn table_tablespace_parser_preserves_quoted_names_and_roundtrips() {
    for sql in [
        "ALTER TABLE t SET TABLESPACE \"MiXeD\"",
        "ALTER TABLE t SET TABLESPACE regress_ts2",
    ] {
        let statement = Parser::parse_sql(&PostgreSqlDialect {}, sql)
            .unwrap()
            .remove(0);
        assert_eq!(statement.to_string(), sql);
        let Statement::AlterTable(alter) = statement else {
            panic!("ALTER TABLE");
        };
        assert!(matches!(
            alter.operations.as_slice(),
            [AlterTableOperation::SetTablespace { .. }]
        ));
    }
    let parsed =
        Parser::parse_sql(&PostgreSqlDialect {}, "CREATE TABLE t () TABLESPACE TsA").unwrap();
    assert!(parsed[0].to_string().ends_with("TABLESPACE tsa"));
}

#[test]
fn table_storage_options_and_tablespace_remain_separate_sql_clauses() {
    let sql = "CREATE TABLE t (a INT) WITH (fillfactor = 70) TABLESPACE tsa";
    let parsed = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
    assert_eq!(parsed[0].to_string(), sql);
}
