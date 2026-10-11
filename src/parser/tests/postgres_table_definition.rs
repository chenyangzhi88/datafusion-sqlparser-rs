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
fn heap_reloption_reset_preserves_typed_operation_values_and_spans() {
    use crate::ast::{AlterTableSetOperation, Spanned, SqlOption};
    for sql in [
        "ALTER TABLE t RESET (toast_tuple_target, autovacuum_enabled)",
        "ALTER TABLE t RESET (toast_tuple_target = 4080)",
    ] {
        let statement = parse_one(sql);
        assert_eq!(statement.to_string(), sql);
        let Statement::AlterTable(alter) = statement else {
            panic!("expected ALTER TABLE")
        };
        let [AlterTableOperation::SetOptionsParens {
            options: AlterTableSetOperation::PostgreSqlReset(options),
        }] = alter.operations.as_slice()
        else {
            panic!("expected RESET metadata")
        };
        assert_ne!(options[0].span(), crate::tokenizer::Span::empty());
        assert_eq!(
            matches!(options[0], SqlOption::KeyValue { .. }),
            sql.contains('=')
        );
    }
    for sql in [
        "ALTER TABLE t SET (autovacuum_enabled)",
        "CREATE TABLE t (a INTEGER) WITH (autovacuum_enabled)",
    ] {
        assert_eq!(parse_one(sql).to_string(), sql);
    }
    assert!(Parser::parse_sql(&PostgreSqlDialect {}, "ALTER TABLE t RESET ()").is_err());
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

#[test]
fn ctas_names_and_data_modes_have_their_own_ast_contract() {
    for (sql, names, data) in [
        (
            "CREATE TABLE copy (x, y) AS SELECT 1 AS a, 2 AS b",
            vec!["x", "y"],
            None,
        ),
        (
            "CREATE TEMP TABLE pg_temp.copy (x) AS SELECT 1 AS a, 2 AS b WITH DATA",
            vec!["x"],
            Some(true),
        ),
        (
            "CREATE TABLE copy AS WITH source AS (SELECT 1 AS i) SELECT i FROM source WITH NO DATA",
            vec![],
            Some(false),
        ),
        (
            "CREATE TABLE copy (\"Mixed\") AS (SELECT 1) WITH NO DATA",
            vec!["Mixed"],
            Some(false),
        ),
    ] {
        let statement = parse_one(sql);
        let Statement::CreateTable(create) = &statement else {
            panic!("expected CTAS: {statement:?}")
        };
        assert!(create.columns.is_empty());
        assert!(create.elements.is_empty());
        assert!(create.constraints.is_empty());
        assert_eq!(
            create
                .query_columns
                .iter()
                .map(|name| name.value.as_str())
                .collect::<Vec<_>>(),
            names
        );
        assert_eq!(create.with_data, data);
        assert_eq!(parse_one(&statement.to_string()), statement);
        #[cfg(all(feature = "serde", feature = "serde_json"))]
        {
            let serialized = serde_json::to_string(create).unwrap();
            let restored: crate::ast::CreateTable = serde_json::from_str(&serialized).unwrap();
            assert_eq!(&restored, create);
        }
        let rebuilt =
            crate::ast::helpers::stmt_create_table::CreateTableBuilder::from(create.clone())
                .build();
        assert_eq!(&rebuilt, create);
    }
    for invalid in [
        "CREATE TABLE copy (x integer) AS SELECT 1", // typed columns are not legal PostgreSQL CTAS targets
        "CREATE TABLE copy (x)",
        "CREATE TABLE copy AS SELECT 1 WITH NO",
        "CREATE TABLE copy AS SELECT 1 WITH DATA DATA",
    ] {
        let result = Parser::parse_sql(&PostgreSqlDialect {}, invalid);
        assert!(result.is_err(), "accepted invalid CTAS: {invalid}");
    }
    let Statement::CreateTable(ordinary) = parse_one("CREATE TABLE normal (i integer, j text)")
    else {
        panic!("expected ordinary table")
    };
    assert!(ordinary.query_columns.is_empty());
    assert!(ordinary.with_data.is_none());
    assert_eq!(ordinary.columns.len(), 2);
    let Statement::CreateTable(empty) = parse_one("CREATE TABLE empty ()") else {
        panic!("expected empty table")
    };
    assert!(empty.query_columns.is_empty());
    assert!(empty.query.is_none());
}

#[test]
fn postgres_constraint_trigger_requires_its_native_after_row_grammar() {
    use crate::ast::{TriggerObject, TriggerObjectKind, TriggerPeriod};
    for sql in [
        "CREATE CONSTRAINT TRIGGER ct AFTER INSERT ON source FOR EACH ROW EXECUTE FUNCTION callback()",
        "CREATE CONSTRAINT TRIGGER ct AFTER UPDATE OR DELETE ON source FROM referenced DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.id > 0) EXECUTE PROCEDURE callback('arg')",
    ] {
        let Statement::CreateTrigger(trigger) = parse_one(sql) else {
            panic!("expected constraint trigger: {sql}");
        };
        assert!(trigger.is_constraint);
        assert_eq!(trigger.period, Some(TriggerPeriod::After));
        assert_eq!(
            trigger.trigger_object,
            Some(TriggerObjectKind::ForEach(TriggerObject::Row))
        );
    }
    for sql in [
        "CREATE CONSTRAINT TRIGGER ct BEFORE INSERT ON source FOR EACH ROW EXECUTE FUNCTION callback()",
        "CREATE CONSTRAINT TRIGGER ct INSTEAD OF INSERT ON source FOR EACH ROW EXECUTE FUNCTION callback()",
        "CREATE CONSTRAINT TRIGGER ct INSERT ON source FOR EACH ROW EXECUTE FUNCTION callback()",
        "CREATE CONSTRAINT TRIGGER ct AFTER INSERT ON source FOR EACH STATEMENT EXECUTE FUNCTION callback()",
        "CREATE CONSTRAINT TRIGGER ct AFTER INSERT ON source FOR ROW EXECUTE FUNCTION callback()",
        "CREATE CONSTRAINT TRIGGER ct AFTER INSERT ON source EXECUTE FUNCTION callback()",
        "CREATE CONSTRAINT TRIGGER ct AFTER INSERT ON source REFERENCING NEW TABLE AS changed FOR EACH ROW EXECUTE FUNCTION callback()",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, sql).is_err(),
            "invalid native constraint grammar: {sql}"
        );
    }
    // The ordinary trigger production keeps PostgreSQL's optional EACH.
    let Statement::CreateTrigger(trigger) = parse_one(
        "CREATE TRIGGER ordinary BEFORE INSERT ON source FOR ROW EXECUTE FUNCTION callback()",
    ) else {
        panic!("expected ordinary trigger");
    };
    assert!(!trigger.is_constraint);
    assert_eq!(trigger.period, Some(TriggerPeriod::Before));
    assert_eq!(
        trigger.trigger_object,
        Some(TriggerObjectKind::For(TriggerObject::Row))
    );
}

