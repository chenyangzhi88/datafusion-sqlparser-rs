use crate::{
    ast::{MergeAction, Statement},
    dialect::{GenericDialect, PostgreSqlDialect},
    parser::Parser,
};

#[test]
fn postgres_merge_do_nothing_has_typed_action_and_round_trips() {
    let dialect = PostgreSqlDialect {};
    let sql =
        "MERGE INTO target USING source ON target.id = source.id WHEN MATCHED THEN DO NOTHING";
    let statements = Parser::parse_sql(&dialect, sql).unwrap();
    let [Statement::Merge(merge)] = statements.as_slice() else {
        panic!("expected one MERGE statement: {statements:?}");
    };
    assert!(matches!(
        merge.clauses[0].action,
        MergeAction::DoNothing { .. }
    ));
    assert_eq!(
        Parser::parse_sql(&dialect, &statements[0].to_string()).unwrap(),
        statements
    );
}

#[test]
fn generic_merge_does_not_accept_postgres_do_nothing_action() {
    let error = Parser::parse_sql(
        &GenericDialect {},
        "MERGE INTO target USING source ON true WHEN MATCHED THEN DO NOTHING",
    )
    .unwrap_err();
    assert!(error.to_string().contains("DO NOTHING"));
}
