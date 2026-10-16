use crate::ast::{Expr, MySqlHistogramAction, Statement};
use crate::dialect::MySqlDialect;
use crate::parser::Parser;

#[test]
fn analyze_modifiers_and_table_synonym_share_the_typed_histogram_action() {
    use crate::ast::{FlushLocation, MySqlTableMaintenanceOperation};
    for (modifier, expected) in [
        ("", None),
        ("LOCAL ", Some(FlushLocation::Local)),
        ("NO_WRITE_TO_BINLOG ", Some(FlushLocation::NoWriteToBinlog)),
    ] {
        for table in ["TABLE", "TABLES"] {
            for suffix in [
                "",
                " UPDATE HISTOGRAM ON a,b WITH 2 BUCKETS",
                " DROP HISTOGRAM ON a,b",
                " UPDATE HISTOGRAM ON a USING DATA '{}'",
            ] {
                let sql = format!("ANALYZE {modifier}{table} db.t{suffix}");
                let parsed = Parser::parse_sql(&MySqlDialect {}, &sql).unwrap().remove(0);
                let Statement::MySqlTableMaintenance {
                    operation,
                    location,
                    tables,
                    options,
                    histogram,
                } = &parsed
                else {
                    panic!("{sql}: {parsed:?}")
                };
                assert_eq!(*operation, MySqlTableMaintenanceOperation::Analyze);
                assert_eq!(*location, expected);
                assert_eq!(tables.len(), 1);
                assert!(options.is_empty());
                assert_eq!(histogram.is_some(), !suffix.is_empty());
                if suffix.contains("USING DATA") {
                    assert!(matches!(
                        histogram,
                        Some(MySqlHistogramAction::Update {
                            data: Some(Expr::Value(_)),
                            buckets: 100,
                            ..
                        })
                    ));
                }
                assert_eq!(
                    Parser::parse_sql(&MySqlDialect {}, &parsed.to_string()).unwrap()[0],
                    parsed
                );
            }
        }
    }
    for sql in [
        "ANALYZE LOCAL NO_WRITE_TO_BINLOG TABLE t",
        "ANALYZE LOCAL LOCAL TABLE t",
        "ANALYZE TABLE LOCAL t",
        "ANALYZE TABLE t LOCAL",
        "ANALYZE NO_WRITE_TO_BINLOG t",
        "CHECK LOCAL TABLE t",
        "CHECK NO_WRITE_TO_BINLOG TABLE t",
        "OPTIMIZE LOCAL NO_WRITE_TO_BINLOG TABLE t",
        "REPAIR LOCAL LOCAL TABLE t",
        "REPAIR NO_WRITE_TO_BINLOG LOCAL TABLE t",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
    // mysql-8.4.0/sql/sql_yacc.yy: all three use table_or_tables;
    // OPTIMIZE and REPAIR also use opt_no_write_to_binlog, including LOCAL.
    for (command, expected_operation) in [
        ("CHECK", MySqlTableMaintenanceOperation::Check),
        ("OPTIMIZE", MySqlTableMaintenanceOperation::Optimize),
        ("REPAIR", MySqlTableMaintenanceOperation::Repair),
    ] {
        for (modifier, expected_location) in [
            ("", None),
            ("LOCAL ", Some(FlushLocation::Local)),
            ("NO_WRITE_TO_BINLOG ", Some(FlushLocation::NoWriteToBinlog)),
        ] {
            if expected_operation == MySqlTableMaintenanceOperation::Check
                && expected_location.is_some()
            {
                continue;
            }
            for table in ["TABLE", "TABLES"] {
                let sql = format!("{command} {modifier}{table} db.t,db.u");
                let parsed = Parser::parse_sql(&MySqlDialect {}, &sql).unwrap().remove(0);
                let Statement::MySqlTableMaintenance {
                    operation,
                    location,
                    tables,
                    options,
                    histogram,
                } = &parsed
                else {
                    panic!("{sql}: {parsed:?}")
                };
                assert_eq!(*operation, expected_operation, "{sql}");
                assert_eq!(*location, expected_location, "{sql}");
                assert_eq!(tables.len(), 2, "{sql}");
                assert!(options.is_empty(), "{sql}");
                assert!(histogram.is_none(), "{sql}");
                assert_eq!(
                    Parser::parse_sql(&MySqlDialect {}, &parsed.to_string()).unwrap()[0],
                    parsed,
                    "{sql}"
                );
            }
        }
    }
}

#[test]
fn histogram_actions_round_trip_without_raw_secondary_parser_or_execution_effects() {
    for (sql, count) in [
        ("ANALYZE TABLE t UPDATE HISTOGRAM ON a,b WITH 1 BUCKETS", 1),
        (
            "ANALYZE TABLE t UPDATE HISTOGRAM ON a WITH 1024 BUCKETS",
            1024,
        ),
        ("ANALYZE TABLE t UPDATE HISTOGRAM ON a", 100),
    ] {
        let parsed = Parser::parse_sql(&MySqlDialect {}, sql).unwrap().remove(0);
        let Statement::MySqlTableMaintenance {
            histogram:
                Some(MySqlHistogramAction::Update {
                    buckets,
                    data,
                    columns,
                }),
            ..
        } = &parsed
        else {
            panic!("{parsed:?}")
        };
        assert_eq!(*buckets, count);
        assert!(data.is_none());
        assert!(!columns.is_empty());
        let reparsed = Parser::parse_sql(&MySqlDialect {}, &parsed.to_string())
            .unwrap()
            .remove(0);
        assert_eq!(reparsed, parsed);
    }
    let parsed = Parser::parse_sql(
        &MySqlDialect {},
        "ANALYZE TABLE t UPDATE HISTOGRAM ON a USING DATA '{}'",
    )
    .unwrap()
    .remove(0);
    assert!(matches!(
        parsed,
        Statement::MySqlTableMaintenance {
            histogram: Some(MySqlHistogramAction::Update {
                data: Some(Expr::Value(_)),
                buckets: 100,
                ..
            }),
            ..
        }
    ));
    let parsed = Parser::parse_sql(&MySqlDialect {}, "ANALYZE TABLE t DROP HISTOGRAM ON a,b")
        .unwrap()
        .remove(0);
    assert!(
        matches!(parsed,Statement::MySqlTableMaintenance {histogram:Some(MySqlHistogramAction::Drop {ref columns}),..} if columns.len()==2)
    );
    for sql in [
        "ANALYZE TABLE t UPDATE HISTOGRAM ON a USING DATA NULL",
        "ANALYZE TABLE t UPDATE HISTOGRAM ON a USING DATA CONCAT('{}')",
        "ANALYZE TABLE t UPDATE HISTOGRAM ON a WITH -1 BUCKETS",
        "CHECK TABLE t UPDATE HISTOGRAM ON a",
        "ANALYZE TABLE t DROP HISTOGRAM ON a WITH 1 BUCKETS",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
}

#[test]
fn json_table_literal_slots_keep_narrow_grammar_charset_and_actual_clause_order() {
    for suffix in [
        "",
        "NULL ON EMPTY",
        "ERROR ON ERROR",
        "NULL ON EMPTY ERROR ON ERROR",
        "ERROR ON ERROR NULL ON EMPTY",
        "DEFAULT -1 ON ERROR",
        "DEFAULT DATE '2020-01-01' ON EMPTY",
    ] {
        let sql = format!(
            "SELECT * FROM JSON_TABLE('{{}}', '$' COLUMNS(v VARCHAR(30) CHARACTER SET latin1 COLLATE latin1_bin PATH '$.v' {suffix})) j"
        );
        let parsed = Parser::parse_sql(&MySqlDialect {}, &sql).unwrap().remove(0);
        let rendered = parsed.to_string();
        let reparsed = Parser::parse_sql(&MySqlDialect {}, &rendered)
            .unwrap()
            .remove(0);
        assert_eq!(parsed, reparsed, "{sql}");
    }
    for suffix in [
        "DEFAULT NULL ON EMPTY",
        "DEFAULT CONCAT('1') ON ERROR",
        "NULL ON EMPTY ERROR ON EMPTY",
        "ERROR ON ERROR NULL ON ERROR",
        "NULL ON EMPTY ERROR ON ERROR NULL ON EMPTY",
    ] {
        let sql =
            format!("SELECT * FROM JSON_TABLE('{{}}', '$' COLUMNS(v INT PATH '$.v' {suffix})) j");
        assert!(Parser::parse_sql(&MySqlDialect {}, &sql).is_err(), "{sql}");
    }
}