#[test]
fn namespaced_reloptions_keep_identifier_quotes_value_and_reset_span() {
    use crate::ast::{CreateTableOptions, Spanned, SqlOption};
    let statement = parse_one(
        "CREATE TABLE t (a INTEGER) WITH (\"toast\".autovacuum_enabled = false, \"toast.autovacuum_enabled\" = true)",
    );
    let Statement::CreateTable(create) = &statement else {
        panic!("CREATE TABLE");
    };
    let CreateTableOptions::With(options) = &create.table_options else {
        panic!("WITH");
    };
    let SqlOption::Namespaced {
        namespace,
        key,
        value,
    } = &options[0]
    else {
        panic!("namespace");
    };
    assert_eq!(namespace.quote_style, Some('"'));
    assert_eq!(namespace.value, "toast");
    assert_eq!(key.value, "autovacuum_enabled");
    assert!(value.is_some());
    assert!(
        matches!(&options[1],SqlOption::KeyValue {key,..} if key.value=="toast.autovacuum_enabled" && key.quote_style==Some('"'))
    );
    assert_eq!(parse_one(&statement.to_string()), statement);
    let statement = parse_one("ALTER TABLE t RESET (toast.autovacuum_enabled)");
    let Statement::AlterTable(alter) = &statement else {
        panic!("ALTER TABLE");
    };
    let AlterTableOperation::SetOptionsParens {
        options: crate::ast::AlterTableSetOperation::PostgreSqlReset(options),
    } = &alter.operations[0]
    else {
        panic!("RESET");
    };
    assert!(matches!(
        options[0],
        SqlOption::Namespaced { value: None, .. }
    ));
    assert_ne!(options[0].span(), crate::tokenizer::Span::empty());
    assert_eq!(
        statement.to_string(),
        "ALTER TABLE t RESET (toast.autovacuum_enabled)"
    );
}
