use crate::ast::{
    AlterColumnOperation, AlterSchemaOperation, AlterSchemaSetOperation, AlterTableOperation,
    AlterTableSetOperation, BinaryOperator, CastKind, ColumnOption, DataType, DateTimeField, Expr,
    MySqlCheckTableOption, MySqlDeleteModifier, MySqlExchangeValidation, MySqlKeyAlgorithm,
    MySqlPartitionBy, MySqlPartitionMaintenance, MySqlPartitionMethod, MySqlPartitionOption,
    MySqlSubpartitionMethod, SelectIntoTarget, SelectItem, Set, SetExpr, Statement,
};
use crate::dialect::MySqlDialect;
use crate::parser::Parser;

#[test]
fn mysql_load_text_string_binary_forms_do_not_widen_filesystem_or_identifier_grammar() {
    for delimiter in ["0x09", "X'09'", "0b1001", "B'1001'"] {
        let sql = format!("LOAD DATA INFILE 'f' INTO TABLE t FIELDS TERMINATED BY {delimiter}");
        let parsed = Parser::parse_sql(&MySqlDialect {}, &sql).unwrap();
        let Statement::MySqlLoadData(load) = &parsed[0] else {
            panic!("actual LOAD production");
        };
        assert!(!load.xml);
        assert_eq!(load.fields.len(), 1);
    }
    let parsed = Parser::parse_sql(&MySqlDialect {}, "LOAD XML LOCAL INFILE 'f' INTO TABLE t ROWS IDENTIFIED BY X'3c723e' IGNORE 2 ROWS (id,@v) SET id=id+1").unwrap();
    let Statement::MySqlLoadData(load) = &parsed[0] else {
        panic!("actual XML production");
    };
    assert!(load.xml && load.local);
    assert_eq!(load.ignore, Some((2, true)));
    assert!(load.xml_rows_identified_by.is_some());
    assert_eq!(load.inputs.as_ref().unwrap().len(), 2);
    assert_eq!(load.assignments.len(), 1);
    for sql in [
        "LOAD DATA INFILE 0x66 INTO TABLE t",
        "LOAD DATA INFILE B'1' INTO TABLE t",
        "LOAD DATA INFILE 'f' INTO TABLE t FIELDS TERMINATED BY identifier",
        "LOAD DATA INFILE 'f' INTO TABLE t FIELDS ESCAPED BY NULL",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
}

#[test]
fn mysql_profile_uses_the_native_option_query_id_and_limit_atoms() {
    for (sql, profiles, mask, query, limit, offset) in [
        ("SHOW PROFILES", true, 0, 0, None, None),
        ("SHOW PROFILE", false, 0, 0, None, None),
        (
            "SHOW PROFILE CPU, BLOCK IO, CONTEXT SWITCHES, PAGE FAULTS, IPC, SWAPS, SOURCE FOR QUERY 7 LIMIT 2 OFFSET 1",
            false,
            1 | 4 | 8 | 16 | 32 | 64 | 65536,
            7,
            Some("2"),
            Some("1"),
        ),
        (
            "SHOW PROFILE MEMORY FOR QUERY 2147483647 LIMIT 1, 2",
            false,
            2,
            2147483647,
            Some("2"),
            Some("1"),
        ),
        (
            "SHOW PROFILE ALL LIMIT ?, ?",
            false,
            u32::MAX,
            0,
            Some("?"),
            Some("?"),
        ),
        (
            "SHOW PROFILE ALL FOR QUERY 7 LIMIT 2 OFFSET 1",
            false,
            u32::MAX,
            7,
            Some("2"),
            Some("1"),
        ),
    ] {
        let parsed = Parser::parse_sql(&MySqlDialect {}, sql).unwrap();
        assert_eq!(parsed.len(), 1, "{sql}");
        let Statement::MySqlShowProfile {
            profiles: actual_profiles,
            options,
            query_id,
            limit: actual_limit,
            offset: actual_offset,
        } = &parsed[0]
        else {
            panic!("{sql} lost its native command");
        };
        assert_eq!(
            (*actual_profiles, *options, *query_id),
            (profiles, mask, query),
            "{sql}"
        );
        assert_eq!(
            actual_limit.as_ref().map(ToString::to_string).as_deref(),
            limit
        );
        assert_eq!(
            actual_offset.as_ref().map(ToString::to_string).as_deref(),
            offset
        );
        if mask == u32::MAX {
            assert_eq!(
                parsed[0].to_string(),
                if query == 0 {
                    "SHOW PROFILE ALL LIMIT ? OFFSET ?"
                } else {
                    "SHOW PROFILE ALL FOR QUERY 7 LIMIT 2 OFFSET 1"
                },
                "ALL preserves its typed mask rather than enumerating known options"
            );
        }
        assert_eq!(
            Parser::parse_sql(&MySqlDialect {}, &parsed[0].to_string()).unwrap(),
            parsed,
            "presentation remains presentation"
        );
    }
    for sql in [
        "SHOW PROFILES LIMIT 1",
        "SHOW FULL PROFILE",
        "SHOW PROFILE CPU BLOCK IO",
        "SHOW PROFILE CPU,",
        "SHOW PROFILE BLOCK",
        "SHOW PROFILE PAGE IO",
        "SHOW PROFILE FOR QUERY ?",
        "SHOW PROFILE FOR QUERY -1",
        "SHOW PROFILE FOR QUERY 2147483648",
        "SHOW PROFILE FOR QUERY '1'",
        "SHOW PROFILE LIMIT -1",
        "SHOW PROFILE LIMIT 1+1",
        "SHOW PROFILE LIMIT '1'",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
}

#[test]
fn mysql_component_uses_one_typed_literal_command_and_preserves_generic_install() {
    for (sql, install, urns) in [
        (
            "INSTALL COMPONENT 'file://component_query_attributes'",
            true,
            vec!["file://component_query_attributes"],
        ),
        (
            "UNINSTALL COMPONENT 'file://component_query_attributes'",
            false,
            vec!["file://component_query_attributes"],
        ),
        ("INSTALL COMPONENT 'a', 'b''c'", true, vec!["a", "b'c"]),
    ] {
        let mut parsed = Parser::parse_sql(&MySqlDialect {}, sql).unwrap();
        let statement = parsed.pop().unwrap();
        assert!(parsed.is_empty());
        assert!(
            matches!(&statement, Statement::MySqlComponent { install: actual, urns: actual_urns }
            if *actual == install && actual_urns.iter().map(String::as_str).collect::<Vec<_>>() == urns)
        );
        assert_eq!(
            Parser::parse_sql(&MySqlDialect {}, &statement.to_string()).unwrap(),
            vec![statement]
        );
    }
    for sql in [
        "INSTALL COMPONENT",
        "INSTALL COMPONENT component_query_attributes",
        "INSTALL COMPONENT @urn",
        "UNINSTALL COMPONENT ?",
        "INSTALL COMPONENT 'a',",
        "INSTALL COMPONENT 'a' SET GLOBAL x=1",
        "`UNINSTALL` COMPONENT 'a'",
        "INSTALL `COMPONENT` 'a'",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
    assert!(matches!(
        Parser::parse_sql(&crate::dialect::DuckDbDialect {}, "INSTALL httpfs").unwrap()[0],
        Statement::Install { .. },
    ));
}

#[test]
fn mysql_against_retains_native_bit_expr_child_and_leaves_mode_on_same_cursor() {
    use crate::ast::SearchModifier;
    use crate::tokenizer::Token;
    let dialect = MySqlDialect {};
    for (sql, mode) in [
        (
            "MATCH(t.body) AGAINST(concat(?, 'word') IN BOOLEAN MODE)",
            Some(SearchModifier::InBooleanMode),
        ),
        (
            "MATCH(t.body) AGAINST(1 + 2 IN NATURAL LANGUAGE MODE)",
            Some(SearchModifier::InNaturalLanguageMode),
        ),
        (
            "MATCH(t.body) AGAINST(@needle WITH QUERY EXPANSION)",
            Some(SearchModifier::WithQueryExpansion),
        ),
        ("MATCH(t.body) AGAINST(NULL)", None),
        ("MATCH(t.body) AGAINST((1 = 1))", None),
    ] {
        let mut parser = Parser::new(&dialect).try_with_sql(sql).unwrap();
        let expression = parser.parse_expr().unwrap();
        let Expr::MatchAgainst {
            columns,
            match_value,
            opt_search_modifier,
        } = &expression
        else {
            panic!("original MATCH Item");
        };
        assert_eq!(columns.len(), 1);
        assert_eq!(columns[0].to_string(), "t.body");
        assert_eq!(*opt_search_modifier, mode);
        assert_eq!(parser.peek_token_ref().token, Token::EOF, "{sql}");
        let display = expression.to_string();
        let mut roundtrip = Parser::new(&dialect).try_with_sql(&display).unwrap();
        let parsed = roundtrip.parse_expr().unwrap();
        assert_eq!(parsed, expression);
        assert!(matches!(
            match_value.as_ref(),
            Expr::Function(_)
                | Expr::BinaryOp { .. }
                | Expr::UserVariable(_)
                | Expr::Value(_)
                | Expr::Nested(_)
        ));
    }
    for sql in [
        "MATCH(t.body) AGAINST(1 = 1)",
        "MATCH(t.body) AGAINST(1 IN (1))",
        "MATCH(t.body) AGAINST('x', 'y')",
        "MATCH(t.body) AGAINST()",
    ] {
        assert!(
            Parser::new(&dialect)
                .try_with_sql(sql)
                .unwrap()
                .parse_expr()
                .is_err(),
            "{sql}"
        );
    }
}

#[test]
fn mysql_all_set_options_share_one_rhs_cursor_without_scope_inheritance_in_the_parser() {
    use crate::parser::{MySqlSetAssignment, MySqlSetOption};
    use crate::tokenizer::Token;
    let dialect = MySqlDialect {};
    let mut parser = Parser::new(&dialect).try_with_sql(
        "SET PERSIST a=1, b=2, @@session.c=3, @u:=4, PERSIST_ONLY d=DEFAULT, GLOBAL e=5, SESSION f=6, NAMES utf8mb4 COLLATE utf8mb4_bin, CHARACTER SET DEFAULT; SELECT 9"
    ).unwrap();
    assert!(parser.parse_keyword(crate::keywords::Keyword::SET));
    let mut reductions = Vec::new();
    let list = parser
        .parse_mysql_set_options_with(|parser, start, expression| {
            assert!(parser.index() > start);
            assert!(parser.mysql_consumed_source(start)?.is_none());
            reductions.push((start, parser.index()));
            Ok(expression)
        })
        .unwrap();
    assert_eq!(reductions.len(), 7);
    assert!(reductions.windows(2).all(|rows| rows[0].1 < rows[1].0));
    assert_eq!(list.len(), 9);
    let keywords = list[..7]
        .iter()
        .map(|assignment| match assignment {
            MySqlSetAssignment::Variable { keyword, .. } => *keyword,
            _ => panic!("actual original RHS"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        keywords,
        [
            Some(MySqlSetOption::Persist),
            None,
            None,
            None,
            Some(MySqlSetOption::PersistOnly),
            Some(MySqlSetOption::Global),
            Some(MySqlSetOption::Session),
        ]
    );
    assert!(matches!(&list[7], MySqlSetAssignment::Names {
        character_set: Some(name), collation: Some(collation)
    } if name.value == "utf8mb4" && collation.value == "utf8mb4_bin"));
    assert!(matches!(
        &list[8],
        MySqlSetAssignment::CharacterSet {
            character_set: None
        }
    ));
    assert!(matches!(parser.peek_token_ref().token, Token::SemiColon));
    parser.next_token();
    assert!(matches!(
        parser.parse_statement().unwrap(),
        Statement::Query(_)
    ));
    let mut broken = Parser::new(&dialect)
        .try_with_sql("SET PERSIST a=1,b=2,c=3")
        .unwrap();
    assert!(broken.parse_keyword(crate::keywords::Keyword::SET));
    let mut calls = 0;
    let failure = broken
        .parse_mysql_set_options_with(|_, _, value| {
            calls += 1;
            if calls == 2 {
                Err(crate::parser::ParserError::ParserError(
                    "one failure".into(),
                ))
            } else {
                Ok(value)
            }
        })
        .unwrap_err();
    assert_eq!(calls, 2);
    assert_eq!(
        failure,
        crate::parser::ParserError::ParserError("one failure".into())
    );
    let foreign = crate::dialect::PostgreSqlDialect {};
    assert!(Parser::new(&foreign)
        .try_with_sql("a=1")
        .unwrap()
        .parse_mysql_set_options_with(|_, _, value| Ok(value))
        .is_err());
}

#[test]
fn mysql_set_rhs_callback_runs_once_at_each_actual_operator_frontier() {
    use crate::ast::ContextModifier;
    let dialect = MySqlDialect {};
    let mut parser = Parser::new(&dialect)
        .try_with_sql("SET a = 1 + 2, SESSION sql_mode = 'ANSI_QUOTES', @b := (3 + 4)")
        .unwrap();
    assert!(parser.parse_keyword(crate::keywords::Keyword::SET));
    let mut frontiers = Vec::new();
    let assignments = parser
        .parse_mysql_set_assignments_with(|parser, start, value| {
            assert!(parser.index() > start);
            assert!(
                parser.mysql_consumed_source(start)?.is_none(),
                "decoded tokens cannot issue raw Source"
            );
            frontiers.push((start, parser.index()));
            Ok(value)
        })
        .unwrap();
    assert_eq!(frontiers.len(), 3);
    assert!(frontiers.windows(2).all(|pairs| pairs[0].1 < pairs[1].0));
    assert_eq!(assignments[0].0, None);
    assert_eq!(assignments[1].0, Some(ContextModifier::Session));
    assert_eq!(assignments[2].0, None);
    assert!(matches!(assignments[0].2, Expr::BinaryOp { .. }));
    assert!(matches!(assignments[2].2, Expr::Nested(_)));

    let mut parser = Parser::new(&dialect)
        .try_with_sql("SET a=1,b=2,c=3")
        .unwrap();
    assert!(parser.parse_keyword(crate::keywords::Keyword::SET));
    let mut calls = 0;
    let error = parser
        .parse_mysql_set_assignments_with(|_, _, value| {
            calls += 1;
            if calls == 2 {
                Err(crate::parser::ParserError::ParserError(
                    "original callback failure".into(),
                ))
            } else {
                Ok(value)
            }
        })
        .unwrap_err();
    assert_eq!(
        calls, 2,
        "failure never retries the list or consumes the third RHS"
    );
    assert_eq!(
        error,
        crate::parser::ParserError::ParserError("original callback failure".into())
    );
    let foreign_dialect = crate::dialect::PostgreSqlDialect {};
    let mut foreign = Parser::new(&foreign_dialect).try_with_sql("a=1").unwrap();
    assert!(foreign
        .parse_mysql_set_assignments_with(|_, _, value| Ok(value))
        .is_err());
}

#[test]
fn mysql_help_preserves_one_system_mask_and_rejects_expression_or_prepared_grammar() {
    for sql in ["HELP HELP", "HELP help", "HELP status", "HELP `SELECT`"] {
        let statements = Parser::parse_sql(&MySqlDialect {}, sql).unwrap();
        let [Statement::Help {
            mask: Expr::Identifier(identifier),
        }] = statements.as_slice()
        else {
            panic!("HELP must retain the actual identifier grammar: {sql}");
        };
        assert!(!identifier.value.is_empty());
        assert_eq!(statements.len(), 1);
    }
    for sql in [
        "HELP 'SELECT'",
        "HELP \"contents\"",
        "HELP '%;\\n%'",
        "HELP '海豚'",
    ] {
        let statements = Parser::parse_sql(&MySqlDialect {}, sql).unwrap();
        let [Statement::Help {
            mask: Expr::Value(value),
        }] = statements.as_slice()
        else {
            panic!("HELP must retain one system-text grammar node: {sql}");
        };
        assert!(matches!(
            value.value,
            crate::ast::Value::SingleQuotedString(_)
                | crate::ast::Value::DoubleQuotedString(_)
                | crate::ast::Value::MySqlText(_)
        ));
    }
    for sql in [
        "HELP",
        "HELP SELECT",
        "HELP select",
        "HELP 1",
        "HELP NULL",
        "HELP ?",
        "HELP @mask",
        "HELP CONCAT('a','b')",
        "HELP 'a' 'b'",
        "HELP 'a' + 'b'",
        "HELP _utf8mb4 'a'",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
    let statements = Parser::parse_sql(&MySqlDialect {}, "HELP 'a; b'; SELECT 1").unwrap();
    assert!(matches!(&statements[0], Statement::Help { .. }));
    assert!(matches!(&statements[1], Statement::Query(_)));
    assert_eq!(statements.len(), 2);
    assert!(Parser::parse_sql(&crate::dialect::PostgreSqlDialect {}, "HELP 'a'").is_err());
}

#[test]
fn mysql_cte_insertion_uses_exact_identifiers_and_native_error_priority() {
    use crate::parser::ParserError;
    for sql in [
        "WITH r AS (SELECT 1), R AS (SELECT 2) UPDATE dst SET n=(SELECT * FROM r)+(SELECT * FROM R)",
        "WITH `r` AS (SELECT 1), `R` AS (SELECT 2) DELETE FROM dst WHERE n IN (SELECT * FROM R)",
        "WITH RECURSIVE r AS (SELECT 1 UNION ALL SELECT * FROM r), R AS (SELECT 2) SELECT * FROM R",
        "WITH r AS (SELECT 1) SELECT * FROM (WITH r AS (SELECT 2) SELECT * FROM r) nested",
    ] {
        let ast = Parser::parse_sql(&MySqlDialect {}, sql).unwrap();
        let [Statement::Query(query)] = ast.as_slice() else {
            panic!("actual WITH query");
        };
        let names: Vec<_> = query
            .with
            .as_ref()
            .unwrap()
            .cte_tables
            .iter()
            .map(|cte| cte.alias.name.value.as_str())
            .collect();
        assert_eq!(names[0], "r");
        if names.len() == 2 {
            assert_eq!(names[1], "R");
        }
        for cte in &query.with.as_ref().unwrap().cte_tables {
            assert!(cte.closing_paren_token.0.span.start.column > 0);
        }
    }
    for (sql, name) in [
        ("WITH r AS (SELECT 1), r AS (SELECT 2) UPDATE", "r"),
        (
            "WITH r AS (SELECT 1), `r` AS (SELECT 2), broken AS (SELECT FROM) SELECT 1",
            "r",
        ),
        (
            "WITH RECURSIVE R AS (SELECT 1), R AS (SELECT 2) DELETE",
            "R",
        ),
        (
            "UPDATE dst SET n=(WITH r AS (SELECT 1), r AS (SELECT 2) SELECT 1)",
            "r",
        ),
        (
            "WITH r AS (SELECT 1), r AS (WITH N AS (SELECT 1), N AS (SELECT 2) SELECT 1) SELECT 1",
            "N",
        ),
        (
            "WITH r AS (SELECT 1), r AS (SELECT 2), s AS (WITH N AS (SELECT 1), N AS (SELECT 2) SELECT 1) SELECT 1",
            "r",
        ),
    ] {
        assert_eq!(
            Parser::parse_sql(&MySqlDialect {}, sql).unwrap_err(),
            ParserError::MySqlDuplicateCte(name.into()),
            "{sql}"
        );
    }
    let malformed_first = Parser::parse_sql(
        &MySqlDialect {},
        "WITH r AS (SELECT FROM), r AS (SELECT 2) SELECT 1",
    )
    .unwrap_err();
    assert!(!matches!(
        malformed_first,
        ParserError::MySqlDuplicateCte(_)
    ));
    // The new control belongs solely to MySQL, not the portable parser.
    let sql = "WITH r AS (SELECT 1), r AS (SELECT 2) SELECT * FROM r";
    assert!(Parser::parse_sql(&crate::dialect::GenericDialect {}, sql).is_ok());
    assert!(Parser::parse_sql(&crate::dialect::PostgreSqlDialect {}, sql).is_ok());
}

#[test]
fn mysql_create_partitioning_uses_the_same_typed_payload_before_ctas_query() {
    for sql in [
        "CREATE TABLE t(id BIGINT PRIMARY KEY) PARTITION BY RANGE(id) (PARTITION p0 VALUES LESS THAN(10),PARTITION p1 VALUES LESS THAN MAXVALUE)",
        "CREATE TABLE t(id BIGINT PRIMARY KEY) PARTITION BY LINEAR HASH(id) PARTITIONS 4 AS SELECT id FROM src",
        "CREATE TABLE t(id BIGINT PRIMARY KEY) PARTITION BY KEY ALGORITHM=1(id) PARTITIONS 2 SELECT id FROM src",
        "CREATE TABLE t(a INT,b INT,PRIMARY KEY(a,b)) PARTITION BY RANGE COLUMNS(a) SUBPARTITION BY HASH(b) SUBPARTITIONS 2 (PARTITION p0 VALUES LESS THAN(10),PARTITION p1 VALUES LESS THAN MAXVALUE)",
    ] {
        let statements = Parser::parse_sql(&MySqlDialect {}, sql).unwrap();
        let [Statement::CreateTable(create)] = statements.as_slice() else {
            panic!("typed CREATE");
        };
        let partitioning = create
            .mysql_partitioning
            .as_ref()
            .expect("one actual typed partition issuer");
        assert!(
            create.partition_by.is_none(),
            "portable partition expression is independent"
        );
        assert_eq!(create.query.is_some(), sql.contains("SELECT"));
        let builder =
            crate::ast::helpers::stmt_create_table::CreateTableBuilder::from(create.clone());
        assert_eq!(&builder.mysql_partitioning, &create.mysql_partitioning);
        assert_eq!(builder.build(), *create);
        let rendered = create.to_string();
        let reparsed = Parser::parse_sql(&MySqlDialect {}, &rendered).unwrap();
        let [Statement::CreateTable(reparsed)] = reparsed.as_slice() else {
            panic!("typed roundtrip");
        };
        assert_eq!(&reparsed.mysql_partitioning, &Some(partitioning.clone()));
        if create.query.is_some() {
            assert!(rendered.find("PARTITION BY").unwrap() < rendered.find(" AS SELECT").unwrap());
        }
    }
    for sql in [
        "CREATE TABLE t(id INT) PARTITION BY RANGE(id)",
        "CREATE TABLE t(id INT) PARTITION BY HASH(id) PARTITIONS 0",
        "CREATE TABLE t(id INT) PARTITION BY KEY ALGORITHM=3(id) PARTITIONS 2",
        "CREATE TABLE t(id INT) PARTITION BY HASH(id) PARTITIONS 2 ASSELECT id FROM src",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
}

#[cfg(feature = "visitor")]
#[test]
fn mysql_create_partitioning_visits_actual_method_and_boundary_nodes() {
    use crate::ast::{Visit, VisitMut, Visitor, VisitorMut};
    use core::ops::ControlFlow;
    struct Nodes(Vec<String>);
    impl Visitor for Nodes {
        type Break = ();
        fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<()> {
            if let Expr::Identifier(ident) = expr {
                self.0.push(ident.value.clone());
            }
            ControlFlow::Continue(())
        }
    }
    impl VisitorMut for Nodes {
        type Break = ();
        fn pre_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<()> {
            if let Expr::Identifier(ident) = expr {
                ident.value.push_str("_bound");
            }
            ControlFlow::Continue(())
        }
    }
    let mut statements = Parser::parse_sql(
        &MySqlDialect {},
        "CREATE TABLE t(id INT) PARTITION BY RANGE(id) (PARTITION p0 VALUES LESS THAN(boundary))",
    )
    .unwrap();
    let mut before = Nodes(Vec::new());
    assert_eq!(
        Visit::visit(&statements, &mut before),
        ControlFlow::Continue(())
    );
    assert_eq!(before.0, ["id", "boundary"]);
    assert_eq!(
        VisitMut::visit(&mut statements, &mut Nodes(Vec::new())),
        ControlFlow::Continue(())
    );
    let mut after = Nodes(Vec::new());
    assert_eq!(
        Visit::visit(&statements, &mut after),
        ControlFlow::Continue(())
    );
    assert_eq!(after.0, ["id_bound", "boundary_bound"]);
}

#[test]
fn mysql_schema_read_only_is_alter_only_and_uses_native_literal_and_conflict_grammar() {
    use crate::parser::ParserError;
    for (sql, named, expected) in [
        ("ALTER DATABASE d READ ONLY 0", true, false),
        ("ALTER SCHEMA `d b` READ ONLY=1", true, true),
        ("ALTER DATABASE READ ONLY DEFAULT", false, false),
        ("ALTER SCHEMA READ ONLY 01 READ ONLY 1", false, true),
        (
            "ALTER DATABASE d READ ONLY 00 READ ONLY DEFAULT",
            true,
            false,
        ),
        (
            "ALTER DATABASE d CHARACTER SET latin1 READ ONLY 0 COLLATE latin1_bin",
            true,
            false,
        ),
    ] {
        let statements = Parser::parse_sql(&MySqlDialect {}, sql).unwrap();
        let [Statement::AlterSchema(alter)] = statements.as_slice() else {
            panic!("{sql}")
        };
        assert_eq!(alter.name.is_some(), named, "{sql}");
        assert!(!alter.if_exists);
        let [crate::ast::AlterSchemaOperation::SetOptionsParens {
            options: crate::ast::AlterSchemaSetOperation::MySqlDatabaseDefaults { read_only, .. },
        }] = alter.operations.as_slice()
        else {
            panic!("native options")
        };
        assert_eq!(*read_only, Some(expected), "{sql}");
        let displayed = statements[0].to_string();
        let reparsed = Parser::parse_sql(&MySqlDialect {}, &displayed).unwrap();
        assert_eq!(statements, reparsed);
    }
    for sql in [
        "ALTER DATABASE d READ ONLY 0 READ ONLY 1",
        "ALTER DATABASE d READ ONLY 1 READ ONLY DEFAULT",
    ] {
        assert_eq!(
            Parser::parse_sql(&MySqlDialect {}, sql).unwrap_err(),
            ParserError::MySqlConflictingSchemaReadOnly
        );
    }
    for sql in [
        "CREATE DATABASE d READ ONLY=1",
        "CREATE SCHEMA d READ ONLY=0",
        "ALTER DATABASE IF EXISTS d READ ONLY=1",
        "ALTER DATABASE d READ ONLY -1",
        "ALTER DATABASE d READ ONLY 2",
        "ALTER DATABASE d READ ONLY ON",
        "ALTER DATABASE d READ ONLY NULL",
        "ALTER DATABASE d READ ONLY ?",
        "ALTER DATABASE d READ ONLY 0+1",
        "ALTER DATABASE d READ ONLY 1.0",
        "ALTER DATABASE d READ ONLY (1)",
        "ALTER DATABASE d READ ONLY",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
    let portable = Parser::parse_sql(
        &crate::dialect::PostgreSqlDialect {},
        "ALTER SCHEMA s RENAME TO t",
    )
    .unwrap();
    let [Statement::AlterSchema(alter)] = portable.as_slice() else {
        panic!("portable schema")
    };
    assert_eq!(alter.name.as_ref().unwrap().to_string(), "s");
}

#[test]
fn mysql_hex_flavor_is_issued_only_by_its_lexer_without_changing_the_value_ast() {
    use crate::tokenizer::{MySqlHexLiteralKind, Token, Tokenizer};
    for (literal, value, flavor) in [
        ("0x0123", "0123", MySqlHexLiteralKind::HexNumber),
        ("X'0123'", "0123", MySqlHexLiteralKind::HexQuoted),
        ("0b1", "01", MySqlHexLiteralKind::BitNumber),
        ("b'1'", "01", MySqlHexLiteralKind::BitQuoted),
    ] {
        assert_eq!(
            Tokenizer::new(&MySqlDialect {}, literal)
                .tokenize()
                .unwrap(),
            vec![Token::MySqlHexStringLiteral(value.into(), flavor)]
        );
        let statement = Parser::parse_sql(&MySqlDialect {}, &format!("SELECT {literal}"))
            .unwrap()
            .remove(0);
        let Statement::Query(query) = statement else {
            panic!("query")
        };
        let SetExpr::Select(select) = *query.body else {
            panic!("select")
        };
        assert!(
            matches!(&select.projection[0], SelectItem::UnnamedExpr(Expr::Value(actual))
            if actual.value == crate::ast::Value::HexStringLiteral(value.into()))
        );
    }
    assert_eq!(
        Tokenizer::new(&crate::dialect::GenericDialect {}, "X'0123'")
            .tokenize()
            .unwrap(),
        vec![Token::HexStringLiteral("0123".into())]
    );
}

#[test]
fn mysql_at_time_zone_is_only_the_native_cast_datetime_production() {
    for sql in [
        "SELECT CAST(TIMESTAMP '2019-10-10 10:11:12' AT TIME ZONE 'UTC' AS DATETIME)",
        "SELECT CAST(a AT TIME ZONE INTERVAL '+00:00' AS DATETIME(6))",
        "SELECT CAST(NULL AT TIME ZONE '+00:00' AS DATETIME(0))",
    ] {
        let parsed = Parser::parse_sql(&MySqlDialect {}, sql).unwrap();
        assert_eq!(parsed.len(), 1);
        let Statement::Query(query) = &parsed[0] else {
            panic!("query")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("select")
        };
        let SelectItem::UnnamedExpr(Expr::Cast {
            expr, data_type, ..
        }) = &select.projection[0]
        else {
            panic!("cast")
        };
        assert!(matches!(data_type, DataType::Datetime(_)));
        let Expr::AtTimeZone { time_zone, .. } = expr.as_ref() else {
            panic!("native AT node")
        };
        assert!(matches!(
            time_zone.as_ref(),
            Expr::Value(_) | Expr::Interval(_)
        ));
    }
    for sql in [
        "SELECT a AT TIME ZONE 'UTC'",
        "SELECT CAST(a AT TIME ZONE x AS DATETIME)",
        "SELECT CAST(a AT TIME ZONE CONCAT('U','TC') AS DATETIME)",
        "SELECT CAST(a AT TIME ZONE 'UTC' AS TIME)",
        "SELECT CAST(a AT TIME ZONE 'UTC' AS TIMESTAMP)",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
    assert!(Parser::parse_sql(
        &crate::dialect::PostgreSqlDialect {},
        "SELECT a AT TIME ZONE 'UTC'"
    )
    .is_ok());
}

#[test]
fn show_indexes_preserves_components_overrides_and_native_where() {
    for spelling in ["INDEX", "INDEXES", "KEYS"] {
        let sql = format!(
            "SHOW EXTENDED {spelling} IN `first.db`.`table.dot` FROM `override.db` WHERE Key_name='PRIMARY'"
        );
        let statement = Parser::parse_sql(&MySqlDialect {}, &sql)
            .unwrap()
            .pop()
            .unwrap();
        let Statement::ShowIndexes {
            extended,
            table_name,
            database,
            filter,
        } = &statement
        else {
            panic!("{statement:?}")
        };
        assert!(*extended);
        assert_eq!(
            table_name
                .0
                .iter()
                .map(|part| part.as_ident().unwrap().value.as_str())
                .collect::<Vec<_>>(),
            ["first.db", "table.dot"]
        );
        assert_eq!(database.as_ref().unwrap().value, "override.db");
        assert!(filter.is_some());
        let rendered = statement.to_string();
        assert_eq!(
            Parser::parse_sql(&MySqlDialect {}, &rendered).unwrap()[0],
            statement
        );
    }
    for sql in [
        "SHOW INDEX",
        "SHOW INDEX FROM a.b.c",
        "SHOW INDEX FROM t LIKE '%'",
        "SHOW FULL INDEX FROM t",
        "SHOW INDEX FROM t FROM a.b",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
    let columns = Parser::parse_sql(
        &MySqlDialect {},
        "SHOW COLUMNS FROM `first.db`.`table.dot` FROM `override.db`",
    )
    .unwrap()
    .pop()
    .unwrap();
    let Statement::ShowColumns { show_options, .. } = columns else {
        panic!("columns")
    };
    let target = show_options.show_in.unwrap().parent_name.unwrap();
    assert_eq!(
        target
            .0
            .iter()
            .map(|part| part.as_ident().unwrap().value.as_str())
            .collect::<Vec<_>>(),
        ["override.db", "table.dot"]
    );
}

#[test]
fn schema_show_has_one_typed_database_and_filter_not_generated_sql() {
    use crate::ast::ShowSchemaMetadataKind;
    for (form, kind) in [
        ("TABLE STATUS", ShowSchemaMetadataKind::TableStatus),
        ("TRIGGERS", ShowSchemaMetadataKind::Triggers),
        ("EVENTS", ShowSchemaMetadataKind::Events),
        ("OPEN TABLES", ShowSchemaMetadataKind::OpenTables),
    ] {
        for suffix in [
            "",
            " FROM `Db.Dot` LIKE 'x%'",
            " IN `Db.Dot` WHERE Name IN (SELECT Name FROM other)",
        ] {
            let sql = format!("SHOW {form}{suffix}");
            let statement = Parser::parse_sql(&MySqlDialect {}, &sql)
                .unwrap()
                .pop()
                .unwrap();
            let Statement::ShowSchemaMetadata {
                kind: actual,
                database,
                ..
            } = &statement
            else {
                panic!("{statement:?}")
            };
            assert_eq!(*actual, kind);
            assert_eq!(
                database.as_ref().map(|ident| ident.value.as_str()),
                (!suffix.is_empty()).then_some("Db.Dot")
            );
            let rendered = statement.to_string();
            assert_eq!(
                Parser::parse_sql(&MySqlDialect {}, &rendered).unwrap()[0],
                statement
            );
        }
    }
    for sql in [
        "SHOW TABLE STATUS FROM a.b",
        "SHOW EVENTS ILIKE '%'",
        "SHOW OPEN TABLES FROM",
        "SHOW TRIGGERS FROM a FROM b",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
}

#[cfg(feature = "std")]
mod mysql_byte_source {
    use super::*;
    use crate::ast::{
        FunctionOrigin, MySqlText, SqlCppPiece, SqlSourceAdmission, SqlSourceOwner, Value,
    };
    use crate::parser::{MySqlParsedStatement, ParserError};
    use crate::tokenizer::{MySqlLexicalUnit, MySqlSourceAdapter, MySqlTokenCursor, Token};
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn saved_trigger_body_is_consumed_once_on_its_original_source_not_a_header_display() {
        use crate::ast::ConditionalStatements;
        use crate::keywords::Keyword;
        for (sql, body, expected_statements) in [
            (
                "CREATE TRIGGER mytrigger BEFORE INSERT ON mytable FOR EACH ROW BEGIN SET NEW.a = 1; SET NEW.b = 2; END",
                "BEGIN SET NEW.a = 1; SET NEW.b = 2; END",
                2,
            ),
            (
                "CREATE TRIGGER tr AFTER INSERT ON t1 FOR EACH ROW BEGIN INSERT INTO t2 VALUES (NEW.id); END",
                "BEGIN INSERT INTO t2 VALUES (NEW.id); END",
                1,
            ),
        ] {
            let facts = Facts::new("utf8mb4");
            {
                let owner = facts.owner(sql.as_bytes().to_vec());
                let calls = AtomicUsize::new(0);
                let (parsed, compiled) = Parser::new(&MySqlDialect {})
                    .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
                    .unwrap()
                    .parse_next_mysql_source_with_program_body(|parser| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        parser.parse_conditional_statements(&[Keyword::END])
                    })
                    .unwrap()
                    .unwrap();
                assert_eq!(calls.load(Ordering::SeqCst), 1);
                assert_eq!(parsed.consumed_end, sql.len());
                let Statement::CreateTrigger(trigger) = &parsed.statement else {
                    panic!("original CREATE TRIGGER header");
                };
                assert!(trigger.exec_body.is_none() && trigger.statements.is_none());
                let source = trigger
                    .mysql_body
                    .as_ref()
                    .unwrap()
                    .source
                    .as_ref()
                    .unwrap();
                assert_eq!(source.bytes().unwrap().trim_ascii(), body.as_bytes());
                assert!(Arc::ptr_eq(source.owner().unwrap(), &owner));
                let ConditionalStatements::BeginEnd(block) = compiled.unwrap() else {
                    panic!("actual body callback consumed the original block");
                };
                assert_eq!(block.statements.len(), expected_statements);
            }
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
        let sql = "CREATE TRIGGER t BEFORE INSERT ON tab FOR EACH ROW BEGIN SET NEW.a=1; END";
        let facts = Facts::new("utf8mb4");
        let owner = facts.owner(sql.as_bytes().to_vec());
        let without_compiler = Parser::new(&MySqlDialect {})
            .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
            .unwrap()
            .parse_next_mysql_source();
        assert!(
            without_compiler.is_err(),
            "raw headers cannot silently discard a saved body"
        );
        let invalid =
            "CREATE TRIGGER t BEFORE INSERT ON tab FOR EACH ROW EXECUTE FUNCTION emp_stamp()";
        let invalid_owner = facts.owner(invalid.as_bytes().to_vec());
        assert!(Parser::new(&MySqlDialect {})
            .try_with_mysql_source(invalid_owner.occurrence(0..invalid.len()).unwrap(), &facts)
            .unwrap()
            .parse_next_mysql_source_with_program_body(|parser| {
                parser.parse_conditional_statements(&[])
            })
            .is_err());
    }

    // Deliberately finite pinned-native fact fixtures. This is NOT a fallback
    // Unicode classifier or production charset authority. Native facade tests
    // independently establish the facts supplied by the real caller adapter.
    struct Facts {
        charset: &'static str,
        minimum: u8,
        limit: usize,
        system_text_limit: usize,
        system_text_calls: AtomicUsize,
        system_role_text_calls: AtomicUsize,
        alias_text_calls: AtomicUsize,
        system_text_live: Arc<AtomicUsize>,
        live: Arc<AtomicUsize>,
        units: Arc<AtomicUsize>,
        tail_probes: Arc<AtomicUsize>,
        refuse: Option<u8>,
        refuse_at_call: Option<usize>,
        failed: Arc<AtomicBool>,
        digest: Option<crate::tokenizer::MySqlDigestConfig>,
        digest_hash_calls: AtomicUsize,
    }
    struct Lease(Arc<AtomicUsize>);
    impl Drop for Lease {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    impl Facts {
        fn new(charset: &'static str) -> Self {
            Self {
                charset,
                minimum: 1,
                limit: usize::MAX,
                system_text_limit: usize::MAX,
                system_text_calls: AtomicUsize::new(0),
                system_role_text_calls: AtomicUsize::new(0),
                alias_text_calls: AtomicUsize::new(0),
                system_text_live: Arc::new(AtomicUsize::new(0)),
                live: Arc::new(AtomicUsize::new(0)),
                units: Arc::new(AtomicUsize::new(0)),
                tail_probes: Arc::new(AtomicUsize::new(0)),
                refuse: None,
                refuse_at_call: None,
                failed: Arc::new(AtomicBool::new(false)),
                digest: None,
                digest_hash_calls: AtomicUsize::new(0),
            }
        }
        fn owner(&self, bytes: Vec<u8>) -> Arc<SqlSourceOwner> {
            let admission = self.admit(bytes.capacity()).expect("source budget");
            SqlSourceOwner::adopt(bytes, admission)
        }
    }
    impl MySqlSourceAdapter for Facts {
        fn canonical_digest_config(&self) -> Option<crate::tokenizer::MySqlDigestConfig> {
            self.digest
        }
        fn canonical_digest_hash(&self, _: &[u8]) -> Option<[u8; 32]> {
            self.digest_hash_calls.fetch_add(1, Ordering::SeqCst);
            None
        }
        fn client_charset(&self) -> &str {
            self.charset
        }
        fn client_min_width(&self) -> u8 {
            self.minimum
        }
        fn lexical_unit(
            &self,
            bytes: &[u8],
        ) -> Result<MySqlLexicalUnit, crate::tokenizer::MySqlSourceError> {
            let call = self.units.fetch_add(1, Ordering::SeqCst) + 1;
            if self.refuse_at_call == Some(call) {
                self.failed.store(true, Ordering::SeqCst);
                return Err("adapter failed before cancellation".into());
            }
            let byte = *bytes.first().ok_or("missing fact")?;
            if self.refuse == Some(byte) {
                return Err("precise mid-source adapter refusal".into());
            }
            if byte == 0xfd {
                self.tail_probes.fetch_add(1, Ordering::SeqCst);
                return Err("tail was actually lexed".into());
            }
            let width = if self.charset == "gbk" && bytes.starts_with(&[0x81, 0x5c]) {
                2
            } else if self.charset == "utf8mb4" && bytes.starts_with(&[0xc3, 0xa9]) {
                2
            } else if self.charset == "utf8mb4" && bytes.starts_with(&[0xf0, 0x9f, 0x98, 0x80]) {
                4
            } else {
                1
            };
            let ascii = byte <= 127;
            let valid = ascii || self.charset == "latin1" || width > 1;
            Ok(MySqlLexicalUnit {
                width,
                expected_width: width,
                state: 0,
                valid,
                ascii,
                space: matches!(byte, 9..=13 | 32),
                control: byte < 32 || byte == 127,
                identifier: byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$') || !ascii,
            })
        }
        fn system_identifier(
            &self,
            bytes: &[u8],
            _: bool,
        ) -> Result<(String, SqlSourceAdmission), crate::tokenizer::MySqlSourceError> {
            let capacity = if self.charset == "latin1" {
                bytes.len().checked_mul(2).ok_or("fixture capacity")?
            } else {
                bytes.len()
            };
            let admission = self.admit(capacity)?;
            let value = if self.charset == "latin1" {
                let mut value = String::with_capacity(capacity);
                value.extend(bytes.iter().map(|byte| char::from(*byte)));
                value
            } else {
                String::from_utf8(bytes.to_vec())
                    .map_err(|_| "native IDENT_sys fixture rejects malformed system bytes")?
            };
            Ok((value, admission))
        }
        fn system_text(
            &self,
            text: &MySqlText,
        ) -> Result<(String, SqlSourceAdmission), crate::tokenizer::MySqlSourceError> {
            self.system_text_calls.fetch_add(1, Ordering::SeqCst);
            if text.role() == crate::ast::MySqlTextRole::System {
                self.system_role_text_calls.fetch_add(1, Ordering::SeqCst);
            }
            let bytes = text.bytes()?;
            let capacity = if self.charset == "latin1" {
                bytes.len().checked_mul(2).ok_or("fixture capacity")?
            } else {
                bytes.len()
            };
            if capacity > self.system_text_limit {
                return Err("system text budget refused before conversion".into());
            }
            let (value, admission) = self.system_identifier(text.bytes()?, false)?;
            // Observe this original conversion lease without replacing its
            // admitted source or issuing any additional parser capability.
            self.system_text_live.fetch_add(1, Ordering::SeqCst);
            Ok((
                value,
                SqlSourceAdmission::new(Arc::new((
                    admission,
                    Lease(self.system_text_live.clone()),
                ))),
            ))
        }
        fn system_text_value(
            &self,
            text: &MySqlText,
        ) -> Result<(Vec<u8>, SqlSourceAdmission), crate::tokenizer::MySqlSourceError> {
            // The same finite test adapter and its original once-conversion
            // counters; production conversion is independently native-backed.
            let (value, admission) = self.system_text(text)?;
            Ok((value.into_bytes(), admission))
        }
        fn projection_alias_text(
            &self,
            text: &MySqlText,
        ) -> Result<
            (Vec<u8>, SqlSourceAdmission),
            (
                crate::tokenizer::MySqlSourceError,
                Option<SqlSourceAdmission>,
            ),
        > {
            self.alias_text_calls.fetch_add(1, Ordering::SeqCst);
            let result = (|| {
                if text.role() != crate::ast::MySqlTextRole::Alias {
                    return Err("fixture alias lost its actual reduction".into());
                }
                let bytes = text.bytes()?;
                // Pinned fixture: the native system charset does not perform
                // a second validation/conversion, including malformed bytes.
                if self.charset == "utf8mb3" {
                    return Ok((bytes.to_vec(), self.admit(bytes.len())?));
                }
                let (value, admission) = self.system_identifier(bytes, false)?;
                Ok((value.into_bytes(), admission))
            })();
            result.map_err(|error| (error, None))
        }
        fn admit(
            &self,
            capacity: usize,
        ) -> Result<SqlSourceAdmission, crate::tokenizer::MySqlSourceError> {
            if capacity > self.limit {
                return Err("source budget refused before allocation".into());
            }
            self.live.fetch_add(1, Ordering::SeqCst);
            Ok(SqlSourceAdmission::new(Arc::new(Lease(self.live.clone()))))
        }
    }
    fn parsed(bytes: &[u8], facts: &Facts, ansi: bool, no_escape: bool) -> MySqlParsedStatement {
        let owner = facts.owner(bytes.to_vec());
        let dialect = MySqlDialect::with_options(ansi, false, no_escape);
        Parser::new(&dialect)
            .try_with_mysql_source(owner.occurrence(0..bytes.len()).unwrap(), facts)
            .unwrap()
            .parse_next_mysql_source()
            .unwrap()
            .expect("actual statement")
    }

    fn digest_facts() -> Facts {
        let mut facts = Facts::new("utf8mb4");
        facts.digest = Some(crate::tokenizer::MySqlDigestConfig {
            capacity: 1024,
            charset_number: 45,
            ignore_space: false,
            high_not_precedence: false,
            pipes_as_concat: false,
        });
        facts
    }

    #[test]
    fn raw_component_literals_use_the_admitted_system_text_and_never_identifiers() {
        for (sql, ansi, no_escape, install, expected) in [
            (
                b"INSTALL COMPONENT 'a','b''c'".as_slice(),
                false,
                false,
                true,
                vec!["a", "b'c"],
            ),
            (
                b"UNINSTALL COMPONENT 'file://\xe9'",
                false,
                false,
                false,
                vec!["file://é"],
            ),
            (b"INSTALL COMPONENT \"a\"", false, false, true, vec!["a"]),
            (
                br"INSTALL COMPONENT 'a\nb'",
                false,
                false,
                true,
                vec!["a\nb"],
            ),
            (
                br"INSTALL COMPONENT 'a\nb'",
                false,
                true,
                true,
                vec![r"a\nb"],
            ),
            (b"INSTALL COMPONENT 'a'", true, false, true, vec!["a"]),
        ] {
            let facts = Facts::new("latin1");
            let result = parsed(sql, &facts, ansi, no_escape);
            assert!(
                matches!(&result.statement, Statement::MySqlComponent { install: actual, urns }
                if *actual == install && urns.iter().map(String::as_str).collect::<Vec<_>>() == expected)
            );
            assert_eq!(result.source_occurrence.bytes().unwrap(), sql);
            assert_eq!(result.consumed_end, sql.len());
            assert!(facts.live.load(Ordering::SeqCst) > 0);
            drop(result);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
        for (sql, ansi) in [
            (b"INSTALL COMPONENT bare".as_slice(), false),
            (b"INSTALL COMPONENT `quoted`", false),
            (b"INSTALL COMPONENT \"quoted\"", true),
            (b"UNINSTALL COMPONENT @urn", false),
            (b"INSTALL COMPONENT ?", false),
            (b"INSTALL COMPONENT 'a',identifier", false),
        ] {
            let facts = Facts::new("latin1");
            let owner = facts.owner(sql.to_vec());
            let dialect = MySqlDialect::with_options(ansi, false, false);
            let result = Parser::new(&dialect)
                .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
                .unwrap()
                .parse_next_mysql_source();
            assert!(result.is_err(), "{sql:?}");
            drop((result, owner));
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn raw_component_system_conversion_obeys_the_original_adapter_admission() {
        let mut facts = Facts::new("latin1");
        let mut sql = b"INSTALL COMPONENT '".to_vec();
        sql.extend_from_slice(&[0xe9; 64]);
        sql.push(b'\'');
        let owner = facts.owner(sql);
        // Source is already admitted; the actual system conversion requires
        // 128 bytes and must fail its own admission rather than accept IDENT.
        facts.system_text_limit = owner.bytes().len();
        let result = Parser::new(&MySqlDialect {})
            .try_with_mysql_source(owner.occurrence(0..owner.bytes().len()).unwrap(), &facts)
            .unwrap()
            .parse_next_mysql_source();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("system text budget refused before conversion"));
        drop(owner);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }
    fn digest_tokens(parsed: &MySqlParsedStatement) -> &[u8] {
        let Some(crate::tokenizer::MySqlCanonicalDigestReceipt::Collected(digest)) =
            parsed.source_occurrence.canonical_digest()
        else {
            panic!("successful actual statement owns admitted token facts");
        };
        assert!(
            digest.hash().is_none(),
            "fixture does not impersonate native hashing"
        );
        digest.token_bytes()
    }

    fn canonical_identifier(wire: &mut Vec<u8>, name: &[u8]) {
        wire.extend(1106u16.to_le_bytes());
        wire.extend((name.len() as u16).to_le_bytes());
        wire.extend(name);
    }

    #[test]
    fn canonical_native_user_and_system_roles_stay_on_actual_successful_source() {
        for (sql, tokens) in [
            (b"SELECT @a".as_slice(), vec![748u16, 64, 1100]),
            (b"SELECT @a.b", vec![748, 64, 1100]),
            (b"SELECT @'a.b'", vec![748, 64, 1100]),
            (b"SELECT @'@name'", vec![748, 64, 1100]),
            (b"SET @a:=1", vec![756, 64, 1100, 757, 1100]),
        ] {
            let facts = digest_facts();
            let result = parsed(sql, &facts, false, false);
            let expected = tokens
                .iter()
                .flat_map(|token| token.to_le_bytes())
                .collect::<Vec<_>>();
            assert_eq!(digest_tokens(&result), expected, "{:?}", sql);
            assert_eq!(result.source_occurrence.bytes().unwrap(), sql);
            assert_eq!(facts.digest_hash_calls.load(Ordering::SeqCst), 1);
            let clone = result.source_occurrence.clone();
            drop(result);
            assert!(facts.live.load(Ordering::SeqCst) > 0);
            drop(clone);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
        let facts = digest_facts();
        let sql = b"SELECT @@warning_count";
        let result = parsed(sql, &facts, false, false);
        let mut expected = [748u16, 64, 64]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        canonical_identifier(&mut expected, b"warning_count");
        assert_eq!(digest_tokens(&result), expected);
        drop(result);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn canonical_charset_identifier_bytes_and_raw_role_unavailability_are_distinct() {
        let mut facts = digest_facts();
        facts.charset = "latin1";
        facts.digest.as_mut().unwrap().charset_number = 8;
        let sql = b"SELECT \xe9.\xe9";
        let result = parsed(sql, &facts, false, false);
        let mut expected = 748u16.to_le_bytes().to_vec();
        canonical_identifier(&mut expected, &[0xe9]);
        expected.extend(46u16.to_le_bytes());
        canonical_identifier(&mut expected, &[0xe9]);
        assert_eq!(digest_tokens(&result), expected);
        assert_eq!(result.source_occurrence.bytes().unwrap(), sql);
        drop(result);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        // The same admitted multibyte identifier is not native HOSTNAME byte
        // ctype proof. Refusal affects observation, not its actual SQL parsing.
        let result = parsed(b"SELECT @\xe9", &facts, false, false);
        let Some(crate::tokenizer::MySqlCanonicalDigestReceipt::Collected(digest)) =
            result.source_occurrence.canonical_digest()
        else {
            panic!("actual admitted Source")
        };
        assert_eq!(
            digest.unavailable(),
            Some(crate::tokenizer::MySqlDigestUnavailable::VariableRole)
        );
        assert!(digest.hash().is_none());
        drop(result);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn canonical_empty_and_ignored_hints_do_not_erase_native_tokens_or_issue_hint_authority() {
        for sql in [
            b"SELECT /*+ */ 1".as_slice(),
            b"SELECT /**/ /*+ MAX_EXECUTION_TIME(25) */ 1",
            b"SELECT 1 /*+ MAX_EXECUTION_TIME(41) */",
        ] {
            let facts = digest_facts();
            let result = parsed(sql, &facts, false, false);
            assert_eq!(
                digest_tokens(&result),
                [748u16, 1100]
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>()
            );
            assert_eq!(result.source_occurrence.bytes().unwrap(), sql);
            assert_eq!(facts.digest_hash_calls.load(Ordering::SeqCst), 1);
            drop(result);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
        let facts = digest_facts();
        let result = parsed(
            b"SELECT /*+ MAX_EXECUTION_TIME(25) */ 1",
            &facts,
            false,
            false,
        );
        let Some(crate::tokenizer::MySqlCanonicalDigestReceipt::Collected(digest)) =
            result.source_occurrence.canonical_digest()
        else {
            panic!("actual Source")
        };
        assert_eq!(
            digest.unavailable(),
            Some(crate::tokenizer::MySqlDigestUnavailable::Hint)
        );
        assert_eq!(facts.digest_hash_calls.load(Ordering::SeqCst), 0);
        drop(result);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn canonical_null_is_a_committed_literal_not_an_is_null_keyword_rewrite() {
        for (sql, tokens) in [
            (b"SELECT NULL".as_slice(), vec![748u16, 1100]),
            (b"SELECT NULL,NULL", vec![748, 1101]),
            (b"SELECT NULL IS NULL", vec![748, 1100, 509, 627]),
        ] {
            let facts = digest_facts();
            let parsed = parsed(sql, &facts, false, false);
            let expected = tokens
                .iter()
                .flat_map(|token| token.to_le_bytes())
                .collect::<Vec<_>>();
            assert_eq!(digest_tokens(&parsed), expected, "{:?}", sql);
            assert_eq!(facts.digest_hash_calls.load(Ordering::SeqCst), 1);
            assert_eq!(parsed.source_occurrence.bytes().unwrap(), sql);
            drop(parsed);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
        for (sql, tokens) in [
            (b"SELECT (NULL)".as_slice(), vec![748u16, 1102]),
            (b"SELECT ((NULL))", vec![748, 40, 1102, 41]),
            (b"SELECT (NULL,NULL)", vec![748, 1104]),
            (b"SELECT (NULL),(NULL)", vec![748, 1103]),
            (b"SELECT (NULL,1),(2,NULL)", vec![748, 1105]),
            (b"SELECT NULL IN (NULL)", vec![748, 1100, 1110]),
            (b"SELECT NULL IN (NULL,NULL)", vec![748, 1100, 1110]),
            (b"SELECT (1 IS NULL)", vec![748, 40, 1100, 509, 627, 41]),
        ] {
            let facts = digest_facts();
            let result = parsed(sql, &facts, false, false);
            let expected = tokens
                .iter()
                .flat_map(|token| token.to_le_bytes())
                .collect::<Vec<_>>();
            assert_eq!(digest_tokens(&result), expected, "{:?}", sql);
            assert_eq!(result.source_occurrence.bytes().unwrap(), sql);
            assert_eq!(facts.digest_hash_calls.load(Ordering::SeqCst), 1);
            drop(result);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn canonical_null_real_alias_union_and_capacity_keep_native_default_reduce_phase() {
        for (sql, prefix) in [
            (b"SELECT NULL n".as_slice(), vec![748u16, 1100]),
            (b"SELECT NULL AS n", vec![748, 1100, 276]),
            (b"SELECT (NULL) AS n", vec![748, 1102, 276]),
        ] {
            let facts = digest_facts();
            let result = parsed(sql, &facts, false, false);
            let mut expected = prefix
                .iter()
                .flat_map(|token| token.to_le_bytes())
                .collect();
            canonical_identifier(&mut expected, b"n");
            assert_eq!(digest_tokens(&result), expected, "{:?}", sql);
            assert_eq!(facts.digest_hash_calls.load(Ordering::SeqCst), 1);
            drop(result);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
        let facts = digest_facts();
        let sql = b"SELECT NULL UNION ALL SELECT (NULL)";
        let result = parsed(sql, &facts, false, false);
        assert_eq!(
            digest_tokens(&result),
            [748u16, 1100, 858, 268, 748, 1102]
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
        );
        drop(result);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        for (capacity, tokens, full) in [
            (1, vec![], true),
            (2, vec![748u16], true),
            (3, vec![748], true),
            (4, vec![748, 40], true),
            (5, vec![748, 40], true),
            (6, vec![748, 1102], false),
            (7, vec![748, 1102], false),
            (8, vec![748, 1102], false),
        ] {
            let mut facts = digest_facts();
            facts.digest.as_mut().unwrap().capacity = capacity;
            let result = parsed(b"SELECT (NULL)", &facts, false, false);
            assert_eq!(
                digest_tokens(&result),
                tokens
                    .iter()
                    .flat_map(|token| token.to_le_bytes())
                    .collect::<Vec<_>>()
            );
            let Some(crate::tokenizer::MySqlCanonicalDigestReceipt::Collected(digest)) =
                result.source_occurrence.canonical_digest()
            else {
                panic!("same original capacity admission");
            };
            assert_eq!(digest.is_full(), full);
            assert_eq!(facts.digest_hash_calls.load(Ordering::SeqCst), 1);
            drop(result);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn canonical_success_receipt_stays_with_six_part_source_and_not_subrange_or_reattach() {
        let facts = digest_facts();
        let parsed = parsed(b"SELECT 1", &facts, false, false);
        let owner = parsed.source_occurrence.owner().unwrap().clone();
        let calls = facts.units.load(Ordering::SeqCst);
        let mapped = parsed.map_body(Some);
        let (_, mut occurrence, cpp, admissions, container, secrets) = mapped.into_owned_parts();
        assert!(occurrence.canonical_digest().is_some());
        let clone = occurrence.clone();
        let (
            Some(crate::tokenizer::MySqlCanonicalDigestReceipt::Collected(first)),
            Some(crate::tokenizer::MySqlCanonicalDigestReceipt::Collected(second)),
        ) = (occurrence.canonical_digest(), clone.canonical_digest())
        else {
            panic!("same retained receipt");
        };
        assert!(Arc::ptr_eq(first, second));
        assert!(occurrence.slice(0..1).unwrap().canonical_digest().is_none());
        assert_eq!(
            facts.units.load(Ordering::SeqCst),
            calls,
            "borrowing does not relex"
        );
        occurrence.reattach(&owner).unwrap();
        assert!(
            occurrence.canonical_digest().is_none(),
            "catalog reattachment is not a parse"
        );
        drop((
            occurrence, clone, cpp, admissions, container, secrets, owner,
        ));
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn canonical_failed_grammar_or_later_byte_error_never_calls_hash_or_exposes_receipt() {
        for sql in [
            b"SELECT (".as_slice(),
            b"SELECT (NULL",
            b"SELECT (NULL) garbage garbage",
            b"SELECT 'unterminated",
            b"SELECT 1 garbage garbage",
        ] {
            let facts = digest_facts();
            let owner = facts.owner(sql.to_vec());
            let dialect = MySqlDialect {};
            let result = Parser::new(&dialect)
                .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
                .unwrap()
                .parse_next_mysql_source();
            assert!(result.is_err(), "{:?}", sql);
            assert_eq!(facts.digest_hash_calls.load(Ordering::SeqCst), 0);
            drop((result, owner));
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn canonical_lazy_tail_and_independent_occurrence_do_not_reissue_token_callbacks() {
        let facts = digest_facts();
        let sql = b"SELECT 1; SELECT 2";
        let owner = facts.owner(sql.to_vec());
        let dialect = MySqlDialect {};
        let first = Parser::new(&dialect)
            .with_mysql_multi_statements(true)
            .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
            .unwrap()
            .parse_next_mysql_source()
            .unwrap()
            .unwrap();
        assert_eq!(first.consumed_end, 9);
        assert_eq!(facts.digest_hash_calls.load(Ordering::SeqCst), 1);
        let second = Parser::new(&dialect)
            .with_mysql_multi_statements(true)
            .try_with_mysql_source(
                owner.occurrence(first.consumed_end..sql.len()).unwrap(),
                &facts,
            )
            .unwrap()
            .parse_next_mysql_source()
            .unwrap()
            .unwrap();
        assert_eq!(digest_tokens(&first), digest_tokens(&second));
        let (
            Some(crate::tokenizer::MySqlCanonicalDigestReceipt::Collected(a)),
            Some(crate::tokenizer::MySqlCanonicalDigestReceipt::Collected(b)),
        ) = (
            first.source_occurrence.canonical_digest(),
            second.source_occurrence.canonical_digest(),
        )
        else {
            panic!("actual siblings");
        };
        assert!(!Arc::ptr_eq(a, b));
        assert_eq!(facts.digest_hash_calls.load(Ordering::SeqCst), 2);
        drop((first, second, owner));
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn canonical_receipt_requires_statement_success_not_comment_eof_or_expression_role() {
        for sql in [
            b"".as_slice(),
            b" /* actual comment */ ",
            b";;; /* actual comment */",
        ] {
            let facts = digest_facts();
            let owner = facts.owner(sql.to_vec());
            let dialect = MySqlDialect {};
            let result = Parser::new(&dialect)
                .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
                .unwrap()
                .parse_next_mysql_source()
                .unwrap();
            assert!(result.is_none());
            assert_eq!(facts.digest_hash_calls.load(Ordering::SeqCst), 0);
            drop(owner);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
        let facts = digest_facts();
        let result = parsed(b";; SELECT 1", &facts, false, false);
        assert_eq!(
            digest_tokens(&result),
            [748u16, 1100]
                .iter()
                .flat_map(|t| t.to_le_bytes())
                .collect::<Vec<_>>()
        );
        drop(result);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        let sql = b"1 + 2";
        let owner = facts.owner(sql.to_vec());
        let dialect = MySqlDialect {};
        let mut parser = Parser::new(&dialect)
            .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
            .unwrap();
        parser.parse_expr().unwrap();
        assert!(parser
            .tokens
            .raw()
            .unwrap()
            .cursor
            .borrow_mut()
            .finish_canonical_digest()
            .unwrap()
            .is_none());
        drop((parser, owner));
        assert_eq!(
            facts.digest_hash_calls.load(Ordering::SeqCst),
            1,
            "only successful statement hashed"
        );
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn canonical_serde_transports_ranges_only_and_configuration_full_does_not_abort_sql() {
        let mut facts = digest_facts();
        facts.digest.as_mut().unwrap().capacity = 2;
        let result = parsed(b"SELECT 1 FROM t", &facts, false, false);
        let Some(crate::tokenizer::MySqlCanonicalDigestReceipt::Collected(digest)) =
            result.source_occurrence.canonical_digest()
        else {
            panic!("actual full receipt");
        };
        assert!(digest.is_full());
        assert_eq!(digest.token_bytes(), 748u16.to_le_bytes());
        assert!(matches!(&result.statement, Statement::Query(_)));
        #[cfg(feature = "serde")]
        {
            let bytes = serde_json::to_vec(&result.source_occurrence).unwrap();
            let detached: crate::ast::SqlSourceOccurrence = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(detached.range(), result.source_occurrence.range());
            assert!(detached.owner().is_none());
            assert!(detached.canonical_digest().is_none());
        }
        drop(result);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn raw_account_identity_uses_system_text_without_opening_identifier_grammar() {
        use crate::ast::GranteeName;
        for (bytes, ansi, no_escape, user, host, quoted) in [
            (
                b"'mysql.sys'@'localhost'".as_slice(),
                false,
                false,
                "mysql.sys",
                "localhost",
                Some('\''),
            ),
            (
                b"'user.dot'@'host.dot'".as_slice(),
                false,
                false,
                "user.dot",
                "host.dot",
                Some('\''),
            ),
            (
                b"'CURRENT_USER'@'localhost'".as_slice(),
                false,
                false,
                "CURRENT_USER",
                "localhost",
                Some('\''),
            ),
            (
                b"'u\\n'@'h'".as_slice(),
                false,
                false,
                "u\n",
                "h",
                Some('\''),
            ),
            (
                b"'u\\n'@'h'".as_slice(),
                false,
                true,
                "u\\n",
                "h",
                Some('\''),
            ),
            (b"\"u\"@\"h\"".as_slice(), false, false, "u", "h", Some('"')),
            (b"\"u\"@\"h\"".as_slice(), true, false, "u", "h", Some('"')),
            (b"'\xE9'@'h'".as_slice(), false, false, "é", "h", Some('\'')),
        ] {
            let facts = Facts::new("latin1");
            let owner = facts.owner(bytes.to_vec());
            let dialect = MySqlDialect::with_options(ansi, false, no_escape);
            let mut parser = Parser::new(&dialect)
                .try_with_mysql_source(owner.occurrence(0..bytes.len()).unwrap(), &facts)
                .unwrap();
            let GranteeName::UserHost {
                user: actual_user,
                host: actual_host,
            } = parser.parse_grantee_name().unwrap()
            else {
                panic!("account pair")
            };
            assert_eq!(actual_user.value, user);
            assert_eq!(actual_host.value, host);
            assert_eq!(actual_user.quote_style, quoted);
            assert!(matches!(parser.peek_token_ref().token, Token::EOF));
            drop(parser);
            drop(owner);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
        for sql in ["CURRENT_USER", "CURRENT_USER()"] {
            let facts = Facts::new("latin1");
            let owner = facts.owner(sql.as_bytes().to_vec());
            let mut parser = Parser::new(&MySqlDialect {})
                .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
                .unwrap();
            let GranteeName::ObjectName(name) = parser.parse_grantee_name().unwrap() else {
                panic!("current user grammar marker")
            };
            assert_eq!(name.0[0].as_ident().unwrap().quote_style, None);
            assert_eq!(name.0[0].as_ident().unwrap().value, "CURRENT_USER");
            assert!(matches!(parser.peek_token_ref().token, Token::EOF));
        }
        for bytes in [
            b"'not_an_identifier'".as_slice(),
            b"'first' 'second'",
            b"N'user'",
            b"_latin1'user'",
            b"@'user'",
        ] {
            let facts = Facts::new("latin1");
            let owner = facts.owner(bytes.to_vec());
            let mut parser = Parser::new(&MySqlDialect {})
                .try_with_mysql_source(owner.occurrence(0..bytes.len()).unwrap(), &facts)
                .unwrap();
            let identifier = parser.parse_identifier();
            assert!(
                identifier.is_err() || !matches!(parser.peek_token_ref().token, Token::EOF),
                "{bytes:?}"
            );
        }
        for bytes in [
            b"'first' 'second'".as_slice(),
            b"N'user'",
            b"_latin1'user'",
            b"@'user'",
            b"?",
            b"CURRENT_USER@'host'",
        ] {
            let facts = Facts::new("latin1");
            let owner = facts.owner(bytes.to_vec());
            let mut parser = Parser::new(&MySqlDialect {})
                .try_with_mysql_source(owner.occurrence(0..bytes.len()).unwrap(), &facts)
                .unwrap();
            let result = parser.parse_grantee_name();
            assert!(
                result.is_err() || !matches!(parser.peek_token_ref().token, Token::EOF),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn raw_account_statement_consumers_share_the_actual_literal_decoder() {
        for sql in [
            "CREATE DEFINER='mysql.sys'@'localhost' VIEW v AS SELECT 1",
            "GRANT SELECT ON t TO 'user.dot'@'host.dot'",
            "REVOKE SELECT ON t FROM 'user.dot'@'host.dot'",
            "REVOKE ALL, GRANT OPTION FROM 'CURRENT_USER'@'localhost', CURRENT_USER()",
            "CREATE USER 'user.dot'",
            "ALTER USER 'user.dot'",
            "CREATE ROLE 'role.dot'",
            "DROP USER 'user.dot'",
            "DROP ROLE 'role.dot'",
            "SET ROLE 'role.dot'",
            "GRANT SELECT ON t TO 'user.dot'@'host.dot' AS 'grantor.dot'",
        ] {
            // Independent parses/reparses retain identity and CPP; they do not
            // manufacture a new literal Source or change account permissions.
            let facts = Facts::new("latin1");
            let first = parsed(sql.as_bytes(), &facts, false, false);
            let second = parsed(sql.as_bytes(), &facts, false, false);
            assert_eq!(first.statement, second.statement, "{sql}");
            assert_eq!(cpp(&first), sql.as_bytes(), "{sql}");
            assert_eq!(cpp(&second), sql.as_bytes(), "{sql}");
        }
    }

    #[test]
    fn raw_introducer_is_on_the_original_head_before_adjacent_reduction() {
        let facts = Facts::new("utf8mb4");
        let result = parsed(b"SELECT _latin1'a' 'b' 'c'", &facts, false, false);
        let SelectItem::MySqlProjection {
            expr: Expr::Prefixed { prefix, value },
            ..
        } = &projection(&result)[0]
        else {
            panic!("introduced projection")
        };
        let literal = text(value);
        assert_eq!(prefix.value, "_latin1");
        assert_eq!(literal.introducer(), Some("_latin1"));
        assert_eq!(literal.bytes().unwrap(), b"abc");
        assert_eq!(literal.segments().len(), 3);
        assert_eq!(literal.segments()[0].introducer(), Some("_latin1"));
        assert_eq!(literal.segments()[1].introducer(), None);
        assert_eq!(literal.segments()[2].introducer(), None);
        assert_eq!(literal.segments()[0].charset(), "utf8mb4");
        assert!(
            literal.clone().with_introducer("_latin1".into()).is_err(),
            "an already reduced value must not be re-signed"
        );
    }

    fn projection(parsed: &MySqlParsedStatement) -> &[SelectItem] {
        let Statement::Query(query) = &parsed.statement else {
            panic!("query")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("select")
        };
        &select.projection
    }
    fn text(expr: &Expr) -> &MySqlText {
        let Expr::Value(value) = expr else {
            panic!("value: {expr:?}")
        };
        let Value::MySqlText(text) = &value.value else {
            panic!("raw value")
        };
        text
    }
    fn cpp(parsed: &MySqlParsedStatement) -> Vec<u8> {
        let mut bytes = Vec::new();
        for piece in &parsed.cpp {
            match piece {
                SqlCppPiece::Source(source) => bytes.extend_from_slice(source.bytes().unwrap()),
                SqlCppPiece::Space => bytes.push(b' '),
            }
        }
        bytes
    }

    #[test]
    fn raw_adapter_errors_never_become_numeric_eof_or_different_parse_errors() {
        for sql in [
            b"SELECT 123456789".as_slice(),
            b"SELECT 0x123456789",
            b"SELECT 0b111111109",
            b"SELECT 123456789name",
        ] {
            let mut facts = Facts::new("utf8mb4");
            facts.refuse = Some(b'9');
            let owner = facts.owner(sql.to_vec());
            let error = Parser::new(&MySqlDialect {})
                .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
                .unwrap()
                .parse_next_mysql_source()
                .err()
                .unwrap();
            assert!(
                error
                    .to_string()
                    .contains("precise mid-source adapter refusal"),
                "{error}"
            );
            drop(owner);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn raw_first_adapter_error_is_not_replaced_by_a_later_cancel_probe() {
        let mut facts = Facts::new("utf8mb4");
        // Each actual unit_at calls this finite fixture once. At1023 the
        // adapter fails;1024 would otherwise discover the later cancellation.
        facts.refuse_at_call = Some(1023);
        let mut sql = b"SELECT 0x".to_vec();
        sql.extend(core::iter::repeat_n(b'1', 100_000));
        let owner = facts.owner(sql);
        let failed = facts.failed.clone();
        let error = Parser::new(&MySqlDialect {})
            .with_cancellation(Arc::new(move || failed.load(Ordering::SeqCst)))
            .try_with_mysql_source(owner.occurrence(0..owner.bytes().len()).unwrap(), &facts)
            .unwrap()
            .parse_next_mysql_source()
            .err()
            .unwrap();
        assert!(facts.failed.load(Ordering::SeqCst));
        assert!(!matches!(error, ParserError::Cancelled));
        assert!(
            error
                .to_string()
                .contains("adapter failed before cancellation"),
            "{error}"
        );
        assert_eq!(facts.units.load(Ordering::SeqCst), 1023);
        drop(owner);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn raw_cursor_retries_keep_the_first_adapter_error_without_more_probes() {
        let mut facts = Facts::new("utf8mb4");
        facts.refuse_at_call = Some(1);
        let owner = facts.owner(b"SELECT 1".to_vec());
        let failed = facts.failed.clone();
        let dialect = MySqlDialect {};
        let mut cursor = MySqlTokenCursor::new(
            &dialect,
            owner.occurrence(0..owner.bytes().len()).unwrap(),
            &facts,
            Some(Arc::new(move || failed.load(Ordering::SeqCst))),
        )
        .unwrap();
        let first = cursor.next_token().err().unwrap();
        let second = cursor.next_token().err().unwrap();
        assert_eq!(first.message, "adapter failed before cancellation");
        assert_eq!(second.message, first.message);
        assert_eq!(facts.units.load(Ordering::SeqCst), 1);
        drop(cursor);
        drop(owner);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn raw_numeric_candidates_and_fallback_identifiers_use_the_same_cancel_probe() {
        for prefix in [
            b"SELECT 0x".as_slice(),
            b"SELECT 0b",
            b"SELECT 0xg",
            b"SELECT 123name",
        ] {
            let facts = Facts::new("utf8mb4");
            let mut sql = prefix.to_vec();
            sql.extend(core::iter::repeat_n(b'1', 100_000));
            let owner = facts.owner(sql);
            let units = facts.units.clone();
            let error = Parser::new(&MySqlDialect {})
                .with_cancellation(Arc::new(move || units.load(Ordering::SeqCst) >= 2048))
                .try_with_mysql_source(owner.occurrence(0..owner.bytes().len()).unwrap(), &facts)
                .unwrap()
                .parse_next_mysql_source()
                .err()
                .unwrap();
            assert!(matches!(error, ParserError::Cancelled), "{error}");
            assert!(facts.units.load(Ordering::SeqCst) < 8192);
            drop(owner);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn raw_literals_keep_source_bytes_charset_and_native_escape_semantics() {
        for (charset, bytes) in [
            ("latin1", vec![0xe9]),
            ("utf8mb4", vec![0xc3, 0xa9]),
            ("utf8mb4", vec![0xff, 0xc3]),
            ("gbk", vec![0x81, 0x5c]),
        ] {
            let facts = Facts::new(charset);
            let mut sql = b"SELECT '".to_vec();
            sql.extend_from_slice(&bytes);
            sql.push(b'\'');
            let result = parsed(&sql, &facts, false, false);
            let [SelectItem::MySqlProjection {
                expr,
                source_occurrence,
                ..
            }] = projection(&result)
            else {
                panic!("one raw projection")
            };
            assert_eq!(text(expr).bytes().unwrap(), bytes);
            assert_eq!(text(expr).charset(), charset);
            assert_eq!(source_occurrence.bytes().unwrap(), &sql[7..]);
            assert_eq!(cpp(&result), sql);
        }
        let facts = Facts::new("utf8mb4");
        let result = parsed(br"SELECT '\a\f\v\n\t\r\b\0\Z\_\%'", &facts, false, false);
        let SelectItem::MySqlProjection { expr, .. } = &projection(&result)[0] else {
            panic!("projection")
        };
        assert_eq!(text(expr).bytes().unwrap(), b"afv\n\t\r\x08\0\x1a\\_\\%");
        let result = parsed(br"SELECT '\a\n'", &facts, false, true);
        let SelectItem::MySqlProjection { expr, .. } = &projection(&result)[0] else {
            panic!("projection")
        };
        assert_eq!(text(expr).bytes().unwrap(), br"\a\n");
    }

    #[test]
    fn raw_projection_keeps_first_literal_alias_and_original_cpp_boundaries() {
        // Invalid system bytes are legal only on the genuine same-system
        // branch; cross-charset validation has its own native Source tests.
        let facts = Facts::new("utf8mb3");
        let sql = b"SELECT 'a'/* raw */'b' AS '\xff\0name', - 1 + 2";
        let result = parsed(sql, &facts, false, false);
        let SelectItem::MySqlProjection {
            expr,
            alias: Some(alias),
            source_occurrence,
            cpp_piece_range,
            ..
        } = &projection(&result)[0]
        else {
            panic!("first projection")
        };
        assert_eq!(text(expr).bytes().unwrap(), b"ab");
        assert_eq!(text(expr).source().bytes().unwrap(), b"'a'");
        assert_eq!(alias.bytes().unwrap(), b"\xff\0name");
        assert_eq!(alias.alias_system_bytes().unwrap(), b"\xff\0name");
        assert_eq!(source_occurrence.bytes().unwrap(), b"'a'/* raw */'b'");
        let mut bytes = Vec::new();
        for piece in cpp_piece_range.pieces(&result.cpp).unwrap() {
            if let SqlCppPiece::Source(source) = piece {
                bytes.extend_from_slice(source.bytes().unwrap());
            }
        }
        assert_eq!(bytes, b"'a'/* raw */'b'");
        let SelectItem::MySqlProjection {
            source_occurrence, ..
        } = &projection(&result)[1]
        else {
            panic!("second projection")
        };
        assert_eq!(source_occurrence.bytes().unwrap(), b"- 1 + 2");
        let mut alias_sql = b"SELECT 1 AS '".to_vec();
        alias_sql.extend_from_slice(&vec![b'x'; 257]);
        alias_sql.push(b'\'');
        let result = parsed(&alias_sql, &facts, false, false);
        let SelectItem::MySqlProjection {
            alias: Some(alias), ..
        } = &projection(&result)[0]
        else {
            panic!("alias")
        };
        assert_eq!(alias.bytes().unwrap().len(), 257); // Name_string is a later actual producer.
    }

    #[test]
    fn projection_alias_reuses_ident_system_value_but_converts_quoted_text_once() {
        for (sql, ansi, expected_calls) in [
            (b"SELECT 1 AS '\xe9'".as_slice(), false, 1),
            (b"SELECT 1 AS `\xe9`".as_slice(), false, 0),
            (b"SELECT 1 AS \"\xe9\"".as_slice(), true, 0),
            (b"SELECT 1 \xe9".as_slice(), false, 0),
        ] {
            let facts = Facts::new("latin1");
            let statement = parsed(sql, &facts, ansi, false);
            let SelectItem::MySqlProjection {
                alias: Some(alias), ..
            } = &projection(&statement)[0]
            else {
                panic!("actual projection alias")
            };
            assert_eq!(alias.alias_system_bytes().unwrap(), "é".as_bytes());
            assert_eq!(alias.charset(), "utf8mb3");
            assert_eq!(
                facts.alias_text_calls.load(Ordering::SeqCst),
                expected_calls
            );
            assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 0);
            assert_eq!(cpp(&statement), sql);
            let retained = alias.clone();
            drop(statement);
            assert!(facts.live.load(Ordering::SeqCst) > 0);
            assert_eq!(retained.alias_system_bytes().unwrap(), "é".as_bytes());
            drop(retained);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn raw_same_parser_version_cpp_and_modes_preserve_real_source_origin() {
        let facts = Facts::new("utf8mb4");
        let result = parsed(b"SELECT /*!80000 1*/+2", &facts, false, false);
        assert_eq!(cpp(&result), b"SELECT  1 +2");
        assert!(result
            .cpp
            .iter()
            .any(|piece| matches!(piece, SqlCppPiece::Space)));
        let result = parsed(b"SELECT /*!99999 ignored*/1", &facts, false, false);
        assert_eq!(cpp(&result), b"SELECT 1");
        let result = parsed(
            b"SELECT mysql_prepared_in('x'), `mysql_bound_user_variable`(1)",
            &facts,
            false,
            false,
        );
        for item in projection(&result) {
            let SelectItem::MySqlProjection {
                expr: Expr::Function(function),
                ..
            } = item
            else {
                panic!("source function")
            };
            assert_eq!(function.origin, FunctionOrigin::Source);
        }
        let result = parsed(b"SELECT \"column\"", &facts, true, false);
        assert!(matches!(
            &projection(&result)[0],
            SelectItem::MySqlProjection {
                expr: Expr::Identifier(_),
                ..
            }
        ));
        let result = parsed(b"SELECT \"column\"", &facts, false, false);
        let SelectItem::MySqlProjection { expr, .. } = &projection(&result)[0] else {
            panic!("literal")
        };
        assert_eq!(text(expr).bytes().unwrap(), b"column");
    }

    #[test]
    fn raw_tail_is_not_lexed_and_next_statement_uses_new_mode() {
        let facts = Facts::new("utf8mb4");
        let sql = b"SELECT 1;\xfdmust_not_be_read";
        let result = parsed(sql, &facts, false, false);
        assert_eq!(result.consumed_end, 9);
        assert_eq!(result.source_occurrence.bytes().unwrap(), b"SELECT 1");
        assert_eq!(facts.tail_probes.load(Ordering::SeqCst), 0);
        let owner = facts.owner(br"SELECT 1; SELECT '\n'".to_vec());
        let first = Parser::new(&MySqlDialect {})
            .try_with_mysql_source(owner.occurrence(0..owner.bytes().len()).unwrap(), &facts)
            .unwrap()
            .parse_next_mysql_source()
            .unwrap()
            .unwrap();
        let dialect = MySqlDialect::with_options(false, false, true);
        let next = Parser::new(&dialect)
            .try_with_mysql_source(
                owner
                    .occurrence(first.consumed_end..owner.bytes().len())
                    .unwrap(),
                &facts,
            )
            .unwrap()
            .parse_next_mysql_source()
            .unwrap()
            .unwrap();
        let SelectItem::MySqlProjection { expr, .. } = &projection(&next)[0] else {
            panic!("tail projection")
        };
        assert_eq!(text(expr).bytes().unwrap(), br"\n");
        assert_eq!(next.consumed_end, owner.bytes().len());
    }

    #[test]
    fn borrowed_raw_tokens_stay_stable_across_blocks_and_checkpoints() {
        let facts = Facts::new("utf8mb4");
        let mut sql = b"SELECT ".to_vec();
        for ordinal in 0..300 {
            if ordinal != 0 {
                sql.push(b',')
            }
            sql.push(b'1');
        }
        let owner = facts.owner(sql);
        let mut parser = Parser::new(&MySqlDialect {})
            .try_with_mysql_source(owner.occurrence(0..owner.bytes().len()).unwrap(), &facts)
            .unwrap();
        let first = parser.peek_token_ref();
        let address = first as *const _;
        assert!(matches!(
            parser.token_at(512).token,
            Token::Comma | Token::Number(_, _)
        ));
        assert_eq!(parser.peek_token_ref() as *const _, address);
        assert_eq!(first.token, Token::make_word("SELECT", None));
        parser.next_token();
        parser.prev_token();
        assert_eq!(parser.peek_token_ref() as *const _, address);
        let result = parser.parse_next_mysql_source().unwrap().unwrap();
        assert_eq!(projection(&result).len(), 300);
    }

    #[test]
    fn native_enum_binary_reductions_keep_one_source_without_expression_proof() {
        use crate::tokenizer::MySqlHexLiteralKind;
        let facts = Facts::new("latin1");
        let sql = b"CREATE TABLE t(e ENUM(X'E9',0xC3A9,0xE,B'101',0b100000000))";
        let parsed = parsed(sql, &facts, false, false);
        let Statement::CreateTable(table) = &parsed.statement else {
            panic!("table")
        };
        let DataType::MySqlEnum(members) = &table.columns[0].data_type else {
            panic!("enum")
        };
        for ((member, expected), kind) in members
            .iter()
            .zip([
                &[0xE9][..],
                &[0xC3, 0xA9][..],
                &[0x0E][..],
                &[5][..],
                &[1, 0][..],
            ])
            .zip([
                MySqlHexLiteralKind::HexQuoted,
                MySqlHexLiteralKind::HexNumber,
                MySqlHexLiteralKind::HexNumber,
                MySqlHexLiteralKind::BitQuoted,
                MySqlHexLiteralKind::BitNumber,
            ])
        {
            assert_eq!(member.bytes().unwrap(), expected);
            assert_eq!(member.charset(), "binary");
            assert_eq!(member.binary_literal(), Some(kind));
            assert!(
                member.declaration_bytes().is_err(),
                "grammar alone is not adapter binding"
            );
            assert!(Arc::ptr_eq(
                member.source().owner().unwrap(),
                parsed.source_occurrence.owner().unwrap()
            ));
        }
        drop(parsed);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn raw_domain_members_and_quoted_user_variables_are_not_decoded_values() {
        let facts = Facts::new("latin1");
        let result = parsed(
            b"CREATE TABLE t (e ENUM('\xe9',''), s SET('a','\xff'))",
            &facts,
            false,
            false,
        );
        let Statement::CreateTable(table) = &result.statement else {
            panic!("table")
        };
        let DataType::MySqlEnum(members) = &table.columns[0].data_type else {
            panic!("raw enum")
        };
        assert_eq!(members[0].bytes().unwrap(), &[0xe9]);
        assert_eq!(members[1].bytes().unwrap(), b"");
        let DataType::MySqlSet(members) = &table.columns[1].data_type else {
            panic!("raw set")
        };
        assert_eq!(members[1].bytes().unwrap(), &[0xff]);
        let result = parsed(
            b"SELECT @'@name', @'comma,name', @'a.b', @''",
            &facts,
            false,
            false,
        );
        let names = projection(&result)
            .iter()
            .map(|item| {
                let SelectItem::MySqlProjection {
                    expr: Expr::UserVariable(ident),
                    ..
                } = item
                else {
                    panic!("user variable")
                };
                ident.value.as_str()
            })
            .collect::<Vec<_>>();
        assert_eq!(names, ["@name", "comma,name", "a.b", ""]);
    }

    #[test]
    fn source_admission_errors_cancellation_drop_and_client_width_are_explicit() {
        let mut facts = Facts::new("utf8mb4");
        let owner = facts.owner(b"SELECT 1".to_vec());
        facts.limit = 1;
        assert!(Parser::new(&MySqlDialect {})
            .try_with_mysql_source(owner.occurrence(0..8).unwrap(), &facts)
            .err()
            .unwrap()
            .to_string()
            .contains("budget"));
        facts.limit = usize::MAX;
        facts.minimum = 2;
        assert!(Parser::new(&MySqlDialect {})
            .try_with_mysql_source(owner.occurrence(0..8).unwrap(), &facts)
            .err()
            .unwrap()
            .to_string()
            .contains("client charset"));
        facts.minimum = 1;
        let calls = Arc::new(AtomicUsize::new(0));
        let probe = calls.clone();
        let parser = Parser::new(&MySqlDialect {})
            .with_cancellation(Arc::new(move || probe.fetch_add(1, Ordering::SeqCst) > 1))
            .try_with_mysql_source(owner.occurrence(0..8).unwrap(), &facts)
            .unwrap();
        assert!(matches!(
            parser.parse_next_mysql_source(),
            Err(ParserError::Cancelled)
        ));
        drop(owner);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        let result = parsed(br"SELECT 'a\n'", &facts, false, false);
        assert!(facts.live.load(Ordering::SeqCst) > 1);
        drop(result);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serialized_raw_text_has_detached_ranges_not_whole_sql_or_admission() {
        let facts = Facts::new("latin1");
        let sql = b"SELECT '\xe9' AS 'alias'";
        let result = parsed(sql, &facts, false, false);
        let SelectItem::MySqlProjection { expr, .. } = &projection(&result)[0] else {
            panic!("projection")
        };
        let original = text(expr);
        let encoded = serde_json::to_vec(original).unwrap();
        assert!(!encoded.windows(6).any(|window| window == b"SELECT"));
        let mut detached: MySqlText = serde_json::from_slice(&encoded).unwrap();
        assert!(detached.bytes().is_err());
        assert_eq!(detached, *original);
        let owner = facts.owner(sql.to_vec());
        detached.reattach(&owner, facts.admit(1).unwrap()).unwrap();
        assert_eq!(detached.bytes().unwrap(), &[0xe9]);
        assert_eq!(detached.source().bytes().unwrap(), b"'\xe9'");
        assert!(owner.occurrence(0..sql.len() + 1).is_err());
        assert!(original.source().slice(0..99).is_err());
        assert!(
            serde_json::from_str::<crate::ast::SqlSourceOccurrence>(r#"{"start":9,"end":1}"#)
                .is_err()
        );
    }

    #[test]
    fn raw_comments_control_and_offset_ranges_share_the_one_ledger() {
        let facts = Facts::new("utf8mb4");
        let sql = b"; /*before*/ SELECT 1 --\x7f native comment\n +2 /*after*/;\xfd";
        let owner = facts.owner(sql.to_vec());
        let parsed = Parser::new(&MySqlDialect {})
            .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
            .unwrap()
            .parse_next_mysql_source()
            .unwrap()
            .unwrap();
        assert_eq!(
            parsed.source_occurrence.bytes().unwrap(),
            b" /*before*/ SELECT 1 --\x7f native comment\n +2 /*after*/"
        );
        assert_eq!(parsed.consumed_end, sql.len() - 1);
        assert_eq!(facts.tail_probes.load(Ordering::SeqCst), 0);
        let SelectItem::MySqlProjection {
            source_occurrence,
            cpp_piece_range,
            ..
        } = &projection(&parsed)[0]
        else {
            panic!("projection")
        };
        assert_eq!(
            source_occurrence.bytes().unwrap(),
            b"1 --\x7f native comment\n +2"
        );
        let actual: Vec<u8> = cpp_piece_range
            .pieces(&parsed.cpp)
            .unwrap()
            .iter()
            .flat_map(|piece| match piece {
                SqlCppPiece::Source(source) => source.bytes().unwrap().to_vec(),
                SqlCppPiece::Space => vec![b' '],
            })
            .collect();
        assert_eq!(actual, source_occurrence.bytes().unwrap());
        assert!(Parser::new(&MySqlDialect {})
            .try_with_mysql_source(owner.occurrence(0..0).unwrap(), &facts)
            .unwrap()
            .parse_next_mysql_source()
            .unwrap()
            .is_none());
        assert!(parsed.source_occurrence.slice(9..1).is_err());
        for sql in [
            b"--".as_slice(),
            b"#comment".as_slice(),
            b"/*comment*/".as_slice(),
        ] {
            let owner = facts.owner(sql.to_vec());
            assert!(Parser::new(&MySqlDialect {})
                .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
                .unwrap()
                .parse_next_mysql_source()
                .unwrap()
                .is_none());
        }
    }

    #[test]
    fn command_terminal_retains_comment_source_without_inventing_a_statement_or_skipping_delimiters(
    ) {
        let facts = Facts::new("utf8mb4");
        for bytes in [
            b"#tail".as_slice(),
            b"/*tail*/",
            b"-- tail\n",
            b"/*!80046 */",
        ] {
            let owner = facts.owner(bytes.to_vec());
            let terminal = Parser::new(&MySqlDialect {})
                .try_with_mysql_source(owner.occurrence(0..bytes.len()).unwrap(), &facts)
                .unwrap()
                .parse_next_mysql_command_with_body::<Statement, ()>(|_| {
                    panic!("a checked comment-only terminal never invokes a body parser")
                })
                .unwrap()
                .unwrap()
                .0;
            assert!(terminal.statement.is_none());
            assert_eq!(terminal.source_occurrence.bytes().unwrap(), bytes);
            assert_eq!(terminal.consumed_end, bytes.len());
            assert!(Arc::ptr_eq(
                terminal.source_occurrence.owner().unwrap(),
                &owner
            ));
            drop(terminal);
            drop(owner);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
        for bytes in [b"".as_slice(), b" \n\t"] {
            let owner = facts.owner(bytes.to_vec());
            assert!(Parser::new(&MySqlDialect {})
                .try_with_mysql_source(owner.occurrence(0..bytes.len()).unwrap(), &facts)
                .unwrap()
                .parse_next_mysql_command_with_body::<Statement, ()>(|_| {
                    panic!("bare EOF is not a comment-only statement")
                })
                .unwrap()
                .is_none());
        }
        for bytes in [
            b"; SELECT 1".as_slice(),
            b"/*unterminated",
            b"/*!80046 SELECT */",
        ] {
            let owner = facts.owner(bytes.to_vec());
            assert!(
                Parser::new(&MySqlDialect {})
                    .try_with_mysql_source(owner.occurrence(0..bytes.len()).unwrap(), &facts)
                    .unwrap()
                    .parse_next_mysql_command_with_body::<Statement, ()>(|parser| {
                        parser.parse_statement().map(|statement| (statement, None))
                    })
                    .is_err(),
                "{bytes:?}"
            );
        }
        let bytes = b"SELECT 1; /*surviving tail*/";
        let owner = facts.owner(bytes.to_vec());
        let first = Parser::new(&MySqlDialect {})
            .with_mysql_multi_statements(true)
            .try_with_mysql_source(owner.occurrence(0..bytes.len()).unwrap(), &facts)
            .unwrap()
            .parse_next_mysql_command_with_body::<Statement, ()>(|parser| {
                parser.parse_statement().map(|statement| (statement, None))
            })
            .unwrap()
            .unwrap()
            .0;
        assert!(first.statement.is_some());
        assert_eq!(first.consumed_end, 9);
        assert_eq!(facts.tail_probes.load(Ordering::SeqCst), 0);
        let tail = Parser::new(&MySqlDialect {})
            .try_with_mysql_source(
                owner.occurrence(first.consumed_end..bytes.len()).unwrap(),
                &facts,
            )
            .unwrap()
            .parse_next_mysql_command_with_body::<Statement, ()>(|_| panic!("comment tail"))
            .unwrap()
            .unwrap()
            .0;
        assert!(tail.statement.is_none());
        assert_eq!(
            tail.source_occurrence.bytes().unwrap(),
            b" /*surviving tail*/"
        );
        drop((tail, first, owner));
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn raw_numeric_fsm_admits_capacity_and_cancellation_before_large_growth() {
        let mut facts = Facts::new("utf8mb4");
        let mut sql = b"SELECT ".to_vec();
        sql.extend(core::iter::repeat_n(b'9', 100_000));
        let owner = facts.owner(sql);
        facts.limit = core::mem::size_of::<crate::parser::MySqlTokenBlock>() + 1024;
        let error = Parser::new(&MySqlDialect {})
            .try_with_mysql_source(owner.occurrence(0..owner.bytes().len()).unwrap(), &facts)
            .unwrap()
            .parse_next_mysql_source()
            .err()
            .unwrap();
        assert!(error.to_string().contains("budget"));
        drop(owner);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        facts.limit = usize::MAX;
        let numeric = parsed(b"SELECT 0xF,0b101,1.25e-3,2e+9", &facts, false, false);
        let values: Vec<_> = projection(&numeric)
            .iter()
            .map(|projection| {
                let SelectItem::MySqlProjection {
                    expr: Expr::Value(value),
                    ..
                } = projection
                else {
                    panic!("literal")
                };
                value.value.clone()
            })
            .collect();
        assert_eq!(
            values,
            vec![
                Value::HexStringLiteral("0F".into()),
                Value::HexStringLiteral("05".into()),
                Value::Number("1.25e-3".into(), false),
                Value::Number("2e+9".into(), false)
            ]
        );
        for sql in [br"SELECT X'4\1'".as_slice(), br"SELECT b'1\0'".as_slice()] {
            let owner = facts.owner(sql.to_vec());
            assert!(Parser::new(&MySqlDialect {})
                .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
                .unwrap()
                .parse_next_mysql_source()
                .is_err());
        }
        for sql in [b"SELECT X''".as_slice(), b"SELECT b''".as_slice()] {
            let actual = parsed(sql, &facts, false, false);
            let SelectItem::MySqlProjection {
                expr: Expr::Value(value),
                ..
            } = &projection(&actual)[0]
            else {
                panic!("empty binary literal")
            };
            assert_eq!(value.value, Value::HexStringLiteral(String::new()));
        }
        let identifiers = parsed(b"SELECT 1e,0x,0xFg,0b12", &facts, false, false);
        let names: Vec<_> = projection(&identifiers)
            .iter()
            .map(|projection| {
                let SelectItem::MySqlProjection {
                    expr: Expr::Identifier(name),
                    ..
                } = projection
                else {
                    panic!("native numeric-prefix identifier")
                };
                name.value.as_str()
            })
            .collect();
        assert_eq!(names, vec!["1e", "0x", "0xFg", "0b12"]);
        let fraction_alias = parsed(b"SELECT 1.2a", &facts, false, false);
        let SelectItem::MySqlProjection {
            expr: Expr::Value(value),
            alias: Some(alias),
            ..
        } = &projection(&fraction_alias)[0]
        else {
            panic!("decimal plus real alias")
        };
        assert_eq!(value.value, Value::Number("1.2".into(), false));
        assert_eq!(alias.bytes().unwrap(), b"a");
        let invalid = b"SELECT 1.2e";
        let owner = facts.owner(invalid.to_vec());
        assert!(Parser::new(&MySqlDialect {})
            .try_with_mysql_source(owner.occurrence(0..invalid.len()).unwrap(), &facts)
            .unwrap()
            .parse_next_mysql_source()
            .is_err());
    }

    #[test]
    fn raw_introducers_and_cancel_mid_quote_preserve_actual_source() {
        let facts = Facts::new("utf8mb4");
        let actual = parsed(
            b"SELECT _latin1'\xc3\xa9',_latin1 X'E9'",
            &facts,
            false,
            false,
        );
        let SelectItem::MySqlProjection {
            expr: Expr::Prefixed { prefix, value },
            ..
        } = &projection(&actual)[0]
        else {
            panic!("introduced source")
        };
        assert_eq!(prefix.value, "_latin1");
        assert_eq!(text(value).bytes().unwrap(), &[0xc3, 0xa9]);
        assert_eq!(text(value).charset(), "utf8mb4");
        let SelectItem::MySqlProjection {
            expr: Expr::Prefixed { value, .. },
            ..
        } = &projection(&actual)[1]
        else {
            panic!("introduced binary")
        };
        assert!(
            matches!(value.as_ref(), Expr::Value(value) if value.value == Value::HexStringLiteral("E9".into()))
        );
        // Native TYPELIB members use text_string, not introduced or adjacent
        // expression literals. Preserve both expression assertions above and
        // reject each declaration production independently.
        for sql in [
            b"CREATE TABLE t(e ENUM(_latin1'\\xc3\\xa9','ab'))".as_slice(),
            b"CREATE TABLE t(e ENUM('a' 'b','c'))".as_slice(),
            b"CREATE TABLE t(s SET(_binary'a'))".as_slice(),
        ] {
            let owner = facts.owner(sql.to_vec());
            assert!(Parser::new(&MySqlDialect {})
                .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
                .unwrap()
                .parse_next_mysql_source()
                .is_err());
        }

        let mut sql = b"SELECT '".to_vec();
        sql.extend(core::iter::repeat_n(b'a', 4096));
        sql.push(b'\'');
        let owner = facts.owner(sql);
        facts.units.store(0, Ordering::SeqCst);
        let units = facts.units.clone();
        let parser = Parser::new(&MySqlDialect {})
            .with_cancellation(Arc::new(move || units.load(Ordering::SeqCst) >= 1024))
            .try_with_mysql_source(owner.occurrence(0..owner.bytes().len()).unwrap(), &facts)
            .unwrap();
        assert!(matches!(
            parser.parse_next_mysql_source(),
            Err(ParserError::Cancelled)
        ));
        assert!((1024..8192).contains(&facts.units.load(Ordering::SeqCst)));
    }
    #[test]
    fn adjacent_raw_segments_preserve_their_actual_reductions_and_one_value() {
        use crate::ast::MySqlTextSegment;
        let facts = Facts::new("latin1");
        let result = parsed(
            b"SELECT 'a\\n' /* gap */ '\xe9' AS 'public'",
            &facts,
            false,
            false,
        );
        let SelectItem::MySqlProjection {
            expr,
            alias: Some(alias),
            ..
        } = &projection(&result)[0]
        else {
            panic!("raw projection")
        };
        let value = text(expr);
        assert_eq!(value.bytes().unwrap(), b"a\n\xe9");
        assert_eq!(value.segments().len(), 2);
        let clone = value.clone();
        assert_eq!(
            clone.segments().as_ptr(),
            value.segments().as_ptr(),
            "adjacent metadata is shared, not recopied on AST clone"
        );
        assert_eq!(value.segment_bytes(0).unwrap(), b"a\n");
        assert_eq!(value.segment_bytes(1).unwrap(), b"\xe9");
        assert_eq!(value.segments()[0].source().bytes().unwrap(), b"'a\\n'");
        assert_eq!(value.segments()[1].source().bytes().unwrap(), b"'\xe9'");
        assert!(Arc::ptr_eq(
            value.segments()[0].source().owner().unwrap(),
            value.segments()[1].source().owner().unwrap()
        ));
        assert_eq!(alias.role(), crate::ast::MySqlTextRole::Alias);
        assert!(value.segment_bytes(2).is_err());
        let semantic = facts.owner(b"a\n\xe9".to_vec());
        let mut parts = value.segments().to_vec();
        let last = &parts[1];
        parts[1] = MySqlTextSegment::new(
            last.semantic_range(),
            last.source().clone(),
            "binary".into(),
            last.quote(),
            Some("_binary".into()),
        )
        .unwrap();
        let changed = MySqlText::from_segments(
            semantic.occurrence(0..3).unwrap(),
            parts,
            facts.admit(256).unwrap(),
        )
        .unwrap();
        assert_ne!(
            changed, *value,
            "tail domain is semantic, not only the first segment"
        );
        let reversed = vec![value.segments()[1].clone(), value.segments()[0].clone()];
        assert!(MySqlText::from_segments(
            semantic.occurrence(0..3).unwrap(),
            reversed,
            facts.admit(256).unwrap()
        )
        .is_err());
        let overlap = vec![
            value.segments()[0].clone(),
            MySqlTextSegment::new(
                2..3,
                value.segments()[0].source().clone(),
                "latin1".into(),
                '\'',
                None,
            )
            .unwrap(),
        ];
        assert!(MySqlText::from_segments(
            semantic.occurrence(0..3).unwrap(),
            overlap,
            facts.admit(256).unwrap()
        )
        .is_err());
        drop(changed);
        drop(semantic);
        drop(clone);
        drop(result);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn native_load_shape_keeps_roles_options_and_set_source_in_the_one_ast() {
        use crate::ast::{
            MySqlLoadDuplicate, MySqlLoadInput, MySqlLoadLock, MySqlLoadSourceType, MySqlTextRole,
        };
        let facts = Facts::new("latin1");
        let sql = b"LOAD DATA LOW_PRIORITY FROM LOCAL INFILE 'file\xe9' COUNT 3 IN PRIMARY KEY ORDER IGNORE INTO TABLE `db`.`t` PARTITION (p0,p1) CHARACTER SET 'latin1' ROWS IDENTIFIED BY '<r>' FIELDS TERMINATED BY ',' OPTIONALLY ENCLOSED BY '\"' ESCAPED BY '\\\\' LINES STARTING BY '>' TERMINATED BY '\\n' IGNORE 2 ROWS (a,@'a,b',@plain) SET a=(@side:=7),b=DEFAULT ALGORITHM=BULK";
        let result = parsed(sql, &facts, false, false);
        let Statement::MySqlLoadData(load) = &result.statement else {
            panic!("native, not Hive LOAD")
        };
        assert_eq!(load.lock, MySqlLoadLock::LowPriority);
        assert!(load.from_keyword && load.local && load.primary_key_order && load.bulk);
        assert_eq!(load.source_type, MySqlLoadSourceType::Infile);
        assert_eq!(load.duplicate, MySqlLoadDuplicate::Ignore);
        assert_eq!(load.source_count, Some(3));
        assert_eq!(load.partitions.len(), 2);
        let Value::MySqlText(filename) = &load.filename.value else {
            panic!("raw filesystem")
        };
        assert_eq!(filename.bytes().unwrap(), b"file\xe9");
        assert_eq!(filename.role(), MySqlTextRole::FileSystem);
        let Some(Expr::Value(cs)) = &load.character_set else {
            panic!("actual charset text")
        };
        let Value::MySqlText(cs) = &cs.value else {
            panic!("raw charset")
        };
        assert_eq!(cs.role(), MySqlTextRole::System);
        assert_eq!(load.fields.len(), 3);
        assert_eq!(load.lines.len(), 2);
        assert_eq!(load.ignore, Some((2, true)));
        let inputs = load.inputs.as_ref().unwrap();
        assert_eq!(inputs.len(), 3);
        let MySqlLoadInput::RawUserVariable(name) = &inputs[1] else {
            panic!("actual quoted target")
        };
        assert_eq!(name.bytes().unwrap(), b"a,b");
        assert_eq!(name.role(), MySqlTextRole::System);
        let MySqlLoadInput::UserVariable(name) = &inputs[2] else {
            panic!("identifier target")
        };
        assert_eq!(name.value, "plain");
        assert_eq!(load.assignments.len(), 2);
        assert_eq!(
            load.assignments[0]
                .source_occurrence
                .as_ref()
                .unwrap()
                .bytes()
                .unwrap(),
            b"=(@side:=7)"
        );
        assert!(load
            .assignments
            .iter()
            .all(|item| item.cpp_piece_range.is_some()));
        assert!(
            matches!(&load.assignments[0].assignment.value, Expr::Nested(value) if matches!(value.as_ref(), Expr::BinaryOp { op: BinaryOperator::Assignment, .. }))
        );
        let xml = parsed(
            b"LOAD XML CONCURRENT URL 'source' REPLACE INTO TABLE t ROWS IDENTIFIED BY '<row>' ()",
            &facts,
            false,
            false,
        );
        let Statement::MySqlLoadData(xml) = &xml.statement else {
            panic!("XML")
        };
        assert!(xml.xml);
        assert_eq!(xml.lock, MySqlLoadLock::Concurrent);
        assert_eq!(xml.source_type, MySqlLoadSourceType::Url);
        assert_eq!(xml.duplicate, MySqlLoadDuplicate::Replace);
        assert!(xml.inputs.as_ref().unwrap().is_empty());
        assert!(
            Parser::parse_sql(
                &crate::dialect::HiveDialect {},
                "LOAD DATA INPATH '/p' INTO TABLE t"
            )
            .is_ok(),
            "generic Hive production remains separate"
        );
    }

    #[test]
    fn raw_outfile_and_column_comments_keep_roles_while_table_comment_consumes_system_text_once() {
        use crate::ast::{CommentDef, MySqlTextRole};
        let facts = Facts::new("latin1");
        let result = parsed(b"SELECT 1 INTO OUTFILE 'p\xe9' CHARACTER SET 'latin1' FIELDS TERMINATED BY '\xe9' LINES TERMINATED BY '\\n'", &facts, false, false);
        let Statement::Query(query) = &result.statement else {
            panic!("query")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("select")
        };
        let SelectIntoTarget::MySqlFile(file) = &select.into.as_ref().unwrap().target else {
            panic!("raw destination")
        };
        let Value::MySqlText(path) = &file.path.value else {
            panic!("raw path")
        };
        assert_eq!(path.bytes().unwrap(), b"p\xe9");
        assert_eq!(path.role(), MySqlTextRole::FileSystem);
        assert_eq!(file.fields.len(), 1);
        assert_eq!(file.lines.len(), 1);
        assert!(!file.dumpfile);
        assert!(matches!(&file.character_set, Some(Expr::Value(value))
            if matches!(&value.value, Value::MySqlText(text)
                if text.role() == MySqlTextRole::System && text.bytes().unwrap() == b"latin1")));
        assert!(
            matches!(&file.fields[0], crate::ast::MySqlLoadFieldOption::TerminatedBy(value)
            if matches!(&value.value, Value::MySqlText(text)
                if text.role() == MySqlTextRole::Literal && text.bytes().unwrap() == b"\xe9"))
        );
        assert!(
            matches!(&file.lines[0], crate::ast::MySqlLoadLineOption::TerminatedBy(value)
            if matches!(&value.value, Value::MySqlText(text)
                if text.role() == MySqlTextRole::Literal && text.bytes().unwrap() == b"\n"))
        );
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 0);
        let dump = parsed(b"SELECT 1 INTO DUMPFILE 'p\xe9'", &facts, false, false);
        let Statement::Query(query) = &dump.statement else {
            panic!("dump")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("select")
        };
        assert!(
            matches!(&select.into.as_ref().unwrap().target, SelectIntoTarget::MySqlFile(file) if file.dumpfile && file.fields.is_empty() && file.character_set.is_none())
        );
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 0);
        let parsed_table = parsed(
            b"CREATE TABLE t(a INT COMMENT 'column\xe9') COMMENT='table\xe9'",
            &facts,
            false,
            false,
        );
        let Statement::CreateTable(table) = &parsed_table.statement else {
            panic!("table")
        };
        let ColumnOption::MySqlComment(comment) = &table.columns[0].options[0].option else {
            panic!("column comment")
        };
        assert_eq!(comment.role(), MySqlTextRole::System);
        assert_eq!(comment.system_bytes().unwrap(), "columné".as_bytes());
        assert!(
            matches!(&table.comment, Some(CommentDef::MySql { has_eq: true, text }) if text.bytes().unwrap() == "tableé".as_bytes())
        );
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 2);
        assert_eq!(facts.system_role_text_calls.load(Ordering::SeqCst), 2);
        drop((parsed_table, dump, result));
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        assert_eq!(facts.system_text_live.load(Ordering::SeqCst), 0);
    }

    #[cfg(feature = "visitor")]
    #[test]
    fn visitors_reach_raw_roles_types_nested_arrays_load_set_and_projection() {
        use crate::ast::{ArrayElemTypeDef, MySqlTextRole, Visit, VisitMut, Visitor, VisitorMut};
        use std::ops::ControlFlow;
        #[derive(Default)]
        struct Seen {
            text: usize,
            expr: usize,
            types: Vec<&'static str>,
        }
        impl Visitor for Seen {
            type Break = ();
            fn pre_visit_mysql_text(&mut self, _: &MySqlText) -> ControlFlow<()> {
                self.text += 1;
                ControlFlow::Continue(())
            }
            fn pre_visit_expr(&mut self, _: &Expr) -> ControlFlow<()> {
                self.expr += 1;
                ControlFlow::Continue(())
            }
            fn pre_visit_data_type(&mut self, value: &DataType) -> ControlFlow<()> {
                self.types.push(match value {
                    DataType::Array(_) => "array",
                    DataType::MySqlEnum(_) => "enum",
                    _ => "other",
                });
                ControlFlow::Continue(())
            }
        }
        let facts = Facts::new("latin1");
        let mut table = parsed(
            b"CREATE TABLE t(e ENUM('ab','c') COMMENT 'column') COMMENT 'table'",
            &facts,
            false,
            false,
        );
        let mut seen = Seen::default();
        assert!(table.statement.visit(&mut seen).is_continue());
        assert_eq!(seen.text, 4);
        assert_eq!(seen.types, ["enum"]);
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 2);
        let Statement::CreateTable(create) = &table.statement else {
            panic!("table")
        };
        let array = DataType::Array(ArrayElemTypeDef::SquareBracket(
            Box::new(create.columns[0].data_type.clone()),
            None,
        ));
        let mut seen = Seen::default();
        assert!(array.visit(&mut seen).is_continue());
        assert_eq!(seen.types, ["array", "enum"]);
        assert_eq!(seen.text, 2);
        struct Change;
        impl VisitorMut for Change {
            type Break = ();
            fn pre_visit_mysql_text(&mut self, value: &mut MySqlText) -> ControlFlow<()> {
                *value = value.clone().with_role(MySqlTextRole::Validated);
                ControlFlow::Continue(())
            }
        }
        assert!(VisitMut::visit(&mut table.statement, &mut Change).is_continue());
        let Statement::CreateTable(create) = &table.statement else {
            panic!("same table after raw-role mutation")
        };
        assert!(
            matches!(&create.comment, Some(crate::ast::CommentDef::MySql { has_eq: false, text }) if text.bytes().unwrap() == b"table")
        );
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 2);
        let load = parsed(
            b"LOAD DATA INFILE 'p' INTO TABLE t (@'a,b') SET a=(@side:=7)",
            &facts,
            false,
            false,
        );
        let mut seen = Seen::default();
        assert!(load.statement.visit(&mut seen).is_continue());
        assert_eq!(seen.text, 2);
        assert!(seen.expr >= 3);
        let query = parsed(b"SELECT 'v' AS 'label'", &facts, false, false);
        let mut seen = Seen::default();
        assert!(query.statement.visit(&mut seen).is_continue());
        assert_eq!((seen.text, seen.expr), (2, 1));
        let normal =
            Parser::parse_sql(&crate::dialect::PostgreSqlDialect {}, "SELECT 1+2").unwrap();
        let mut seen = Seen::default();
        assert!(normal.visit(&mut seen).is_continue());
        assert_eq!((seen.text, seen.expr), (0, 3));
        drop((table, array, load, query));
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[cfg(feature = "visitor")]
    #[test]
    fn raw_create_table_comment_has_one_canonical_owner_and_native_last_clause_wins() {
        use crate::ast::{
            CommentDef, CreateTableElement, CreateTableOptions, MySqlTextRole, SqlOption, Visit,
            VisitMut, Visitor, VisitorMut,
        };
        use std::ops::ControlFlow;
        let facts = Facts::new("latin1");
        let sql = b"CREATE TABLE t(e ENUM('ab','c') COMMENT 'column\xe9') COMMENT='old\xe9' ENGINE=InnoDB COMMENT 'table\xe9'";
        let mut parsed = parsed(sql, &facts, false, false);
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 3);
        assert_eq!(cpp(&parsed), sql);
        assert_eq!(facts.system_role_text_calls.load(Ordering::SeqCst), 3);
        assert_eq!(facts.system_text_live.load(Ordering::SeqCst), 3);
        let Statement::CreateTable(table) = &parsed.statement else {
            panic!("table")
        };
        assert_eq!(table.elements, [CreateTableElement::Column(0)]);
        assert!(table.validate_source_order().is_ok());
        assert!(
            matches!(&table.comment, Some(CommentDef::MySql { has_eq: false, text }) if text.bytes().unwrap() == "tableé".as_bytes())
        );
        let CreateTableOptions::Plain(options) = &table.table_options else {
            panic!("options")
        };
        assert_eq!(
            options.len(),
            1,
            "ENGINE is retained without either COMMENT copy"
        );
        assert!(options
            .iter()
            .all(|value| !matches!(value, SqlOption::Comment(_))));
        struct Seen(Vec<Vec<u8>>);
        impl Visitor for Seen {
            type Break = ();
            fn pre_visit_mysql_text(&mut self, value: &MySqlText) -> ControlFlow<()> {
                self.0.push(value.bytes().unwrap().to_vec());
                ControlFlow::Continue(())
            }
        }
        let mut seen = Seen(Vec::new());
        assert!(Visit::visit(&parsed.statement, &mut seen).is_continue());
        assert_eq!(
            seen.0,
            [
                b"ab".to_vec(),
                b"c".to_vec(),
                "columné".as_bytes().to_vec(),
                "tableé".as_bytes().to_vec()
            ]
        );
        struct Change(usize);
        impl VisitorMut for Change {
            type Break = ();
            fn pre_visit_mysql_text(&mut self, value: &mut MySqlText) -> ControlFlow<()> {
                self.0 += 1;
                *value = value.clone().with_role(MySqlTextRole::Validated);
                ControlFlow::Continue(())
            }
        }
        let mut change = Change(0);
        assert!(VisitMut::visit(&mut parsed.statement, &mut change).is_continue());
        assert_eq!(change.0, 4);
        let Statement::CreateTable(table) = &parsed.statement else {
            panic!("table")
        };
        let DataType::MySqlEnum(members) = &table.columns[0].data_type else {
            panic!("enum")
        };
        assert!(members
            .iter()
            .all(|value| value.role() == MySqlTextRole::Validated));
        assert!(
            matches!(&table.columns[0].options[0].option, ColumnOption::MySqlComment(value)
            if value.role() == MySqlTextRole::Validated)
        );
        assert!(
            matches!(&table.comment, Some(CommentDef::MySql { has_eq: false, text }) if text.bytes().unwrap() == "tableé".as_bytes())
        );
        let mut after = Seen(Vec::new());
        assert!(Visit::visit(&parsed.statement, &mut after).is_continue());
        assert_eq!(
            after.0, seen.0,
            "mutation changed canonical roles, not bytes or a mirror"
        );
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 3);
        drop(parsed);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        assert_eq!(facts.system_text_live.load(Ordering::SeqCst), 0);
    }

    #[cfg(all(feature = "serde", feature = "visitor"))]
    #[test]
    fn raw_create_table_ordinals_detach_three_text_values_without_resigning_decoded_table_comment()
    {
        use crate::ast::{
            CommentDef, CreateTable, CreateTableElement, Visit, VisitMut, Visitor, VisitorMut,
        };
        use std::ops::ControlFlow;
        let facts = Facts::new("latin1");
        let sql = b"CREATE TABLE t(e ENUM('ab','c') COMMENT 'column\xe9') COMMENT='table\xe9'";
        let parsed = parsed(sql, &facts, false, false);
        let Statement::CreateTable(table) = &parsed.statement else {
            panic!("table")
        };
        let encoded = serde_json::to_value(table).unwrap();
        assert_eq!(encoded["elements"], serde_json::json!([{"Column": 0}]));
        assert!(
            matches!(&table.comment, Some(CommentDef::MySql { has_eq: true, text }) if text.bytes().unwrap() == "tableé".as_bytes())
        );
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 2);
        let mut detached: CreateTable = serde_json::from_value(encoded.clone()).unwrap();
        assert!(detached.validate_source_order().is_ok());
        assert_eq!(detached.comment, table.comment);
        struct Detached(usize);
        impl Visitor for Detached {
            type Break = ();
            fn pre_visit_mysql_text(&mut self, value: &MySqlText) -> ControlFlow<()> {
                assert!(value.bytes().is_err());
                self.0 += 1;
                ControlFlow::Continue(())
            }
        }
        let mut seen = Detached(0);
        assert!(Visit::visit(&detached, &mut seen).is_continue());
        assert_eq!(seen.0, 4);
        let owner = facts.owner(sql.to_vec());
        struct Attach<'a> {
            facts: &'a Facts,
            owner: &'a Arc<SqlSourceOwner>,
            count: usize,
        }
        impl VisitorMut for Attach<'_> {
            type Break = crate::tokenizer::MySqlSourceError;
            fn pre_visit_mysql_text(&mut self, value: &mut MySqlText) -> ControlFlow<Self::Break> {
                let admission = match self.facts.admit(1024) {
                    Ok(value) => value,
                    Err(error) => return ControlFlow::Break(error),
                };
                if let Err(error) = value.reattach(self.owner, admission) {
                    return ControlFlow::Break(error.into());
                }
                self.count += 1;
                ControlFlow::Continue(())
            }
        }
        let mut attach = Attach {
            facts: &facts,
            owner: &owner,
            count: 0,
        };
        assert!(VisitMut::visit(&mut detached, &mut attach).is_continue());
        assert_eq!(attach.count, 4);
        assert_eq!(detached.elements, [CreateTableElement::Column(0)]);
        assert!(
            matches!(&detached.comment, Some(CommentDef::MySql { has_eq: true, text })
            if text.bytes().unwrap() == "tableé".as_bytes() && text.system_bytes().is_err()),
            "reattachment restores admitted transport bytes, never the native System issuer"
        );
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 2);
        let mut bad = encoded;
        bad["elements"][0]["Column"] = serde_json::json!(1);
        let bad: CreateTable = serde_json::from_value(bad).unwrap();
        assert!(bad.validate_source_order().is_err());
        assert!(core::fmt::write(&mut String::new(), format_args!("{bad}")).is_err());
        drop((detached, bad, parsed, owner));
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn table_comment_system_conversion_is_once_and_its_admission_moves_with_the_original_bundle() {
        use crate::ast::CommentDef;
        for (sql, has_eq, expected, no_escape) in [
            (
                b"CREATE TABLE t(a INT) COMMENT='table\xe9'".as_slice(),
                true,
                "tableé",
                false,
            ),
            (
                b"CREATE TABLE t(a INT) COMMENT 'table\xe9'".as_slice(),
                false,
                "tableé",
                false,
            ),
            (
                b"ALTER TABLE t COMMENT='table\xe9'".as_slice(),
                true,
                "tableé",
                false,
            ),
            (
                b"ALTER TABLE t COMMENT 'table\xe9'".as_slice(),
                false,
                "tableé",
                false,
            ),
            (
                br"CREATE TABLE t(a INT) COMMENT='a\nb'".as_slice(),
                true,
                "a\nb",
                false,
            ),
            (
                br"CREATE TABLE t(a INT) COMMENT='a\nb'".as_slice(),
                true,
                r"a\nb",
                true,
            ),
            (
                b"CREATE TABLE t(a INT) COMMENT=''".as_slice(),
                true,
                "",
                false,
            ),
        ] {
            let facts = Facts::new("latin1");
            let result = parsed(sql, &facts, false, no_escape);
            match &result.statement {
                Statement::CreateTable(table) => {
                    assert!(
                        match &table.comment {
                            Some(CommentDef::MySql {
                                has_eq: actual,
                                text,
                            }) =>
                                *actual == has_eq
                                    && text.system_bytes().unwrap() == expected.as_bytes(),
                            _ => false,
                        },
                        "{sql:?}"
                    );
                    assert!(table.validate_source_order().is_ok());
                }
                Statement::AlterTable(table) => {
                    let comment = table.comment.as_ref().expect("original ALTER COMMENT");
                    assert!(
                        matches!(&comment.value.value, Value::MySqlText(value) if value.system_bytes().unwrap() == expected.as_bytes())
                    );
                    assert_eq!(comment.value.span, comment.value_token.0.span);
                    assert!(
                        comment.value.span.start.column > comment.comment_token.0.span.start.column
                    );
                    assert!(table.operations.is_empty());
                }
                statement => panic!("actual comment grammar: {statement:?}"),
            }
            assert_eq!(result.source_occurrence.bytes().unwrap(), sql);
            assert_eq!(cpp(&result), sql);
            assert_eq!(result.consumed_end, sql.len());
            assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 1);
            assert_eq!(facts.system_role_text_calls.load(Ordering::SeqCst), 1);
            assert_eq!(facts.system_text_live.load(Ordering::SeqCst), 1);
            let (statement, source, cpp, admissions, container, secrets) =
                result.into_owned_parts();
            drop((statement, source, cpp, secrets));
            assert_eq!(
                facts.system_text_live.load(Ordering::SeqCst),
                1,
                "the same original admission, not the decoded AST, owns conversion capacity"
            );
            drop((admissions, container));
            assert_eq!(facts.system_text_live.load(Ordering::SeqCst), 0);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn table_comment_budget_grammar_failure_and_scanner_cancel_release_original_admissions() {
        for sql in [
            b"CREATE TABLE t(a INT) COMMENT='table\xe9'".as_slice(),
            b"CREATE TABLE t(a INT) COMMENT 'table\xe9'".as_slice(),
            b"ALTER TABLE t COMMENT='table\xe9'".as_slice(),
        ] {
            let mut facts = Facts::new("latin1");
            facts.system_text_limit = 1;
            let owner = facts.owner(sql.to_vec());
            let error = Parser::new(&MySqlDialect {})
                .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
                .unwrap()
                .parse_next_mysql_source()
                .unwrap_err();
            assert!(error
                .to_string()
                .contains("system text budget refused before conversion"));
            assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 1);
            assert_eq!(facts.system_role_text_calls.load(Ordering::SeqCst), 1);
            assert_eq!(facts.system_text_live.load(Ordering::SeqCst), 0);
            drop(owner);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
        let facts = Facts::new("latin1");
        let sql = b"CREATE TABLE t(a INT) COMMENT='table\xe9' COMMENT=+";
        let owner = facts.owner(sql.to_vec());
        let error = Parser::new(&MySqlDialect {})
            .try_with_mysql_source(owner.occurrence(0..sql.len()).unwrap(), &facts)
            .unwrap()
            .parse_next_mysql_source()
            .unwrap_err();
        assert!(!matches!(error, ParserError::Cancelled));
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 1);
        assert_eq!(facts.system_text_live.load(Ordering::SeqCst), 0);
        drop(owner);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);

        let facts = Facts::new("latin1");
        let mut sql = b"CREATE TABLE t(a INT) COMMENT='".to_vec();
        sql.extend(core::iter::repeat_n(b'a', 4096));
        sql.push(b'\'');
        let owner = facts.owner(sql);
        let units = facts.units.clone();
        let error = Parser::new(&MySqlDialect {})
            .with_cancellation(Arc::new(move || units.load(Ordering::SeqCst) >= 1024))
            .try_with_mysql_source(owner.occurrence(0..owner.bytes().len()).unwrap(), &facts)
            .unwrap()
            .parse_next_mysql_source()
            .unwrap_err();
        assert!(matches!(error, ParserError::Cancelled));
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 0);
        assert_eq!(facts.system_text_live.load(Ordering::SeqCst), 0);
        drop(owner);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn raw_comments_keep_bytes_and_do_not_silently_discard_unbound_optimizer_hints() {
        let facts = Facts::new("latin1");
        let result = parsed(b"SELECT /* raw \xe9 */ 1", &facts, false, false);
        assert_eq!(projection(&result).len(), 1);
        drop(result);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        for comments in [
            vec![b"/*+ MAX_EXECUTION_TIME(1000) */".as_slice()],
            vec![
                b"/*+ SET_VAR(sort_buffer_size=16384) */".as_slice(),
                b"/*+ QB_NAME(q\xe9) */".as_slice(),
            ],
        ] {
            let mut sql = b"SELECT ".to_vec();
            let mut ranges = Vec::new();
            for comment in &comments {
                let start = sql.len();
                sql.extend_from_slice(comment);
                ranges.push(start..sql.len());
                sql.push(b' ');
            }
            sql.push(b'1');
            let owner = facts.owner(sql);
            let result = Parser::new(&MySqlDialect {})
                .try_with_mysql_source(owner.occurrence(0..owner.bytes().len()).unwrap(), &facts)
                .unwrap()
                .parse_next_mysql_source()
                .unwrap()
                .expect("parser retains source; binding belongs to the native consumer");
            let Statement::Query(query) = &result.statement else {
                panic!("raw query")
            };
            let SetExpr::Select(select) = query.body.as_ref() else {
                panic!("raw select")
            };
            assert_eq!(select.optimizer_hints.len(), comments.len());
            let mut retained = Vec::new();
            for ((hint, comment), range) in
                select.optimizer_hints.iter().zip(&comments).zip(&ranges)
            {
                assert!(hint.prefix.is_empty());
                assert!(hint.text.is_empty());
                let crate::ast::OptimizerHintStyle::MySqlSource(source) = &hint.style else {
                    panic!("raw hint must not claim bound or decoded text")
                };
                assert_eq!(source.bytes().unwrap(), *comment);
                assert_eq!(&source.range(), range);
                assert!(Arc::ptr_eq(source.owner().unwrap(), &owner));
                retained.push(source.clone());
            }
            drop(owner);
            assert!(facts.live.load(Ordering::SeqCst) > 0);
            drop(result);
            assert!(facts.live.load(Ordering::SeqCst) > 0);
            for (source, comment) in retained.iter().zip(&comments) {
                assert_eq!(source.bytes().unwrap(), *comment);
            }
            drop(retained);
            assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        }
        let ordinary =
            Parser::parse_sql(&MySqlDialect {}, "SELECT /*+ MAX_EXECUTION_TIME(1000) */ 1")
                .unwrap();
        let Statement::Query(query) = &ordinary[0] else {
            panic!("ordinary query")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("ordinary select")
        };
        assert_eq!(select.optimizer_hints.len(), 1);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn adjacent_transport_checks_partition_source_order_and_explicit_reattachment() {
        let facts = Facts::new("latin1");
        let sql = b"SELECT 'a' '\xe9'";
        let result = parsed(sql, &facts, false, false);
        let SelectItem::MySqlProjection { expr, .. } = &projection(&result)[0] else {
            panic!("projection")
        };
        let original = text(expr);
        let encoded = serde_json::to_value(original).unwrap();
        let mut detached: MySqlText = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(detached, *original);
        assert!(detached.bytes().is_err());
        let short = facts.owner(b"x".to_vec());
        assert!(detached
            .reattach(&short, facts.admit(256).unwrap())
            .is_err());
        assert!(
            detached.bytes().is_err(),
            "failed checked admission leaves transport detached"
        );
        let owner = facts.owner(sql.to_vec());
        detached
            .reattach(&owner, facts.admit(256).unwrap())
            .unwrap();
        assert_eq!(detached.bytes().unwrap(), b"a\xe9");
        assert_eq!(detached.segment_bytes(1).unwrap(), b"\xe9");
        let mut gap = encoded.clone();
        gap["segments"][1]["semantic_start"] = serde_json::json!(0);
        assert!(serde_json::from_value::<MySqlText>(gap).is_err());
        let mut overlap = encoded;
        overlap["segments"][1]["source"]["start"] = serde_json::json!(7);
        assert!(serde_json::from_value::<MySqlText>(overlap).is_err());
    }

    #[cfg(all(feature = "visitor", feature = "serde"))]
    #[test]
    fn final_partition_tail_keeps_original_raw_occurrences_spans_and_one_system_comment_conversion()
    {
        use crate::ast::{
            AlterTableOperation, AlterTableSetOperation, MySqlPartitionMaintenance, MySqlTextRole,
            Spanned, Visit, VisitMut, Visitor, VisitorMut,
        };
        use std::ops::ControlFlow;
        let facts = Facts::new("latin1");
        let sql = b"ALTER TABLE t ADD e ENUM('a','b') COMMENT 'column',ADD CHECK(e<>'x'),CONVERT TO CHARACTER SET utf8mb4,COMMENT='table\xe9' PARTITION BY LIST COLUMNS(e)(PARTITION p0 VALUES IN('a'),PARTITION p1 VALUES IN('b'))";
        let parsed = parsed(sql, &facts, false, false);
        assert_eq!(cpp(&parsed), sql);
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 2);
        assert_eq!(facts.system_text_live.load(Ordering::SeqCst), 2);
        assert_eq!(facts.system_role_text_calls.load(Ordering::SeqCst), 2);
        let Statement::AlterTable(alter) = &parsed.statement else {
            panic!("actual ALTER");
        };
        assert!(matches!(
            alter.operations.last(),
            Some(AlterTableOperation::SetOptionsParens {
                options: AlterTableSetOperation::MySqlPartition(
                    MySqlPartitionMaintenance::PartitionBy(_)
                )
            })
        ));
        assert_ne!(alter.span(), crate::tokenizer::Span::empty());
        let comment = alter.comment.as_ref().unwrap();
        assert!(matches!(&comment.value.value, Value::MySqlText(value)
            if value.system_bytes().unwrap() == "tableé".as_bytes()));
        let value_span = crate::tokenizer::Span::new(
            crate::tokenizer::Location::new(1, 111),
            crate::tokenizer::Location::new(1, 119),
        );
        assert_eq!(comment.value.span, value_span);
        assert_eq!(comment.value_token.0.span, value_span);
        assert_eq!(
            comment.span(),
            crate::tokenizer::Span::new(
                crate::tokenizer::Location::new(1, 103),
                crate::tokenizer::Location::new(1, 119),
            )
        );
        assert!(matches!(&comment.value_token.0.token,
            crate::tokenizer::Token::MySqlText(text)
                if text.role() == MySqlTextRole::Literal
                    && text.source().range() == (110..118)
                    && text.bytes().unwrap() == b"table\xe9"));
        // The value is decoded once by the System consumer. Its AttachedToken
        // separately retains the original lexical Literal, not a second
        // semantic table comment or a second conversion.
        let expected = [
            (
                MySqlTextRole::System,
                110..118,
                "tableé".as_bytes(),
                b"'table\xe9'".as_slice(),
            ),
            (
                MySqlTextRole::Literal,
                25..28,
                b"a".as_slice(),
                b"'a'".as_slice(),
            ),
            (
                MySqlTextRole::Literal,
                29..32,
                b"b".as_slice(),
                b"'b'".as_slice(),
            ),
            (
                MySqlTextRole::System,
                42..50,
                b"column".as_slice(),
                b"'column'".as_slice(),
            ),
            (
                MySqlTextRole::Literal,
                64..67,
                b"x".as_slice(),
                b"'x'".as_slice(),
            ),
            (
                MySqlTextRole::Literal,
                110..118,
                b"table\xe9".as_slice(),
                b"'table\xe9'".as_slice(),
            ),
            (
                MySqlTextRole::Literal,
                171..174,
                b"a".as_slice(),
                b"'a'".as_slice(),
            ),
            (
                MySqlTextRole::Literal,
                199..202,
                b"b".as_slice(),
                b"'b'".as_slice(),
            ),
        ];
        let attached_expected = expected
            .iter()
            .map(|(role, range, bytes, source)| {
                (
                    *role,
                    range.clone(),
                    Some(bytes.to_vec()),
                    Some(source.to_vec()),
                )
            })
            .collect::<Vec<_>>();
        let detached_expected = expected
            .iter()
            .map(|(role, range, _, _)| (*role, range.clone(), None, None))
            .collect::<Vec<_>>();
        struct Seen {
            values: Vec<(
                MySqlTextRole,
                core::ops::Range<usize>,
                Option<Vec<u8>>,
                Option<Vec<u8>>,
            )>,
            detached: bool,
        }
        impl Visitor for Seen {
            type Break = ();
            fn pre_visit_mysql_text(&mut self, value: &MySqlText) -> ControlFlow<()> {
                assert_eq!(value.bytes().is_err(), self.detached);
                assert_eq!(value.source().bytes().is_err(), self.detached);
                assert_eq!(value.source().owner().is_none(), self.detached);
                assert_eq!(
                    value.charset(),
                    if value.role() == MySqlTextRole::System {
                        "utf8mb3"
                    } else {
                        "latin1"
                    }
                );
                assert_eq!(value.quote(), '\'');
                assert_eq!(value.segments().len(), 1);
                self.values.push((
                    value.role(),
                    value.source().range(),
                    value.bytes().ok().map(<[u8]>::to_vec),
                    value.source().bytes().ok().map(<[u8]>::to_vec),
                ));
                ControlFlow::Continue(())
            }
        }
        let mut seen = Seen {
            values: Vec::new(),
            detached: false,
        };
        assert!(parsed.statement.visit(&mut seen).is_continue());
        seen.values.sort_by_key(|(_, range, _, _)| range.start);
        assert_eq!(
            seen.values, attached_expected,
            "each TYPELIB member, column comment, CHECK literal, table COMMENT source token and partition bound occurs once"
        );
        assert!(
            seen.values
                .windows(2)
                .all(|pair| pair[0].1.end <= pair[1].1.start
                    || pair[0].1 == pair[1].1 && pair[0].1 == (110..118)),
            "only the original AttachedToken and its same System semantic loan share a range"
        );
        let cloned = parsed.statement.clone();
        let mut cloned_seen = Seen {
            values: Vec::new(),
            detached: false,
        };
        assert!(cloned.visit(&mut cloned_seen).is_continue());
        cloned_seen
            .values
            .sort_by_key(|(_, range, _, _)| range.start);
        assert_eq!(cloned_seen.values, attached_expected);
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 2);
        let mut detached: Statement =
            serde_json::from_value(serde_json::to_value(&parsed.statement).unwrap()).unwrap();
        let mut transported = Seen {
            values: Vec::new(),
            detached: true,
        };
        assert!(detached.visit(&mut transported).is_continue());
        transported
            .values
            .sort_by_key(|(_, range, _, _)| range.start);
        assert_eq!(transported.values, detached_expected);
        let detached_comment = match &detached {
            Statement::AlterTable(alter) => alter.comment.as_ref().unwrap(),
            _ => panic!("transport lost ALTER"),
        };
        assert_eq!(detached_comment.value.value, comment.value.value);
        assert_eq!(detached_comment.value.span, value_span);
        assert_eq!(detached_comment.value_token.0.span, value_span);
        let owner = facts.owner(sql.to_vec());
        struct Attach<'a> {
            facts: &'a Facts,
            owner: &'a Arc<SqlSourceOwner>,
            count: usize,
        }
        impl VisitorMut for Attach<'_> {
            type Break = crate::tokenizer::MySqlSourceError;
            fn pre_visit_mysql_text(&mut self, value: &mut MySqlText) -> ControlFlow<Self::Break> {
                let result = self.facts.admit(1024).and_then(|admission| {
                    value.reattach(self.owner, admission).map_err(Into::into)
                });
                match result {
                    Ok(()) => {
                        self.count += 1;
                        ControlFlow::Continue(())
                    }
                    Err(error) => ControlFlow::Break(error),
                }
            }
        }
        let mut attach = Attach {
            facts: &facts,
            owner: &owner,
            count: 0,
        };
        assert!(VisitMut::visit(&mut detached, &mut attach).is_continue());
        assert_eq!(attach.count, 8);
        let mut reattached = Seen {
            values: Vec::new(),
            detached: false,
        };
        assert!(detached.visit(&mut reattached).is_continue());
        reattached
            .values
            .sort_by_key(|(_, range, _, _)| range.start);
        assert_eq!(reattached.values, attached_expected);
        assert_eq!(detached, parsed.statement);
        assert_eq!(facts.system_text_calls.load(Ordering::SeqCst), 2);
        assert_eq!(facts.system_role_text_calls.load(Ordering::SeqCst), 2);
        drop((detached, owner, cloned));
        assert_eq!(facts.system_text_live.load(Ordering::SeqCst), 2);
        drop(parsed);
        assert_eq!(facts.live.load(Ordering::SeqCst), 0);
        assert_eq!(facts.system_text_live.load(Ordering::SeqCst), 0);
    }
}

fn parse_one(sql: &str) -> Statement {
    let statements = Parser::parse_sql(&MySqlDialect {}, sql)
        .unwrap_or_else(|error| panic!("failed to parse `{sql}`: {error}"));
    let [statement] = statements.as_slice() else {
        panic!("expected one statement for `{sql}`: {statements:?}");
    };
    statement.clone()
}

#[test]
fn mysql_quoted_json_value_preserves_generic_function_source_identity() {
    fn expression(sql: &str) -> Expr {
        let Statement::Query(query) = parse_one(sql) else {
            panic!("expected SELECT")
        };
        let SetExpr::Select(select) = *query.body else {
            panic!("expected SELECT body")
        };
        let [SelectItem::UnnamedExpr(expression)] = select.projection.as_slice() else {
            panic!("expected one expression")
        };
        expression.clone()
    }

    assert!(matches!(
        expression("SELECT JSON_VALUE('{\"n\":1}', '$.n')"),
        Expr::JsonValue(_)
    ));
    for (sql, qualified) in [
        ("SELECT `JsOn_VaLuE`('{\"n\":1}', '$.n')", false),
        ("SELECT db.`JsOn_VaLuE`('{\"n\":1}', '$.n')", true),
    ] {
        let Expr::Function(function) = expression(sql) else {
            panic!("quoted JSON_VALUE must retain a generic source call")
        };
        assert_eq!(function.origin, crate::ast::FunctionOrigin::Source);
        assert_eq!(function.name.0.len(), if qualified { 2 } else { 1 });
        let identifier = function.name.0.last().unwrap().as_ident().unwrap();
        assert_eq!(identifier.value, "JsOn_VaLuE");
        assert_eq!(identifier.quote_style, Some('`'));
        let crate::ast::FunctionArguments::List(arguments) = function.args else {
            panic!("expected actual source arguments")
        };
        assert_eq!(arguments.args.len(), 2);
    }
    for sql in [
        "SELECT db.JSON_VALUE('{\"n\":1}', '$.n')",
        "SELECT db.EXTRACT(1)",
        "SELECT db.CEIL(1, 2)",
    ] {
        let Expr::Function(function) = expression(sql) else {
            panic!("qualified MySQL name must retain a generic source call: {sql}")
        };
        assert_eq!(function.origin, crate::ast::FunctionOrigin::Source);
        assert_eq!(function.name.0.len(), 2);
        assert_eq!(function.name.0[0].as_ident().unwrap().value, "db");
    }
    assert!(
        Parser::parse_sql(&MySqlDialect {}, "SELECT JSON_VALUE('{\"n\":1}', '$.n', 7)").is_err()
    );
}

#[test]
fn mysql_check_table_options_are_structured_and_round_trip_in_order() {
    let sql = "CHECK TABLE t, db.u FOR UPGRADE QUICK FAST MEDIUM EXTENDED CHANGED";
    let Statement::MySqlTableMaintenance {
        operation,
        location,
        tables,
        options,
        histogram,
    } = parse_one(sql)
    else {
        panic!("expected CHECK TABLE");
    };
    assert_eq!(operation.to_string(), "CHECK");
    assert_eq!(location, None);
    assert_eq!(tables.len(), 2);
    assert_eq!(
        options,
        vec![
            MySqlCheckTableOption::ForUpgrade,
            MySqlCheckTableOption::Quick,
            MySqlCheckTableOption::Fast,
            MySqlCheckTableOption::Medium,
            MySqlCheckTableOption::Extended,
            MySqlCheckTableOption::Changed,
        ]
    );
    assert_eq!(
        Statement::MySqlTableMaintenance {
            operation,
            location,
            tables,
            options,
            histogram,
        }
        .to_string(),
        sql
    );
    assert_eq!(
        parse_one("CHECK TABLE t extended").to_string(),
        "CHECK TABLE t EXTENDED"
    );
    assert!(Parser::parse_sql(&MySqlDialect {}, "CHECK TABLE t FOR missing").is_err());
    assert!(Parser::parse_sql(&MySqlDialect {}, "ANALYZE TABLE t EXTENDED").is_err());
    assert!(Parser::parse_sql(&MySqlDialect {}, "CHECK TABLE t `EXTENDED`").is_err());
}

#[test]
fn mysql_drop_check_preserves_its_kind_scoped_operation_and_source_identity() {
    for (sql, expected_name, quote) in [
        ("ALTER TABLE t DROP CHECK t_chk_1", "t_chk_1", None),
        ("ALTER TABLE t DROP CHECK `检查`", "检查", Some('`')),
        ("ALTER TABLE t DROP CONSTRAINT t_chk_1", "t_chk_1", None),
        ("ALTER TABLE t DROP CONSTRAINT `检查`", "检查", Some('`')),
    ] {
        let statement = parse_one(sql);
        let Statement::AlterTable(alter) = &statement else {
            panic!("expected ALTER TABLE");
        };
        let name = match alter.operations.as_slice() {
            [AlterTableOperation::DropCheck { name }] if sql.contains("DROP CHECK") => name,
            [AlterTableOperation::DropConstraint {
                if_exists: false,
                name,
                ..
            }] if sql.contains("DROP CONSTRAINT") => name,
            operations => panic!("lost the original CHECK/CONSTRAINT kind: {operations:?}"),
        };
        assert_eq!(name.value, expected_name);
        assert_eq!(name.quote_style, quote);
        assert_eq!(statement.to_string(), sql);
        assert_eq!(parse_one(&statement.to_string()), statement);
    }
    for sql in [
        "ALTER TABLE t DROP CHECK",
        "ALTER TABLE t DROP CHECK IF EXISTS t_chk_1",
        "ALTER TABLE t DROP CHECK t_chk_1 CASCADE",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
}

#[test]
fn mysql_sounds_like_is_a_structured_comparison_operator() {
    let sql = "SELECT 'mood' SOUNDS LIKE 'mud'";
    let Statement::Query(query) = parse_one(sql) else {
        panic!("expected SELECT")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("expected SELECT body")
    };
    assert!(matches!(
        select.projection.as_slice(),
        [SelectItem::UnnamedExpr(Expr::BinaryOp {
            op: BinaryOperator::Custom(operator),
            ..
        })] if operator == "SOUNDS LIKE"
    ));
    assert_eq!(parse_one(sql).to_string(), sql);
}

#[test]
fn mysql_end_is_an_unquoted_select_item_alias() {
    let sql = "SELECT (t1.a,t1.a) IN (('a','c'),('a','b')) END FROM t1";
    let Statement::Query(query) = parse_one(sql) else {
        panic!("expected SELECT")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("expected SELECT body")
    };
    assert!(matches!(
        select.projection.as_slice(),
        [SelectItem::ExprWithAlias { alias, .. }]
            if alias.value.eq_ignore_ascii_case("end")
    ));
    assert_eq!(select.from.len(), 1);
    let rendered = Statement::Query(query).to_string();
    assert_eq!(parse_one(&rendered), parse_one(sql));
}

#[test]
fn mysql_json_arrows_bind_before_comparisons() {
    let sql = "SELECT document->>'$.name' = 'goodyear' FROM documents";
    let Statement::Query(query) = parse_one(sql) else {
        panic!("expected SELECT")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("expected SELECT body")
    };
    assert!(matches!(
        select.projection.as_slice(),
        [SelectItem::UnnamedExpr(Expr::BinaryOp {
            left,
            op: BinaryOperator::Eq,
            ..
        })] if matches!(left.as_ref(), Expr::BinaryOp {
            op: BinaryOperator::LongArrow,
            ..
        })
    ));
    let rendered = parse_one(sql).to_string();
    assert_eq!(parse_one(&rendered), parse_one(sql));
}

#[test]
fn mysql_set_accepts_colon_equals_without_losing_typed_assignment() {
    let statement = parse_one("SET @a:=1,@b := @a + 1");
    let Statement::Set(Set::MultipleAssignments { assignments }) = &statement else {
        panic!("expected MySQL SET assignments: {statement:?}");
    };
    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[0].name.to_string(), "@a");
    assert_eq!(assignments[0].value.to_string(), "1");
    assert_eq!(assignments[1].name.to_string(), "@b");
    assert!(matches!(&assignments[1].value, Expr::BinaryOp { .. }));
    assert_eq!(statement.to_string(), "SET @a = 1, @b = @a + 1");

    assert!(Parser::parse_sql(&crate::dialect::PostgreSqlDialect {}, "SET a := 1").is_err());
}

#[test]
fn mysql_cast_character_set_and_zerofill_are_structured() {
    let Statement::Query(query) =
        parse_one("SELECT CAST(_koi8r x'D4C5D3D4' AS CHAR CHARACTER SET 'cp1251')")
    else {
        panic!("expected SELECT")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("expected SELECT body")
    };
    let [SelectItem::UnnamedExpr(Expr::Convert {
        data_type: Some(DataType::Char(None)),
        charset: Some(charset),
        ..
    })] = select.projection.as_slice()
    else {
        panic!("expected charset conversion: {:?}", select.projection)
    };
    assert_eq!(charset.to_string(), "'cp1251'");

    let Statement::Query(query) = parse_one("SELECT CAST('a' AS CHAR(2) BINARY)") else {
        panic!("expected SELECT")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("expected SELECT body")
    };
    assert!(matches!(
        select.projection.as_slice(),
        [SelectItem::UnnamedExpr(Expr::Convert {
            data_type: Some(DataType::Char(Some(_))),
            charset: None,
            binary_attribute: true,
            ..
        })]
    ));

    let Statement::CreateTable(table) = parse_one("CREATE TABLE t(a INT(2) ZEROFILL)") else {
        panic!("expected CREATE TABLE")
    };
    assert!(matches!(
        table.columns[0].options[0].option,
        ColumnOption::DialectSpecific(ref tokens)
            if tokens.len() == 1 && tokens[0].to_string().eq_ignore_ascii_case("ZEROFILL")
    ));
    assert_eq!(table.to_string(), "CREATE TABLE t (a INT(2) ZEROFILL)");
}

#[test]
fn mysql_cast_rejects_double_dimensions_without_restricting_float_precision() {
    assert!(Parser::parse_sql(&MySqlDialect {}, "SELECT CAST(1 AS DOUBLE(52))").is_err());
    assert!(Parser::parse_sql(&MySqlDialect {}, "SELECT CAST(1 AS DOUBLE(10, 2))").is_err());
    assert!(Parser::parse_sql(&MySqlDialect {}, "SELECT CAST(1 AS FLOAT(7, 4))").is_err());
    parse_one("SELECT CAST(1 AS DOUBLE), CAST(1 AS DOUBLE PRECISION)");
    parse_one("SELECT CAST(1 AS FLOAT(53))");
}

#[test]
fn mysql_preserves_unary_binary_cast_identity() {
    let Statement::Query(query) = parse_one("SELECT BINARY value, CAST(value AS BINARY)") else {
        panic!("expected SELECT")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("expected SELECT body")
    };
    assert!(matches!(
        select.projection.as_slice(),
        [
            SelectItem::UnnamedExpr(Expr::Cast {
                kind: CastKind::Binary,
                data_type: DataType::Binary(None),
                ..
            }),
            SelectItem::UnnamedExpr(Expr::Cast {
                kind: CastKind::Cast,
                data_type: DataType::Binary(None),
                ..
            })
        ]
    ));
    assert_eq!(
        query.to_string(),
        "SELECT BINARY value, CAST(value AS BINARY)"
    );
}

#[test]
fn mysql_unquoted_odd_hex_literal_pads_the_leading_nibble_only() {
    assert_eq!(parse_one("SELECT 0x123").to_string(), "SELECT X'0123'");
    assert_eq!(parse_one("SELECT X'0123'").to_string(), "SELECT X'0123'");
    assert!(Parser::parse_sql(&MySqlDialect {}, "SELECT X'123'").is_ok());
}

#[test]
fn mysql_bit_literals_share_one_byte_oriented_literal_path() {
    for (unquoted, quoted, hex) in [
        ("0b0", "b'0'", "X'00'"),
        ("0b1", "B'1'", "X'01'"),
        ("0b11111111", "b'11111111'", "X'ff'"),
        (
            "0b1111111111111111111111111111111111111111111",
            "b'1111111111111111111111111111111111111111111'",
            "X'07ffffffffff'",
        ),
        (
            "0b1111111111111111111111111111111111111111111111111111111111111111",
            "b'1111111111111111111111111111111111111111111111111111111111111111'",
            "X'ffffffffffffffff'",
        ),
    ] {
        assert_eq!(
            parse_one(&format!("SELECT {unquoted}")).to_string(),
            format!("SELECT {hex}")
        );
        assert_eq!(
            parse_one(&format!("SELECT {quoted}")).to_string(),
            format!("SELECT {hex}")
        );
    }
}

#[test]
fn mysql_invalid_unquoted_bit_prefix_remains_an_identifier() {
    assert_eq!(parse_one("SELECT 0b102").to_string(), "SELECT 0b102");
    assert_eq!(parse_one("SELECT 0binary").to_string(), "SELECT 0binary");
    // sql_lex.cc MY_LEX_BIN_NUMBER accepts zero digits; Item_bin_string
    // preserves the actual zero-byte value, exactly as X''.
    assert_eq!(parse_one("SELECT b''"), parse_one("SELECT X''"));
    assert!(Parser::parse_sql(&MySqlDialect {}, "SELECT b'102'").is_err());
}

#[test]
fn mysql_nchar_cast_is_a_distinct_structured_type() {
    for (sql, rendered, expected) in [
        (
            "SELECT CAST(_koi8r x'C6C7' AS NCHAR)",
            "SELECT CAST(_koi8r X'C6C7' AS NCHAR)",
            DataType::Nchar(None),
        ),
        (
            "SELECT CAST(_koi8r x'C6C7C8' AS NCHAR(2))",
            "SELECT CAST(_koi8r X'C6C7C8' AS NCHAR(2))",
            DataType::Nchar(Some(crate::ast::CharacterLength::IntegerLength {
                length: 2,
                unit: None,
            })),
        ),
        (
            "SELECT CAST(_koi8r x'C6C7' AS NATIONAL CHAR)",
            "SELECT CAST(_koi8r X'C6C7' AS NCHAR)",
            DataType::Nchar(None),
        ),
        (
            "SELECT CAST(_koi8r x'C6C7C8' AS NATIONAL CHARACTER(2))",
            "SELECT CAST(_koi8r X'C6C7C8' AS NCHAR(2))",
            DataType::Nchar(Some(crate::ast::CharacterLength::IntegerLength {
                length: 2,
                unit: None,
            })),
        ),
    ] {
        let Statement::Query(query) = parse_one(sql) else {
            panic!("expected SELECT")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("expected SELECT body")
        };
        let [SelectItem::UnnamedExpr(Expr::Cast { data_type, .. })] = select.projection.as_slice()
        else {
            panic!("expected CAST expression: {select:?}")
        };
        assert_eq!(data_type, &expected);
        assert_eq!(parse_one(sql).to_string(), rendered);
    }
    assert!(Parser::parse_sql(&MySqlDialect {}, "SELECT CAST('x' AS NATIONAL INT)").is_err());
}

#[test]
fn mysql_column_visibility_options_preserve_declaration_order() {
    let sql = "CREATE TABLE t (f1 INT, f2 INT INVISIBLE VISIBLE, f3 INT VISIBLE INVISIBLE)";
    let Statement::CreateTable(table) = parse_one(sql) else {
        panic!("expected CREATE TABLE")
    };
    assert_eq!(
        table.columns[1]
            .options
            .iter()
            .map(|option| &option.option)
            .collect::<Vec<_>>(),
        vec![&ColumnOption::Invisible, &ColumnOption::Visible]
    );
    assert_eq!(
        table.columns[2]
            .options
            .iter()
            .map(|option| &option.option)
            .collect::<Vec<_>>(),
        vec![&ColumnOption::Visible, &ColumnOption::Invisible]
    );
    assert_eq!(table.to_string(), sql);
    assert!(Parser::parse_sql(&MySqlDialect {}, "CREATE TABLE t(f1 INT VISIBILITY)").is_err());
}

#[test]
fn mysql_alter_column_visibility_has_structured_ast() {
    for (sql, expected_visible) in [
        ("ALTER TABLE t ALTER COLUMN c SET INVISIBLE", false),
        ("ALTER TABLE t ALTER COLUMN c SET VISIBLE", true),
    ] {
        let statement = parse_one(sql);
        assert_eq!(statement.to_string(), sql);
        let Statement::AlterTable(alter_table) = statement else {
            panic!("expected ALTER TABLE");
        };
        let [AlterTableOperation::AlterColumn {
            op: AlterColumnOperation::SetVisibility { visible },
            ..
        }] = alter_table.operations.as_slice()
        else {
            panic!("expected structured column visibility operation");
        };
        assert_eq!(*visible, expected_visible);
    }

    assert!(Parser::parse_sql(
        &MySqlDialect {},
        "ALTER TABLE t ALTER COLUMN c SET INVISIBLE EXTRA",
    )
    .is_err());
}

#[test]
fn mysql_alter_drop_key_is_drop_index_alias() {
    let statement = parse_one("ALTER TABLE t DROP KEY by_name, ADD KEY (name(20))");
    let Statement::AlterTable(alter) = statement else {
        panic!("expected ALTER TABLE");
    };
    assert!(matches!(
        alter.operations.first(),
        Some(AlterTableOperation::DropIndex { name }) if name.value == "by_name"
    ));
}

#[test]
fn trim_omitted_character_preserves_source_and_direction() {
    for sql in [
        "SELECT TRIM(LEADING FROM '  left  ')",
        "SELECT TRIM(TRAILING FROM '  right  ')",
        "SELECT TRIM(BOTH FROM '  both  ')",
        "SELECT TRIM(LEADING 'ab' FROM 'ababc')",
        "SELECT TRIM('  plain  ')",
    ] {
        let statement = parse_one(sql);
        assert_eq!(statement.to_string(), sql);
        assert_eq!(parse_one(&statement.to_string()), statement);
    }
}

fn mysql_partition_by(statement: &Statement) -> &MySqlPartitionBy {
    let Statement::AlterTable(alter) = statement else {
        panic!("expected ALTER TABLE: {statement:?}");
    };
    let [AlterTableOperation::SetOptionsParens {
        options:
            AlterTableSetOperation::MySqlPartition(MySqlPartitionMaintenance::PartitionBy(partitioning)),
    }] = alter.operations.as_slice()
    else {
        panic!("expected typed PARTITION BY: {statement:?}");
    };
    partitioning
}

#[test]
fn rename_table_keeps_every_pair_in_one_statement() {
    let statement =
        parse_one("RENAME /* keep identity */ TABLE `old one` TO `new one`, app.t2 TO archive.t2");
    let Statement::RenameTable(renames) = &statement else {
        panic!("expected RENAME TABLE: {statement:?}");
    };
    assert_eq!(renames.len(), 2);
    assert_eq!(
        statement.to_string(),
        "RENAME TABLE `old one` TO `new one`, app.t2 TO archive.t2"
    );
}

#[test]
fn mysql_specific_statements_round_trip_with_typed_payloads() {
    let delete = parse_one("DELETE LOW_PRIORITY QUICK IGNORE FROM `items` WHERE id = ?");
    let Statement::Delete(delete_ast) = &delete else {
        panic!("expected DELETE: {delete:?}");
    };
    assert_eq!(
        delete_ast.mysql_modifiers().collect::<Vec<_>>(),
        vec![
            MySqlDeleteModifier::LowPriority,
            MySqlDeleteModifier::Quick,
            MySqlDeleteModifier::Ignore,
        ]
    );
    assert_eq!(
        delete.to_string(),
        "DELETE LOW_PRIORITY QUICK IGNORE FROM `items` WHERE id = ?"
    );

    let global = parse_one("SET /* comment */ GLOBAL TRANSACTION READ ONLY");
    assert!(matches!(
        global,
        Statement::Set(Set::SetGlobalTransaction { ref modes }) if modes.len() == 1
    ));
    assert_eq!(global.to_string(), "SET GLOBAL TRANSACTION READ ONLY");

    let database =
        parse_one("ALTER DATABASE `tenant db` DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin");
    assert!(matches!(
        database,
        Statement::AlterSchema(ref alter) if matches!(
            alter.operations.as_slice(),
            [AlterSchemaOperation::SetOptionsParens {
                options: AlterSchemaSetOperation::MySqlDatabaseDefaults { .. }
            }]
        )
    ));
    assert_eq!(
        database.to_string(),
        "ALTER DATABASE `tenant db` DEFAULT CHARACTER SET = utf8mb4 DEFAULT COLLATE = utf8mb4_bin"
    );
}

#[test]
fn alter_table_character_operations_have_different_ast_identities() {
    let defaults = parse_one("ALTER TABLE t DEFAULT CHARACTER SET utf8mb4");
    let convert = parse_one("ALTER TABLE t CONVERT TO CHARACTER SET utf8mb4");
    assert!(matches!(
        defaults,
        Statement::AlterTable(ref alter) if matches!(
            alter.operations.as_slice(),
            [AlterTableOperation::SetOptionsParens {
                options: AlterTableSetOperation::MySqlDefaultCharacterSet { .. }
            }]
        )
    ));
    assert!(matches!(
        convert,
        Statement::AlterTable(ref alter) if matches!(
            alter.operations.as_slice(),
            [AlterTableOperation::SetOptionsParens {
                options: AlterTableSetOperation::MySqlConvertToCharacterSet { .. }
            }]
        )
    ));
}

#[test]
fn partition_maintenance_and_key_algorithm_are_typed() {
    let exchange = parse_one(
        "ALTER TABLE `orders` EXCHANGE PARTITION `p 0` WITH TABLE staging.orders WITHOUT VALIDATION",
    );
    assert!(matches!(
        exchange,
        Statement::AlterTable(ref alter) if matches!(
            alter.operations.as_slice(),
            [AlterTableOperation::SetOptionsParens {
                options: AlterTableSetOperation::MySqlPartition(
                    MySqlPartitionMaintenance::Exchange {
                        validation: MySqlExchangeValidation::Without,
                        ..
                    }
                )
            }]
        )
    ));

    let keyed = parse_one(
        "ALTER TABLE orders PARTITION BY LINEAR KEY ALGORITHM=2 (`tenant`, id) PARTITIONS 7",
    );
    assert!(matches!(
        keyed,
        Statement::AlterTable(ref alter) if matches!(
            alter.operations.as_slice(),
            [AlterTableOperation::SetOptionsParens {
                options: AlterTableSetOperation::MySqlPartition(
                    MySqlPartitionMaintenance::PartitionBy(partitioning)
                )
            }] if matches!(
                &partitioning.method,
                MySqlPartitionMethod::Key {
                    linear: true,
                    algorithm: Some(MySqlKeyAlgorithm::Algorithm2),
                    columns,
                } if columns.len() == 2
            ) && partitioning.partitions == Some(7)
        )
    ));

    for sql in [
        "ALTER TABLE orders PARTITION BY RANGE(id) (PARTITION p0 VALUES LESS THAN (10), PARTITION pmax VALUES LESS THAN MAXVALUE)",
        "ALTER TABLE orders PARTITION BY RANGE COLUMNS(`tenant`, name) (PARTITION p0 VALUES LESS THAN (10, 'm'), PARTITION pmax VALUES LESS THAN (MAXVALUE, MAXVALUE))",
        "ALTER TABLE orders PARTITION BY LIST(MOD(id, 4)) (PARTITION p0 VALUES IN (0, 2), PARTITION p1 VALUES IN (1, 3))",
        "ALTER TABLE orders PARTITION BY LIST COLUMNS(`tenant`, name) (PARTITION p0 VALUES IN ((1, 'a'), (2, NULL)), PARTITION p1 VALUES IN ((3, 'z')))",
        "ALTER TABLE orders PARTITION BY HASH(id) PARTITIONS 4",
        "ALTER TABLE orders PARTITION BY HASH(id) PARTITIONS 2 (PARTITION p0 ENGINE=InnoDB, PARTITION p1 COMMENT='cold')",
        "ALTER TABLE orders PARTITION BY LINEAR HASH(id) PARTITIONS 6",
        "ALTER TABLE orders PARTITION BY KEY() PARTITIONS 2",
        "ALTER TABLE orders PARTITION BY RANGE(id) SUBPARTITION BY HASH(bucket) SUBPARTITIONS 2 (PARTITION p0 VALUES LESS THAN (10) (SUBPARTITION p0s0, SUBPARTITION p0s1), PARTITION p1 VALUES LESS THAN MAXVALUE (SUBPARTITION p1s0, SUBPARTITION p1s1))",
        "ALTER TABLE orders PARTITION BY RANGE(id) SUBPARTITION BY LINEAR KEY ALGORITHM=1 (`tenant`) SUBPARTITIONS 2 (PARTITION p0 VALUES LESS THAN MAXVALUE STORAGE ENGINE=InnoDB COMMENT='hot' DATA DIRECTORY='/data' INDEX DIRECTORY='/index' MAX_ROWS=100 MIN_ROWS=1 TABLESPACE=ts NODEGROUP=2 (SUBPARTITION p0s0 ENGINE=InnoDB, SUBPARTITION p0s1 COMMENT='cold'))",
    ] {
        let statement = parse_one(sql);
        Parser::parse_sql(&MySqlDialect {}, &statement.to_string())
            .unwrap_or_else(|error| panic!("partition roundtrip failed for `{sql}`: {error}"));
    }

    let range_columns = parse_one(
        "ALTER TABLE orders PARTITION BY RANGE COLUMNS(`tenant`, name) (PARTITION p0 VALUES LESS THAN (10, 'm'), PARTITION pmax VALUES LESS THAN (MAXVALUE, MAXVALUE))",
    );
    assert!(matches!(
        range_columns,
        Statement::AlterTable(ref alter) if matches!(
            alter.operations.as_slice(),
            [AlterTableOperation::SetOptionsParens {
                options: AlterTableSetOperation::MySqlPartition(
                    MySqlPartitionMaintenance::PartitionBy(partitioning)
                )
            }] if matches!(
                &partitioning.method,
                MySqlPartitionMethod::RangeColumns { columns } if columns.len() == 2
            ) && partitioning.definitions.len() == 2
        )
    ));

    let subpartitioned = parse_one(
        "ALTER TABLE orders PARTITION BY RANGE(id) SUBPARTITION BY LINEAR KEY ALGORITHM=1 (`tenant`) SUBPARTITIONS 2 (PARTITION p0 VALUES LESS THAN MAXVALUE STORAGE ENGINE=InnoDB COMMENT='hot' DATA DIRECTORY='/data' INDEX DIRECTORY='/index' MAX_ROWS=100 MIN_ROWS=1 TABLESPACE=ts NODEGROUP=2 (SUBPARTITION p0s0 ENGINE=InnoDB, SUBPARTITION p0s1 COMMENT='cold'))",
    );
    assert!(matches!(
        subpartitioned,
        Statement::AlterTable(ref alter) if matches!(
            alter.operations.as_slice(),
            [AlterTableOperation::SetOptionsParens {
                options: AlterTableSetOperation::MySqlPartition(
                    MySqlPartitionMaintenance::PartitionBy(partitioning)
                )
            }] if matches!(
                partitioning.subpartition.as_ref().map(|subpartition| &subpartition.method),
                Some(MySqlSubpartitionMethod::Key {
                    linear: true,
                    algorithm: Some(MySqlKeyAlgorithm::Algorithm1),
                    ..
                })
            ) && matches!(
                partitioning.definitions[0].options.as_slice(),
                [
                    MySqlPartitionOption::Engine { storage: true, .. },
                    MySqlPartitionOption::Comment(_),
                    MySqlPartitionOption::DataDirectory(_),
                    MySqlPartitionOption::IndexDirectory(_),
                    MySqlPartitionOption::MaxRows(100),
                    MySqlPartitionOption::MinRows(1),
                    MySqlPartitionOption::Tablespace(_),
                    MySqlPartitionOption::Nodegroup(2),
                ]
            ) && matches!(
                partitioning.definitions[0].subpartitions.as_slice(),
                [
                    crate::ast::MySqlSubpartitionDefinition { options, .. },
                    crate::ast::MySqlSubpartitionDefinition { .. },
                ] if matches!(options.as_slice(), [MySqlPartitionOption::Engine { storage: false, .. }])
            )
        )
    ));

    for sql in [
        "ALTER TABLE orders ADD PARTITION (PARTITION pmax VALUES LESS THAN MAXVALUE)",
        "ALTER TABLE orders ADD PARTITION (PARTITION p2 VALUES IN ((1, 'a'), (2, 'b')))",
        "ALTER TABLE orders ADD PARTITION PARTITIONS 2",
        "ALTER TABLE orders REORGANIZE PARTITION p0, p1 INTO (PARTITION p0a VALUES LESS THAN (10), PARTITION p1a VALUES LESS THAN (MAXVALUE))",
        "ALTER TABLE orders DROP PARTITION p0, p1",
        "ALTER TABLE orders ANALYZE PARTITION ALL",
        "ALTER TABLE orders CHECK PARTITION p0",
        "ALTER TABLE orders OPTIMIZE PARTITION p0",
        "ALTER TABLE orders REBUILD PARTITION p0",
        "ALTER TABLE orders REPAIR PARTITION p0",
        "ALTER TABLE orders TRUNCATE PARTITION ALL",
        "ALTER TABLE orders REMOVE PARTITIONING",
    ] {
        let statement = parse_one(sql);
        assert!(matches!(
            statement,
            Statement::AlterTable(ref alter) if matches!(
                alter.operations.as_slice(),
                [AlterTableOperation::SetOptionsParens {
                    options: AlterTableSetOperation::MySqlPartition(_)
                }]
            )
        ));
        Parser::parse_sql(&MySqlDialect {}, &statement.to_string())
            .unwrap_or_else(|error| panic!("roundtrip failed for `{sql}`: {error}"));
    }
}

#[test]
fn every_mysql_partition_method_has_a_distinct_typed_identity() {
    let range =
        parse_one("ALTER TABLE t PARTITION BY RANGE(id) (PARTITION p VALUES LESS THAN MAXVALUE)");
    assert!(matches!(
        &mysql_partition_by(&range).method,
        MySqlPartitionMethod::Range { .. }
    ));

    let range_columns = parse_one(
        "ALTER TABLE t PARTITION BY RANGE COLUMNS(a,b) (PARTITION p VALUES LESS THAN (MAXVALUE,MAXVALUE))",
    );
    assert!(matches!(
        &mysql_partition_by(&range_columns).method,
        MySqlPartitionMethod::RangeColumns { .. }
    ));

    let list = parse_one("ALTER TABLE t PARTITION BY LIST(id) (PARTITION p VALUES IN (1))");
    assert!(matches!(
        &mysql_partition_by(&list).method,
        MySqlPartitionMethod::List { .. }
    ));

    let list_columns =
        parse_one("ALTER TABLE t PARTITION BY LIST COLUMNS(a,b) (PARTITION p VALUES IN ((1,2)))");
    assert!(matches!(
        &mysql_partition_by(&list_columns).method,
        MySqlPartitionMethod::ListColumns { .. }
    ));

    let hash = parse_one("ALTER TABLE t PARTITION BY HASH(id) PARTITIONS 2");
    assert!(matches!(
        &mysql_partition_by(&hash).method,
        MySqlPartitionMethod::Hash { linear: false, .. }
    ));
    let linear_hash = parse_one("ALTER TABLE t PARTITION BY LINEAR HASH(id) PARTITIONS 3");
    assert!(matches!(
        &mysql_partition_by(&linear_hash).method,
        MySqlPartitionMethod::Hash { linear: true, .. }
    ));

    let key = parse_one("ALTER TABLE t PARTITION BY KEY() PARTITIONS 2");
    assert!(matches!(
        &mysql_partition_by(&key).method,
        MySqlPartitionMethod::Key { linear: false, .. }
    ));
    assert_eq!(
        mysql_partition_by(&key).method.effective_key_algorithm(),
        Some(MySqlKeyAlgorithm::Algorithm2)
    );
    let linear_key =
        parse_one("ALTER TABLE t PARTITION BY LINEAR KEY ALGORITHM=1(id) PARTITIONS 3");
    assert!(matches!(
        &mysql_partition_by(&linear_key).method,
        MySqlPartitionMethod::Key {
            linear: true,
            algorithm: Some(MySqlKeyAlgorithm::Algorithm1),
            ..
        }
    ));
}

#[test]
fn malformed_mysql_statement_corpus_is_rejected() {
    for sql in [
        "RENAME TABLE a TO b, c",
        "DELETE QUICK LOW_PRIORITY FROM t",
        "DELETE IGNORE IGNORE FROM t",
        "SET GLOBAL TRANSACTION SNAPSHOT 'x'",
        "ALTER DATABASE db DEFAULT CHARACTER SET",
        "ALTER DATABASE db DEFAULT COLLATE c DEFAULT COLLATE d",
        "ALTER TABLE t COALESCE PARTITION 0",
        "ALTER TABLE t ADD PARTITION ()",
        "ALTER TABLE t ADD PARTITION (PARTITION p VALUES IN (MAXVALUE))",
        "ALTER TABLE t REORGANIZE PARTITION p INTO ()",
        "ALTER TABLE t EXCHANGE PARTITION p WITH TABLE q WITH MAYBE",
        "ALTER TABLE t PARTITION BY KEY ALGORITHM=0 (id)",
        "ALTER TABLE t PARTITION BY KEY ALGORITHM=3 (id)",
        "ALTER TABLE t PARTITION BY KEY (id) PARTITIONS 0",
        "ALTER TABLE t PARTITION BY LINEAR RANGE(id) (PARTITION p VALUES LESS THAN MAXVALUE)",
        "ALTER TABLE t PARTITION BY HASH() PARTITIONS 2",
        "ALTER TABLE t PARTITION BY RANGE COLUMNS() (PARTITION p VALUES LESS THAN MAXVALUE)",
        "ALTER TABLE t PARTITION BY KEY(id, ID) PARTITIONS 2",
        "ALTER TABLE t PARTITION BY RANGE COLUMNS(id, ID) (PARTITION p VALUES LESS THAN (1, 2))",
        "ALTER TABLE t PARTITION BY RANGE(id)",
        "ALTER TABLE t PARTITION BY LIST(id)",
        "ALTER TABLE t PARTITION BY HASH(id) (PARTITION p VALUES LESS THAN (10))",
        "ALTER TABLE t PARTITION BY RANGE(id) (PARTITION p VALUES IN (1))",
        "ALTER TABLE t PARTITION BY LIST(id) (PARTITION p VALUES LESS THAN (10))",
        "ALTER TABLE t PARTITION BY RANGE COLUMNS(a,b) (PARTITION p VALUES LESS THAN (10))",
        "ALTER TABLE t PARTITION BY LIST COLUMNS(a,b) (PARTITION p VALUES IN ((1,2),3))",
        "ALTER TABLE t PARTITION BY HASH(id) SUBPARTITION BY HASH(id) PARTITIONS 2",
        "ALTER TABLE t PARTITION BY RANGE(id) SUBPARTITION BY RANGE(id) (PARTITION p VALUES LESS THAN MAXVALUE)",
        "ALTER TABLE t PARTITION BY RANGE(id) SUBPARTITION BY HASH(id) SUBPARTITIONS 2 (PARTITION p VALUES LESS THAN MAXVALUE (SUBPARTITION s0))",
        "ALTER TABLE t PARTITION BY RANGE(id) (PARTITION p VALUES LESS THAN (10), PARTITION P VALUES LESS THAN MAXVALUE)",
        "ALTER TABLE t PARTITION BY RANGE(id) SUBPARTITION BY HASH(id) (PARTITION p0 VALUES LESS THAN (10) (SUBPARTITION same), PARTITION p1 VALUES LESS THAN MAXVALUE (SUBPARTITION SAME))",
        "ALTER TABLE t PARTITION BY RANGE(id) (PARTITION p VALUES LESS THAN MAXVALUE COMMENT='a' COMMENT='b')",
        "ALTER TABLE t PARTITION BY RANGE(id) (PARTITION p VALUES LESS THAN MAXVALUE COMMENT=unquoted)",
        "ALTER TABLE t PARTITION BY RANGE(id) PARTITIONS 2 (PARTITION p VALUES LESS THAN MAXVALUE)",
        "ALTER TABLE t PARTITION BY RANGE(id) SUBPARTITION BY HASH(id) SUBPARTITIONS 8192 (PARTITION p0 VALUES LESS THAN (10), PARTITION p1 VALUES LESS THAN MAXVALUE)",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
}

#[test]
fn mysql_composite_interval_units_map_to_existing_interval_fields() {
    for (unit, leading, last) in [
        ("DAY_SECOND", DateTimeField::Day, DateTimeField::Second),
        ("DAY_MINUTE", DateTimeField::Day, DateTimeField::Minute),
        ("DAY_HOUR", DateTimeField::Day, DateTimeField::Hour),
        (
            "DAY_MICROSECOND",
            DateTimeField::Day,
            DateTimeField::Microsecond,
        ),
        ("HOUR_MINUTE", DateTimeField::Hour, DateTimeField::Minute),
        ("HOUR_SECOND", DateTimeField::Hour, DateTimeField::Second),
        (
            "HOUR_MICROSECOND",
            DateTimeField::Hour,
            DateTimeField::Microsecond,
        ),
        (
            "MINUTE_SECOND",
            DateTimeField::Minute,
            DateTimeField::Second,
        ),
        (
            "MINUTE_MICROSECOND",
            DateTimeField::Minute,
            DateTimeField::Microsecond,
        ),
        (
            "SECOND_MICROSECOND",
            DateTimeField::Second,
            DateTimeField::Microsecond,
        ),
        ("YEAR_MONTH", DateTimeField::Year, DateTimeField::Month),
    ] {
        let statement = parse_one(&format!("SELECT INTERVAL '1' {unit}"));
        let Statement::Query(query) = &statement else {
            panic!("expected SELECT: {statement:?}");
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("expected SELECT body: {statement:?}");
        };
        let [SelectItem::UnnamedExpr(Expr::Interval(interval))] = select.projection.as_slice()
        else {
            panic!("expected interval expression: {statement:?}");
        };
        assert_eq!(interval.leading_field.as_ref(), Some(&leading), "{unit}");
        assert_eq!(interval.last_field.as_ref(), Some(&last), "{unit}");
        Parser::parse_sql(&MySqlDialect {}, &statement.to_string())
            .unwrap_or_else(|error| panic!("roundtrip failed for {unit}: {error}"));
    }

    for invalid in ["DAY_YEAR", "HOUR_DAY", "MONTH_YEAR", "MICROSECOND_SECOND"] {
        let sql = format!("SELECT INTERVAL '1' {invalid}");
        assert!(Parser::parse_sql(&MySqlDialect {}, &sql).is_err(), "{sql}");
    }
    for invalid in [
        "SELECT INTERVAL '1' DAY TO YEAR",
        "SELECT INTERVAL '1' HOUR TO DAY",
        "SELECT INTERVAL '1' MONTH TO YEAR",
        "SELECT INTERVAL '1' MICROSECOND TO SECOND",
    ] {
        assert!(
            Parser::parse_sql(&MySqlDialect {}, invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn mysql_extract_accepts_only_mysql_composite_interval_units() {
    for unit in [
        "YEAR_MONTH",
        "DAY_HOUR",
        "DAY_MINUTE",
        "DAY_SECOND",
        "DAY_MICROSECOND",
        "HOUR_MINUTE",
        "HOUR_SECOND",
        "HOUR_MICROSECOND",
        "MINUTE_SECOND",
        "MINUTE_MICROSECOND",
        "SECOND_MICROSECOND",
    ] {
        let sql = format!("SELECT EXTRACT({unit} FROM '1999-01-02 10:11:12')");
        let statement = parse_one(&sql);
        let Statement::Query(query) = &statement else {
            panic!("expected SELECT: {statement:?}");
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("expected SELECT body: {statement:?}");
        };
        assert!(matches!(
            select.projection.as_slice(),
            [SelectItem::UnnamedExpr(Expr::Extract {
                field: DateTimeField::Custom(field),
                ..
            })] if field.value == unit
        ));
        Parser::parse_sql(&MySqlDialect {}, &statement.to_string())
            .unwrap_or_else(|error| panic!("roundtrip failed for {unit}: {error}"));
    }
    for invalid in ["DAY_YEAR", "HOUR_DAY", "MONTH_YEAR"] {
        assert!(
            Parser::parse_sql(
                &MySqlDialect {},
                &format!("SELECT EXTRACT({invalid} FROM '2000-01-01')")
            )
            .is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn mysql_char_using_character_set_is_preserved() {
    assert_eq!(
        parse_one("SELECT CHAR(0xff,0x8f USING utf8)").to_string(),
        "SELECT CHAR(X'ff', X'8f' USING utf8)"
    );
}

#[test]
fn mysql_select_into_file_is_structured_and_roundtrips() {
    let statement = parse_one(
        "SELECT a INTO OUTFILE '/tmp/result.tsv' CHARACTER SET utf8mb4 \
         FIELDS TERMINATED BY ',' OPTIONALLY ENCLOSED BY '\"' ESCAPED BY '\\\\' \
         LINES STARTING BY '>' TERMINATED BY '\\n' FROM t",
    );
    let Statement::Query(query) = &statement else {
        panic!("expected SELECT")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("expected SELECT body")
    };
    let Some(SelectIntoTarget::File(file)) =
        select.into.as_ref().map(|destination| &destination.target)
    else {
        panic!("expected OUTFILE destination")
    };
    assert_eq!(file.path, "/tmp/result.tsv");
    assert_eq!(file.fields_terminated_by.as_deref(), Some(","));
    assert_eq!(file.fields_enclosed_by.as_deref(), Some("\""));
    assert!(file.fields_optionally_enclosed);
    assert_eq!(file.lines_starting_by.as_deref(), Some(">"));
    Parser::parse_sql(&MySqlDialect {}, &statement.to_string()).unwrap();

    let trailing = parse_one("SELECT a FROM t INTO DUMPFILE '/tmp/result.bin'");
    let Statement::Query(query) = trailing else {
        panic!("expected SELECT")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("expected SELECT body")
    };
    assert!(select
        .into
        .as_ref()
        .map(|destination| &destination.target)
        .is_some_and(|target| matches!(target, SelectIntoTarget::File(file) if file.dumpfile)));
}

#[test]
fn mysql_select_into_user_variable_list_is_structured_and_roundtrips() {
    for (sql, first, second) in [
        ("SELECT 1, 'two' INTO @first, @second", "@first", "@second"),
        (
            "SELECT 1, 'two' FROM t WHERE id = 1 INTO @first, @second",
            "@first",
            "@second",
        ),
        (
            "SELECT 1, 'two' INTO local_a, local_b",
            "local_a",
            "local_b",
        ),
    ] {
        let statement = parse_one(sql);
        let Statement::Query(query) = &statement else {
            panic!("expected SELECT")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("expected SELECT body")
        };
        let SelectIntoTarget::Variables(variables) = &select.into.as_ref().expect("INTO").target
        else {
            panic!("expected variable target")
        };
        assert_eq!(variables.len(), 2);
        assert_eq!(variables[0].to_string(), first);
        assert_eq!(variables[1].to_string(), second);
        let reparsed = Parser::parse_sql(&MySqlDialect {}, &statement.to_string()).unwrap();
        assert_eq!(reparsed[0].to_string(), statement.to_string());
    }
}

#[test]
fn condition_atom_on_original_cursor_preserves_scoped_variables_and_next_target() {
    use crate::tokenizer::Token;
    let dialect = MySqlDialect {};
    for atom in [
        "1",
        "1.1",
        "'text'",
        "NULL",
        "TRUE",
        "@n",
        "@'comma,name'",
        "@@session.warning_count",
        "@@GLOBAL.max_connections",
        "local_number",
        "t.n",
        "db.t.n",
    ] {
        let sql = format!("{atom} @target=MESSAGE_TEXT");
        let mut parser = Parser::new(&dialect).try_with_sql(&sql).unwrap();
        let value = parser.parse_mysql_signal_allowed_expr().unwrap();
        assert!(
            !matches!(
                value,
                Expr::BinaryOp { .. } | Expr::Function(_) | Expr::Nested(_)
            ),
            "{atom}"
        );
        let target = parser.parse_mysql_variable_target(false).unwrap();
        assert_eq!(target.0.len(), 1);
        assert_eq!(target.0[0].as_user_variable().unwrap().value, "target");
        assert_eq!(parser.peek_token_ref().token, Token::Eq);
    }
    for atom in ["-1", "(1)", "f()", "db.f('child')", "?", "(SELECT 1)"] {
        let mut parser = Parser::new(&dialect).try_with_sql(atom).unwrap();
        assert!(parser.parse_mysql_signal_allowed_expr().is_err(), "{atom}");
    }
    let mut parser = Parser::new(&dialect)
        .try_with_sql("db.f('child') @target=MESSAGE_TEXT")
        .unwrap();
    assert!(parser.parse_mysql_signal_allowed_expr().is_err());
    assert_eq!(
        parser.peek_token_ref().token,
        Token::LParen,
        "reject the function shape before consuming its child"
    );
    let mut parser = Parser::new(&dialect)
        .try_with_sql("1+1 @target=MESSAGE_TEXT")
        .unwrap();
    parser.parse_mysql_signal_allowed_expr().unwrap();
    assert_eq!(
        parser.peek_token_ref().token,
        Token::Plus,
        "infix belongs to parent rejection, not this atom"
    );
}

#[test]
fn flush_table_reduction_leaves_the_parent_statement_delimiter_unconsumed() {
    use crate::tokenizer::Token;
    let dialect = MySqlDialect {};
    for command in [
        "FLUSH TABLES",
        "FLUSH TABLES WITH READ LOCK",
        "FLUSH TABLES t1,db.t2",
        "FLUSH TABLES t1 WITH READ LOCK",
        "FLUSH TABLES t1 FOR EXPORT",
    ] {
        let sql = format!("{command}; SELECT 7");
        let mut parser = Parser::new(&dialect).try_with_sql(&sql).unwrap();
        assert!(
            matches!(parser.parse_statement().unwrap(), Statement::Flush { .. }),
            "{command}"
        );
        assert_eq!(parser.peek_token_ref().token, Token::SemiColon, "{command}");
        parser.next_token();
        assert!(matches!(
            parser.parse_statement().unwrap(),
            Statement::Query(_)
        ));
        assert_eq!(parser.peek_token_ref().token, Token::EOF);
    }
    for sql in [
        "FLUSH TABLES FOR EXPORT",
        "FLUSH TABLES WITH WRITE LOCK",
        "FLUSH TABLES t1 FOR OTHER",
    ] {
        assert!(Parser::parse_sql(&dialect, sql).is_err(), "{sql}");
    }
}

#[test]
fn mysql_explain_accepts_query_and_dml_but_not_portable_utility_wrappers() {
    let dialect = MySqlDialect {};
    for sql in [
        "EXPLAIN SELECT 1",
        "EXPLAIN WITH c AS (SELECT 1 AS n) SELECT n FROM c",
        "EXPLAIN INSERT INTO t VALUES(1)",
        "EXPLAIN REPLACE INTO t VALUES(1)",
        "EXPLAIN UPDATE t SET n=1",
        "EXPLAIN DELETE FROM t",
        "DESCRIBE SELECT 1",
        "DESC UPDATE t SET n=1",
    ] {
        let statements = Parser::parse_sql(&dialect, sql).unwrap();
        assert!(
            matches!(statements.as_slice(), [Statement::Explain { .. }]),
            "{sql}"
        );
    }
    for command in [
        "CREATE TABLE t(n INT)",
        "CREATE TEMPORARY TABLE t(n INT)",
        "ALTER TABLE t ADD COLUMN n INT",
        "DROP TABLE t",
        "DROP TEMPORARY TABLE t",
        "SHOW TABLES",
        "SET @n=1",
        "CALL p()",
        "FLUSH TABLES",
        "COMMIT",
    ] {
        for alias in ["EXPLAIN", "DESCRIBE", "DESC"] {
            let sql = format!("{alias} {command}");
            assert!(Parser::parse_sql(&dialect, &sql).is_err(), "{sql}");
        }
    }
    assert!(matches!(
        Parser::parse_sql(&dialect, "DESCRIBE t")
            .unwrap()
            .as_slice(),
        [Statement::ExplainTable { .. }]
    ));
    assert!(
        matches!(
            Parser::parse_sql(
                &crate::dialect::GenericDialect {},
                "EXPLAIN CREATE TABLE t(n INT)"
            )
            .unwrap()
            .as_slice(),
            [Statement::Explain { .. }]
        ),
        "the MySQL grammar gate must not change other dialects"
    );
}

#[test]
fn mysql_maintenance_native_options_have_one_ordered_ast_and_roundtrip() {
    use crate::ast::{FlushLocation as Location, MySqlTableMaintenanceOperation as Operation};
    for (verb, operation) in [
        ("ANALYZE", Operation::Analyze),
        ("OPTIMIZE", Operation::Optimize),
        ("REPAIR", Operation::Repair),
    ] {
        for (prefix, location) in [
            ("", None),
            ("LOCAL ", Some(Location::Local)),
            ("NO_WRITE_TO_BINLOG ", Some(Location::NoWriteToBinlog)),
        ] {
            for table_keyword in ["TABLE", "TABLES"] {
                let sql = format!("{verb} {prefix}{table_keyword} `QUICK`, db.t");
                let parsed = Parser::parse_sql(&MySqlDialect {}, &sql).unwrap();
                let Statement::MySqlTableMaintenance {
                    operation: actual,
                    location: actual_location,
                    tables,
                    options,
                    histogram,
                } = &parsed[0]
                else {
                    panic!("{sql}")
                };
                assert_eq!(*actual, operation, "{sql}");
                assert_eq!(*actual_location, location, "{sql}");
                assert_eq!(tables.len(), 2, "{sql}");
                assert!(options.is_empty());
                assert!(histogram.is_none());
                assert_eq!(
                    Parser::parse_sql(&MySqlDialect {}, &parsed[0].to_string()).unwrap(),
                    parsed,
                    "{sql}"
                );
            }
        }
    }
    for (sql, expected) in [
        (
            "REPAIR LOCAL TABLES t QUICK EXTENDED USE_FRM QUICK",
            vec![
                MySqlCheckTableOption::Quick,
                MySqlCheckTableOption::Extended,
                MySqlCheckTableOption::UseFrm,
                MySqlCheckTableOption::Quick,
            ],
        ),
        (
            "CHECK TABLES t QUICK FAST MEDIUM EXTENDED CHANGED FOR UPGRADE",
            vec![
                MySqlCheckTableOption::Quick,
                MySqlCheckTableOption::Fast,
                MySqlCheckTableOption::Medium,
                MySqlCheckTableOption::Extended,
                MySqlCheckTableOption::Changed,
                MySqlCheckTableOption::ForUpgrade,
            ],
        ),
    ] {
        let parsed = Parser::parse_sql(&MySqlDialect {}, sql).unwrap();
        let Statement::MySqlTableMaintenance { options, .. } = &parsed[0] else {
            panic!("{sql}")
        };
        assert_eq!(options, &expected);
        assert_eq!(
            Parser::parse_sql(&MySqlDialect {}, &parsed[0].to_string()).unwrap(),
            parsed
        );
    }
    for sql in [
        "CHECK LOCAL TABLE t",
        "CHECK NO_WRITE_TO_BINLOG TABLE t",
        "REPAIR TABLE t FAST",
        "REPAIR TABLE t FOR UPGRADE",
        "CHECK TABLE t USE_FRM",
        "ANALYZE TABLE t QUICK",
        "OPTIMIZE TABLE t EXTENDED",
        "REPAIR QUICK TABLE t",
        "CHECK TABLE t FOR",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
}

#[test]
fn array_character_cast_clones_and_serializes_the_original_convert_bit_and_visits_its_operand() {
    let sql = "CREATE INDEX i ON t ((CAST(j AS CHAR(4) CHARACTER SET binary ARRAY)))";
    let statements = Parser::parse_sql(&MySqlDialect {}, sql).unwrap();
    let cloned = statements.clone();
    assert_eq!(statements, cloned);
    assert_eq!(statements[0].to_string(), sql);
    #[cfg(feature = "serde")]
    {
        let encoded = serde_json::to_vec(&statements).unwrap();
        let decoded: Vec<Statement> = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, statements);
    }
    #[cfg(feature = "visitor")]
    {
        let mut cast = 0;
        let mut operand = 0;
        let _: std::ops::ControlFlow<()> =
            crate::ast::visit_expressions(&statements, |expression| {
                if matches!(expression, Expr::Convert { array: true, .. }) {
                    cast += 1;
                }
                if matches!(expression, Expr::Identifier(name) if name.value == "j") {
                    operand += 1;
                }
                std::ops::ControlFlow::Continue(())
            });
        assert_eq!((cast, operand), (1, 1));
    }
    assert!(
        Parser::parse_sql(
            &MySqlDialect {},
            "SELECT CAST(j AS CHAR(4) CHARACTER SET binary ARRAY FORMAT 'x')"
        )
        .is_err()
    );
}

#[test]
fn temporal_array_cast_preserves_fsp_and_rejects_timezone_only_in_the_mysql_array_context() {
    for target in [
        "DATE",
        "TIME",
        "TIME(0)",
        "TIME(6)",
        "DATETIME",
        "DATETIME(0)",
        "DATETIME(6)",
    ] {
        let sql = format!("CREATE INDEX i ON t ((CAST(j AS {target} ARRAY)))");
        let parsed = Parser::parse_sql(&MySqlDialect {}, &sql).unwrap();
        assert_eq!(parsed[0].to_string(), sql);
        assert_eq!(
            Parser::parse_sql(&MySqlDialect {}, &parsed[0].to_string()).unwrap(),
            parsed
        );
        #[cfg(feature = "serde")]
        assert_eq!(
            serde_json::from_slice::<Vec<Statement>>(&serde_json::to_vec(&parsed).unwrap())
                .unwrap(),
            parsed
        );
    }
    for target in [
        "TIME WITH TIME ZONE",
        "TIME(6) WITH TIME ZONE",
        "TIME WITHOUT TIME ZONE",
        "TIME(6) WITHOUT TIME ZONE",
        "TIMETZ",
        "TIMETZ(6)",
    ] {
        let sql = format!("CREATE INDEX bad ON t ((CAST((@once:=@once+1) AS {target} ARRAY)))");
        let error = Parser::parse_sql(&MySqlDialect {}, &sql).unwrap_err();
        assert!(
            matches!(&error, crate::parser::ParserError::ParserError(_)),
            "original syntax error, not a semantic cast/Field error: {error}"
        );
        assert!(
            error.to_string().contains("time zone qualifier"),
            "{sql}: {error}"
        );
        let ordinary = format!("SELECT CAST(j AS {target})");
        assert!(
            Parser::parse_sql(&MySqlDialect {}, &ordinary).is_ok(),
            "old ordinary syntax is untouched: {ordinary}"
        );
        let generic = format!("SELECT CAST(j AS {target} ARRAY)");
        assert!(
            Parser::parse_sql(&crate::dialect::GenericDialect {}, &generic).is_ok(),
            "other dialect is untouched: {generic}"
        );
    }
}

#[test]
fn array_time_zone_capability_is_delegated_by_every_mysql_option_combination() {
    use crate::dialect::{Dialect, GenericDialect};
    assert!(GenericDialect {}.supports_time_zone_in_array_cast());
    assert!(!MySqlDialect {}.supports_time_zone_in_array_cast());
    for mask in 0..8 {
        let configured = MySqlDialect::with_options(mask & 1 != 0, mask & 2 != 0, mask & 4 != 0);
        assert!(!configured.supports_time_zone_in_array_cast());
        for target in [
            "TIME WITH TIME ZONE",
            "TIME(6) WITHOUT TIME ZONE",
            "TIMETZ(6)",
        ] {
            let sql = format!("SELECT CAST(j AS {target} ARRAY)");
            let native = Parser::parse_sql(&MySqlDialect {}, &sql);
            assert!(native.is_err());
            assert_eq!(
                Parser::parse_sql(&configured, &sql),
                native,
                "options={mask}: {sql}"
            );
            assert!(Parser::parse_sql(&configured, &format!("SELECT CAST(j AS {target})")).is_ok());
            assert!(Parser::parse_sql(&GenericDialect {}, &sql).is_ok());
        }
        for target in ["DATE", "TIME", "TIME(6)", "DATETIME(6)"] {
            let sql = format!("SELECT CAST(j AS {target} ARRAY)");
            assert_eq!(
                Parser::parse_sql(&configured, &sql).unwrap(),
                Parser::parse_sql(&MySqlDialect {}, &sql).unwrap()
            );
        }
    }
}
