use crate::ast::{
    AlterColumnOperation, AlterTableOperation, CreateTableElement, CreateTableLikeOption,
    CreateTableLikeOptionKind, Statement, TableConstraint,
};
use crate::dialect::PostgreSqlDialect;
use crate::parser::Parser;

fn parse_one(sql: &str) -> Statement {
    let statements = Parser::parse_sql(&PostgreSqlDialect {}, sql)
        .unwrap_or_else(|error| panic!("failed to parse `{sql}`: {error}"));
    let [statement] = statements.as_slice() else {
        panic!("expected one statement for `{sql}`: {statements:?}");
    };
    statement.clone()
}

#[test]
fn create_table_preserves_ordered_like_elements_and_overrides() {
    let statement = parse_one(
        "CREATE TABLE target (first integer, LIKE source INCLUDING ALL EXCLUDING DEFAULTS, last text)",
    );
    let Statement::CreateTable(create) = &statement else {
        panic!("expected CREATE TABLE: {statement:?}");
    };
    let [CreateTableElement::Column(first), CreateTableElement::Like(like), CreateTableElement::Column(last)] =
        create.elements.as_slice()
    else {
        panic!(
            "CREATE TABLE elements lost source order: {:?}",
            create.elements
        );
    };
    assert_eq!((*first, *last), (0, 1));
    assert_eq!(create.columns[*first].name.value, "first");
    assert_eq!(create.columns[*last].name.value, "last");
    assert!(create.validate_source_order().is_ok());
    assert_eq!(
        like.options,
        vec![
            CreateTableLikeOption::Including(CreateTableLikeOptionKind::All),
            CreateTableLikeOption::Excluding(CreateTableLikeOptionKind::Defaults),
        ]
    );
    assert_eq!(parse_one(&statement.to_string()), statement);
}

#[test]
fn alter_column_catalog_options_have_typed_ast() {
    for (sql, expected) in [
        (
            "ALTER TABLE items ALTER COLUMN payload SET STATISTICS -1",
            AlterColumnOperation::SetStatistics { target: -1 },
        ),
        (
            "ALTER TABLE items ALTER COLUMN payload SET STORAGE EXTENDED",
            AlterColumnOperation::SetStorage {
                storage: crate::ast::Ident::new("EXTENDED"),
            },
        ),
        (
            "ALTER TABLE items ALTER COLUMN payload SET COMPRESSION pglz",
            AlterColumnOperation::SetCompression {
                method: crate::ast::Ident::new("pglz"),
            },
        ),
    ] {
        let statement = parse_one(sql);
        let Statement::AlterTable(alter) = &statement else {
            panic!("expected ALTER TABLE: {statement:?}");
        };
        let [AlterTableOperation::AlterColumn { op, .. }] = alter.operations.as_slice() else {
            panic!("expected one ALTER COLUMN operation: {statement:?}");
        };
        assert_eq!(op, &expected);
        assert_eq!(parse_one(&statement.to_string()), statement);
    }
}

#[test]
fn alter_column_statistics_rejects_default_keyword() {
    let error = Parser::parse_sql(
        &PostgreSqlDialect {},
        "ALTER TABLE items ALTER COLUMN payload SET STATISTICS DEFAULT",
    )
    .expect_err("PostgreSQL accepts only SignedIconst after SET STATISTICS");
    assert!(error.to_string().contains("unsigned integer"));
}

#[test]
fn postgres_exclusion_constraint_preserves_operator_and_index_definition() {
    let sql = "CREATE TABLE booking (room int4range, during tsrange, CONSTRAINT booking_no_overlap EXCLUDE USING gist (room gist_int4_range_ops WITH =, (lower(during)) WITH OPERATOR(pg_catalog.&&)) INCLUDE (note) WITH (fillfactor = 80) USING INDEX TABLESPACE local_data WHERE (room IS NOT NULL) DEFERRABLE INITIALLY DEFERRED)";
    let statement = parse_one(sql);
    let Statement::CreateTable(create) = &statement else {
        panic!("expected CREATE TABLE: {statement:?}");
    };
    let [TableConstraint::Exclude(exclude)] = create.constraints.as_slice() else {
        panic!(
            "expected typed exclusion constraint: {:?}",
            create.constraints
        );
    };
    assert_eq!(
        exclude.name.as_ref().map(|name| name.value.as_str()),
        Some("booking_no_overlap")
    );
    assert_eq!(exclude.elements.len(), 2);
    assert_eq!(exclude.elements[0].operator.to_string(), "=");
    assert_eq!(exclude.elements[1].operator.to_string(), "pg_catalog.&&");
    assert_eq!(
        exclude
            .include
            .iter()
            .map(|name| name.value.as_str())
            .collect::<Vec<_>>(),
        ["note"]
    );
    assert!(exclude.predicate.is_some());
    assert!(exclude
        .characteristics
        .as_ref()
        .is_some_and(|characteristics| characteristics.deferrable == Some(true)
            && characteristics.initially == Some(crate::ast::DeferrableInitial::Deferred)));
    assert_eq!(parse_one(&statement.to_string()), statement);

    let alter = parse_one("ALTER TABLE booking ADD CONSTRAINT no_overlap EXCLUDE (room WITH =)");
    let Statement::AlterTable(alter) = alter else {
        panic!("expected ALTER TABLE");
    };
    assert!(matches!(
        alter.operations.as_slice(),
        [AlterTableOperation::AddConstraint {
            constraint: TableConstraint::Exclude(_),
            ..
        }]
    ));
}
