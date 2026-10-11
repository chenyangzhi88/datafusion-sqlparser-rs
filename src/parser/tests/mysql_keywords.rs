use std::collections::HashSet;

use crate::dialect::{Dialect, MySqlDialect};
use crate::keywords::Keyword;
use crate::mysql_keywords::{MYSQL_8_0_46_KEYWORDS, mysql_8_0_46_keyword};

#[test]
fn mysql_window_parent_identifiers_use_the_real_nonreserved_domain() {
    use crate::ast::{NamedWindowExpr, SetExpr, Statement};
    use crate::parser::Parser;
    for dialect in [
        Box::new(MySqlDialect {}) as Box<dyn Dialect>,
        Box::new(MySqlDialect::with_options(true, true, true)) as Box<dyn Dialect>,
    ] {
        for name in ["absent", "ABSENT", "`ROWS`", "`ORDER`", "`select`"] {
            let sql = format!("SELECT 1 WINDOW w AS ({name}),w2 AS (ORDER BY missing)");
            let parsed = Parser::parse_sql(dialect.as_ref(), &sql).unwrap();
            let Statement::Query(query) = &parsed[0] else {
                panic!("query")
            };
            let SetExpr::Select(select) = query.body.as_ref() else {
                panic!("select")
            };
            let NamedWindowExpr::WindowSpec(spec) = &select.named_window[0].1 else {
                panic!("spec")
            };
            assert_eq!(spec.window_name.as_ref().unwrap().to_string(), name);
            assert!(spec.partition_by.is_empty());
            assert!(spec.order_by.is_empty());
            assert!(spec.window_frame.is_none());
        }
        for sql in [
            "SELECT 1 WINDOW w AS (ROWS CURRENT ROW)",
            "SELECT 1 WINDOW w AS (RANGE CURRENT ROW)",
            "SELECT 1 WINDOW w AS (GROUPS CURRENT ROW)",
            "SELECT 1 WINDOW w AS (PARTITION BY n ORDER BY n)",
        ] {
            let parsed = Parser::parse_sql(dialect.as_ref(), sql).unwrap();
            let Statement::Query(query) = &parsed[0] else {
                panic!("query")
            };
            let SetExpr::Select(select) = query.body.as_ref() else {
                panic!("select")
            };
            let NamedWindowExpr::WindowSpec(spec) = &select.named_window[0].1 else {
                panic!("spec")
            };
            assert!(spec.window_name.is_none(), "{sql}");
        }
        assert!(Parser::parse_sql(dialect.as_ref(), "SELECT 1 WINDOW w AS (SELECT)").is_err());
    }
}

#[test]
fn window_clause_cannot_be_consumed_as_a_mysql_select_alias() {
    use crate::ast::{SelectItem, SetExpr, Statement};
    use crate::parser::Parser;
    for dialect in [
        Box::new(MySqlDialect {}) as Box<dyn Dialect>,
        Box::new(MySqlDialect::with_options(true, true, true)) as Box<dyn Dialect>,
    ] {
        for sql in [
            "SELECT 1 WINDOW w AS (ORDER BY 1+0)",
            "SELECT 1 AS marker WINDOW w AS (ROWS CURRENT ROW)",
            "SELECT 1 FROM t WINDOW w AS (PARTITION BY k)",
        ] {
            let statements = Parser::parse_sql(dialect.as_ref(), sql).unwrap();
            let Statement::Query(query) = &statements[0] else {
                panic!("query")
            };
            let SetExpr::Select(select) = query.body.as_ref() else {
                panic!("select")
            };
            assert_eq!(select.named_window.len(), 1, "{sql}");
        }
        for sql in ["SELECT 1 AS `WINDOW`", "SELECT 1 `WINDOW`"] {
            let statements = Parser::parse_sql(dialect.as_ref(), sql).unwrap();
            let Statement::Query(query) = &statements[0] else {
                panic!("query")
            };
            let SetExpr::Select(select) = query.body.as_ref() else {
                panic!("select")
            };
            assert!(
                matches!(&select.projection[0], SelectItem::ExprWithAlias { alias, .. } if alias.value == "WINDOW")
            );
            assert!(select.named_window.is_empty());
        }
        assert!(Parser::parse_sql(dialect.as_ref(), "SELECT 1 AS WINDOW").is_err());
    }
}

#[test]
fn window_origin_and_exclusion_roundtrip_as_structured_source_clauses() {
    use crate::ast::{
        Expr, FromFirstLast, SelectItem, SetExpr, Statement, WindowFrameExclusion, WindowType,
    };
    use crate::parser::Parser;
    for (suffix, expected) in [
        ("FIRST", FromFirstLast::First),
        ("LAST", FromFirstLast::Last),
    ] {
        let sql = format!(
            "SELECT NTH_VALUE(n, 2) FROM {suffix} RESPECT NULLS OVER (ORDER BY id ROWS 2 PRECEDING) FROM t"
        );
        let statements = Parser::parse_sql(&MySqlDialect {}, &sql).unwrap();
        let Statement::Query(query) = &statements[0] else {
            panic!("query")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("select")
        };
        let SelectItem::UnnamedExpr(Expr::Function(function)) = &select.projection[0] else {
            panic!("function")
        };
        assert_eq!(function.from_first_last, Some(expected));
        assert_eq!(select.from.len(), 1);
        assert_eq!(
            Parser::parse_sql(&MySqlDialect {}, &statements[0].to_string()).unwrap(),
            statements
        );
    }
    for (suffix, expected) in [
        ("CURRENT ROW", WindowFrameExclusion::CurrentRow),
        ("GROUP", WindowFrameExclusion::Group),
        ("TIES", WindowFrameExclusion::Ties),
        ("NO OTHERS", WindowFrameExclusion::NoOthers),
    ] {
        let sql = format!(
            "SELECT SUM(n) OVER (ORDER BY id ROWS BETWEEN 2 PRECEDING AND CURRENT ROW EXCLUDE {suffix}) FROM t"
        );
        let statements = Parser::parse_sql(&MySqlDialect {}, &sql).unwrap();
        let Statement::Query(query) = &statements[0] else {
            panic!("query")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("select")
        };
        let SelectItem::UnnamedExpr(Expr::Function(function)) = &select.projection[0] else {
            panic!("function")
        };
        let Some(WindowType::WindowSpec(window)) = &function.over else {
            panic!("window")
        };
        assert_eq!(
            window.window_frame.as_ref().unwrap().exclusion,
            Some(expected)
        );
        assert_eq!(
            Parser::parse_sql(&MySqlDialect {}, &statements[0].to_string()).unwrap(),
            statements
        );
    }
    // FROM is still a table clause, and quoted OTHERS is not a frame keyword.
    for sql in [
        "SELECT LOWER(s) FROM last",
        "SELECT NTH_VALUE(n,2) OVER () FROM first",
    ] {
        let statements = Parser::parse_sql(&MySqlDialect {}, sql).unwrap();
        let Statement::Query(query) = &statements[0] else {
            panic!("query")
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("select")
        };
        assert_eq!(select.from.len(), 1);
    }
    for sql in [
        "SELECT SUM(n) OVER (ROWS 1 PRECEDING EXCLUDE NO `OTHERS`) FROM t",
        "SELECT SUM(n) OVER (ROWS 1 PRECEDING EXCLUDE NONE) FROM t",
    ] {
        assert!(Parser::parse_sql(&MySqlDialect {}, sql).is_err(), "{sql}");
    }
}

#[test]
fn create_table_plain_comments_have_one_canonical_last_wins_value_and_roundtrip() {
    use crate::ast::{CommentDef, CreateTableElement, CreateTableOptions, SqlOption, Statement};
    use crate::dialect::GenericDialect;
    use crate::parser::Parser;
    let mysql = MySqlDialect {};
    let generic = GenericDialect {};
    for dialect in [&mysql as &dyn Dialect, &generic as &dyn Dialect] {
        let mut parsed = Parser::parse_sql(
            dialect,
            "CREATE TABLE comment_owner(a INT) COMMENT='old' ENGINE=InnoDB COMMENT 'last''quote'",
        )
        .unwrap();
        let Statement::CreateTable(table) = parsed.remove(0) else {
            panic!("table")
        };
        assert_eq!(table.elements, [CreateTableElement::Column(0)]);
        assert_eq!(
            table.comment,
            Some(CommentDef::WithoutEq("last'quote".into()))
        );
        let CreateTableOptions::Plain(options) = &table.table_options else {
            panic!("options")
        };
        assert_eq!(options.len(), 1);
        assert!(
            options
                .iter()
                .all(|option| !matches!(option, SqlOption::Comment(_)))
        );
        assert!(table.validate_source_order().is_ok());
        let rendered = table.to_string();
        assert_eq!(rendered.matches("COMMENT").count(), 1);
        assert!(rendered.contains("COMMENT 'last''quote'"));
        assert_eq!(
            Parser::parse_sql(dialect, &rendered).unwrap(),
            [Statement::CreateTable(table)]
        );
    }
}

#[test]
fn create_table_builder_creates_ordinals_without_cloning_definitions_or_comments() {
    use crate::ast::helpers::stmt_create_table::CreateTableBuilder;
    use crate::ast::{CommentDef, CreateTableElement, CreateTableOptions, SqlOption, Statement};
    use crate::parser::Parser;
    let mut parsed = Parser::parse_sql(
        &MySqlDialect {},
        "CREATE TABLE builder_owner(a INT DEFAULT 7, CHECK(a>0))",
    )
    .unwrap();
    let Statement::CreateTable(table) = parsed.remove(0) else {
        panic!("table")
    };
    let built = CreateTableBuilder::new(table.name)
        .columns(table.columns)
        .constraints(table.constraints)
        .table_options(CreateTableOptions::Plain(vec![
            SqlOption::Comment(CommentDef::WithEq("first".into())),
            SqlOption::Comment(CommentDef::WithEq("last".into())),
        ]))
        .build();
    assert_eq!(
        built.elements,
        [
            CreateTableElement::Column(0),
            CreateTableElement::Constraint(0)
        ]
    );
    assert_eq!(built.comment, Some(CommentDef::WithEq("last".into())));
    assert!(
        matches!(&built.table_options, CreateTableOptions::Plain(options) if options.is_empty())
    );
    assert!(built.validate_source_order().is_ok());
    assert!(built.to_string().contains("COMMENT = 'last'"));
}

#[test]
fn quoted_user_variables_share_decoded_set_expression_and_into_identity() {
    use crate::ast::{Expr, SelectIntoTarget, Set, SetExpr, Statement};
    use crate::parser::Parser;
    for ansi_quotes in [false, true] {
        for no_backslash_escapes in [false, true] {
            let dialect = MySqlDialect::with_options(ansi_quotes, false, no_backslash_escapes);
            for (source, decoded) in [
                ("@`wanted.value`", "wanted.value"),
                ("@`a``b`", "a`b"),
                ("@'space name'", "space name"),
                ("@'a''b'", "a'b"),
                ("@\"double name\"", "double name"),
                ("@'@x'", "@x"),
                ("@''", ""),
                ("@' leading'", " leading"),
                ("@'trailing '", "trailing "),
                ("@'a,b'", "a,b"),
                ("@'x\0y'", "x\0y"),
            ] {
                let statements = Parser::parse_sql(
                    &dialect,
                    &format!("SET {source}=9; SELECT {source} INTO {source}"),
                )
                .unwrap();
                let Statement::Set(Set::SingleAssignment { variable, .. }) = &statements[0] else {
                    panic!("expected SET assignment: {:?}", statements[0]);
                };
                let [part] = variable.0.as_slice() else {
                    panic!("variable is not one component")
                };
                assert!(part.as_ident().is_none());
                let name = part.as_user_variable().unwrap();
                assert_eq!(name.value, decoded);
                assert!(name.quote_style.is_some());
                let Statement::Query(query) = &statements[1] else {
                    panic!("expected SELECT")
                };
                let SetExpr::Select(select) = query.body.as_ref() else {
                    panic!("expected SELECT")
                };
                let crate::ast::SelectItem::UnnamedExpr(expression) = &select.projection[0] else {
                    panic!("expected variable expression")
                };
                assert_eq!(expression.as_user_variable().unwrap().value, decoded);
                assert_eq!(
                    expression.as_user_variable().unwrap().quote_style,
                    name.quote_style
                );
                let SelectIntoTarget::Variables(variables) = &select.into.as_ref().unwrap().target
                else {
                    panic!("expected variable INTO")
                };
                assert_eq!(variables.len(), 1);
                assert_eq!(variables[0].0.len(), 1);
                assert!(variables[0].0[0].as_ident().is_none());
                assert_eq!(variables[0].0[0].as_user_variable().unwrap().value, decoded);
                let assignment =
                    Parser::parse_sql(&dialect, &format!("SELECT {source}:=7")).unwrap();
                let Statement::Query(query) = &assignment[0] else {
                    panic!("expected SELECT assignment")
                };
                let SetExpr::Select(select) = query.body.as_ref() else {
                    panic!("expected SELECT assignment")
                };
                let crate::ast::SelectItem::UnnamedExpr(Expr::BinaryOp { left, op, .. }) =
                    &select.projection[0]
                else {
                    panic!("expected assignment AST")
                };
                assert_eq!(*op, crate::ast::BinaryOperator::Assignment);
                assert_eq!(left.as_user_variable().unwrap().value, decoded);
            }
            let statements = Parser::parse_sql(&dialect, "SET @'a\\b'=1; SELECT @'a\\b'").unwrap();
            let Statement::Set(Set::SingleAssignment { variable, .. }) = &statements[0] else {
                panic!("expected SET")
            };
            assert_eq!(
                variable.0[0].as_user_variable().unwrap().value,
                if no_backslash_escapes {
                    "a\\b"
                } else {
                    "a\u{0008}"
                }
            );
        }
    }
    let statements = Parser::parse_sql(
        &MySqlDialect {},
        "SET @wanted.value=1, @`wanted.value`=2, @@session.autocommit=1",
    )
    .unwrap();
    let Statement::Set(Set::MultipleAssignments { assignments }) = &statements[0] else {
        panic!("expected ordered SET")
    };
    assert_eq!(assignments.len(), 3);
    assert_eq!(assignments[0].name.0.len(), 1);
    assert_eq!(
        assignments[0].name.0[0].as_user_variable().unwrap().value,
        "wanted.value"
    );
    assert_eq!(assignments[1].name.0.len(), 1);
    assert_eq!(
        assignments[1].name.0[0].as_user_variable().unwrap().value,
        "wanted.value"
    );
    assert_eq!(
        assignments[2].name.0[0].as_ident().unwrap().value,
        "@@session"
    );
    for source in [
        "SET @=1",
        "SET @ `name`=1",
        "SELECT @",
        "SELECT @`unterminated",
        "SET @'name'=1 trailing",
    ] {
        assert!(
            Parser::parse_sql(&MySqlDialect {}, source).is_err(),
            "{source}"
        );
    }
    for dialect in [
        &crate::dialect::PostgreSqlDialect {} as &dyn Dialect,
        &crate::dialect::GenericDialect {},
    ] {
        assert!(Parser::parse_sql(dialect, "SET @`wanted.value`=9").is_err());
    }
}

#[test]
fn mysql_user_variables_have_borrowed_identity_span_and_safe_display() {
    use crate::ast::{Expr, Ident, ObjectNamePart, Spanned};
    use crate::parser::Parser;
    use crate::tokenizer::Location;
    for (source, decoded, quote) in [
        ("@wanted.value", "wanted.value", None),
        ("@É", "É", None),
        ("@'@x'", "@x", Some('\'')),
        ("@''", "", Some('\'')),
        ("@`a``b`", "a`b", Some('`')),
    ] {
        let mut parser = Parser::new(&MySqlDialect {}).try_with_sql(source).unwrap();
        let expression = parser.parse_expr().unwrap();
        assert_eq!(expression.span().start, Location::new(1, 1));
        assert_eq!(
            expression.span().end,
            Location::new(1, source.chars().count() as u64 + 1)
        );
        let name = expression.as_user_variable().unwrap();
        assert_eq!(name.value, decoded);
        assert_eq!(name.quote_style, quote);
        let displayed = expression.to_string();
        assert_eq!(displayed, source);
        let mut parser = Parser::new(&MySqlDialect {})
            .try_with_sql(&displayed)
            .unwrap();
        assert_eq!(
            parser
                .parse_expr()
                .unwrap()
                .as_user_variable()
                .unwrap()
                .value,
            decoded
        );
    }
    for raw in [
        "", "@x", "a`b", "a,b", "x\0y", ".a", "a.", "a..b", "a b", "a\nb",
    ] {
        let expression = Expr::UserVariable(Ident::new(raw));
        let target = ObjectNamePart::UserVariable(Ident::new(raw));
        assert_eq!(expression.to_string(), target.to_string());
        assert!(target.as_ident().is_none());
        let displayed = expression.to_string();
        let mut parser = Parser::new(&MySqlDialect {})
            .try_with_sql(&displayed)
            .unwrap();
        assert_eq!(
            parser
                .parse_expr()
                .unwrap()
                .as_user_variable()
                .unwrap()
                .value,
            raw
        );
    }
    for raw in ["a", "first", "wanted.value", "É", "_a", "$a", "123", "a.12"] {
        let expression = Expr::UserVariable(Ident::new(raw));
        let target = ObjectNamePart::UserVariable(Ident::new(raw));
        let displayed = expression.to_string();
        assert_eq!(displayed, format!("@{raw}"));
        assert_eq!(displayed, target.to_string());
        let mut parser = Parser::new(&MySqlDialect {})
            .try_with_sql(&displayed)
            .unwrap();
        assert_eq!(
            parser
                .parse_expr()
                .unwrap()
                .as_user_variable()
                .unwrap()
                .value,
            raw
        );
    }
}

#[test]
fn mysql_user_variables_do_not_widen_object_or_other_dialect_grammars() {
    use crate::ast::{Expr, SelectItem, SetExpr, Statement, UnaryOperator};
    use crate::parser::Parser;
    for source in [
        "SELECT * FROM @x",
        "SELECT * FROM foo.@x",
        "CREATE TABLE @x (a INT)",
        "CREATE TABLE t (@x INT)",
        "SELECT 1 INTO @@autocommit",
    ] {
        assert!(
            Parser::parse_sql(&MySqlDialect {}, source).is_err(),
            "{source}"
        );
    }
    let statements =
        Parser::parse_sql(&crate::dialect::PostgreSqlDialect {}, "SELECT @'1'").unwrap();
    let Statement::Query(query) = &statements[0] else {
        panic!("query")
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("select")
    };
    assert!(matches!(
        &select.projection[0],
        SelectItem::UnnamedExpr(Expr::UnaryOp {
            op: UnaryOperator::PGAbs,
            ..
        })
    ));
    let mut parser = Parser::new(&crate::dialect::GenericDialect {})
        .try_with_sql("@x")
        .unwrap();
    assert!(parser.parse_expr().unwrap().as_user_variable().is_none());
    let mut parser = Parser::new(&MySqlDialect {}).try_with_sql("`@x`").unwrap();
    assert!(matches!(parser.parse_expr().unwrap(), Expr::Identifier(_)));
    let mut parser = Parser::new(&MySqlDialect {})
        .try_with_sql("@@autocommit")
        .unwrap();
    assert!(parser.parse_expr().unwrap().as_user_variable().is_none());
}

#[cfg(feature = "visitor")]
#[test]
fn visitors_distinguish_user_variables_from_columns_and_parameter_values() {
    use crate::ast::{Expr, Visit, VisitMut, Visitor, VisitorMut};
    use crate::parser::Parser;
    use core::ops::ControlFlow;
    struct Names(Vec<String>);
    impl Visitor for Names {
        type Break = ();
        fn pre_visit_expr(&mut self, expression: &Expr) -> ControlFlow<()> {
            if let Some(name) = expression.as_user_variable() {
                self.0.push(name.value.clone());
            }
            ControlFlow::Continue(())
        }
    }
    struct Rename;
    impl VisitorMut for Rename {
        type Break = ();
        fn pre_visit_expr(&mut self, expression: &mut Expr) -> ControlFlow<()> {
            if let Expr::UserVariable(name) = expression {
                name.value = "@renamed".into();
            }
            ControlFlow::Continue(())
        }
    }
    let mut statements = Parser::parse_sql(&MySqlDialect {}, "SELECT @x, @'@x', ?, `@x`").unwrap();
    let mut names = Names(Vec::new());
    let _ = statements.visit(&mut names);
    assert_eq!(names.0, ["x", "@x"]);
    let _ = VisitMut::visit(&mut statements, &mut Rename);
    let mut names = Names(Vec::new());
    let _ = statements.visit(&mut names);
    assert_eq!(names.0, ["@renamed", "@renamed"]);
    assert!(statements[0].to_string().contains('?'));
}

#[test]
fn character_attributes_remain_real_column_options_not_fixed_collations() {
    use crate::ast::{ColumnOption, Statement};
    use crate::parser::Parser;
    for dialect in [
        &MySqlDialect {} as &dyn Dialect,
        &crate::dialect::GenericDialect {},
    ] {
        let statements=Parser::parse_sql(dialect,"CREATE TABLE t(a CHAR(2) BINARY,b CHAR(2) ASCII BINARY,c CHAR(2) UNICODE,d CHAR(2) CHARSET latin1) CHARSET=latin1").unwrap();
        let Statement::CreateTable(table) = &statements[0] else {
            panic!("expected CREATE TABLE");
        };
        assert!(
            matches!(&table.columns[0].options[0].option,ColumnOption::DialectSpecific(tokens) if tokens[0].to_string()=="BINARY")
        );
        assert_eq!(table.columns[1].options.len(), 2);
        assert!(
            matches!(&table.columns[2].options[0].option,ColumnOption::DialectSpecific(tokens) if tokens[0].to_string()=="UNICODE")
        );
        assert!(
            matches!(&table.columns[3].options[0].option,ColumnOption::CharacterSet(name) if name.to_string()=="latin1")
        );
        assert!(!table.columns.iter().any(|column| {
            column
                .options
                .iter()
                .any(|option| matches!(option.option, ColumnOption::Collation(_)))
        }));
    }
    assert!(
        Parser::parse_sql(
            &crate::dialect::PostgreSqlDialect {},
            "CREATE TABLE t(a CHAR(2) ASCII)"
        )
        .is_err()
    );
}

#[test]
fn character_attribute_words_preserve_case_order_and_existing_type_syntax() {
    use crate::ast::{ColumnOption, DataType, Statement};
    use crate::parser::Parser;
    for dialect in [
        &MySqlDialect {} as &dyn Dialect,
        &crate::dialect::GenericDialect {},
    ] {
        let statements = Parser::parse_sql(dialect, "CREATE TABLE t(a CHAR(2) aScIi BiNaRy,b CHAR(2) bInArY aScIi,c CHAR(2) uNiCoDe BiNaRy,d CHAR(2) bInArY uNiCoDe,e BINARY(2),f VARBINARY(4),g ENUM('ASCII','UNICODE','BINARY'),h SET('ASCII','UNICODE','BINARY'))").unwrap();
        let Statement::CreateTable(table) = &statements[0] else {
            panic!("expected CREATE TABLE");
        };
        for (index, expected) in [
            ["aScIi", "BiNaRy"],
            ["bInArY", "aScIi"],
            ["uNiCoDe", "BiNaRy"],
            ["bInArY", "uNiCoDe"],
        ]
        .iter()
        .enumerate()
        {
            let actual = table.columns[index]
                .options
                .iter()
                .map(|option| {
                    let ColumnOption::DialectSpecific(tokens) = &option.option else {
                        panic!("expected original character attribute token");
                    };
                    assert_eq!(tokens.len(), 1);
                    tokens[0].to_string()
                })
                .collect::<Vec<_>>();
            assert_eq!(
                actual,
                expected
                    .iter()
                    .map(|word| (*word).to_owned())
                    .collect::<Vec<_>>()
            );
        }
        assert_eq!(table.columns[4].data_type, DataType::Binary(Some(2)));
        assert!(
            table.columns[4..]
                .iter()
                .all(|column| column.options.is_empty())
        );
        assert!(matches!(table.columns[6].data_type, DataType::Enum(..)));
        assert!(matches!(table.columns[7].data_type, DataType::Set(..)));
    }
}

#[test]
fn quoted_character_attribute_names_are_not_grammar_words() {
    use crate::parser::Parser;
    for dialect in [
        &MySqlDialect {} as &dyn Dialect,
        &crate::dialect::GenericDialect {},
    ] {
        for word in ["ASCII", "UNICODE", "BINARY"] {
            for quote in ['`', '"'] {
                let sql = format!("CREATE TABLE t(a CHAR(2) {quote}{word}{quote})");
                assert!(Parser::parse_sql(dialect, &sql).is_err(), "{sql}");
            }
        }
        let statements = Parser::parse_sql(
            dialect,
            "CREATE TABLE `ASCII`(`UNICODE` INT, `BINARY` BINARY(2))",
        )
        .unwrap();
        let crate::ast::Statement::CreateTable(table) = &statements[0] else {
            panic!("expected CREATE TABLE");
        };
        assert_eq!(table.name.to_string(), "`ASCII`");
        assert_eq!(table.columns[0].name.value, "UNICODE");
        assert_eq!(table.columns[1].name.value, "BINARY");
        assert!(table.columns.iter().all(|column| column.options.is_empty()));
    }
}

#[test]
fn character_attribute_words_do_not_change_identifiers_functions_or_other_dialects() {
    use crate::ast::{Expr, SelectItem, SetExpr, Statement};
    use crate::parser::Parser;
    for dialect in [
        &MySqlDialect {} as &dyn Dialect,
        &crate::dialect::GenericDialect {},
        &crate::dialect::PostgreSqlDialect {},
    ] {
        Parser::parse_sql(dialect, "CREATE TABLE ascii(unicode INT, ascii INT)").unwrap();
        let statements = Parser::parse_sql(
            dialect,
            "SELECT unicode AS ascii, ASCII(unicode) AS unicode FROM ascii unicode",
        )
        .unwrap();
        let Statement::Query(query) = &statements[0] else {
            panic!("expected SELECT");
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("expected SELECT body");
        };
        assert!(
            matches!(&select.projection[0], SelectItem::ExprWithAlias { expr: Expr::Identifier(name), alias } if name.value == "unicode" && alias.value == "ascii")
        );
        assert!(
            matches!(&select.projection[1], SelectItem::ExprWithAlias { expr: Expr::Function(function), alias } if function.name.to_string() == "ASCII" && alias.value == "unicode")
        );
        assert!(
            matches!(&select.from[0].relation, crate::ast::TableFactor::Table { alias: Some(alias), .. } if alias.name.value == "unicode")
        );
    }
    for word in ["BINARY", "ASCII", "UNICODE"] {
        let sql = format!("CREATE TABLE t(a CHAR(2) {word})");
        assert!(
            Parser::parse_sql(&crate::dialect::PostgreSqlDialect {}, &sql).is_err(),
            "{sql}"
        );
    }
    let statements = Parser::parse_sql(&MySqlDialect {}, "SELECT CAST('a' AS BINARY(2))").unwrap();
    let Statement::Query(query) = &statements[0] else {
        panic!("expected SELECT");
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        panic!("expected SELECT body");
    };
    assert!(matches!(
        &select.projection[0],
        SelectItem::UnnamedExpr(Expr::Cast {
            data_type: crate::ast::DataType::Binary(Some(2)),
            ..
        })
    ));
}

#[test]
fn character_attribute_tokens_keep_the_shared_keyword_boundary() {
    use crate::tokenizer::{Token, Tokenizer};
    let words = Tokenizer::new(
        &MySqlDialect {},
        "ASCII UNICODE BINARY `ASCII` `UNICODE` `BINARY`",
    )
    .tokenize()
    .unwrap()
    .into_iter()
    .filter_map(|token| match token {
        Token::Word(word) => Some(word),
        _ => None,
    })
    .collect::<Vec<_>>();
    assert_eq!(words.len(), 6);
    assert_eq!(words[0].keyword, Keyword::NoKeyword);
    assert_eq!(words[1].keyword, Keyword::NoKeyword);
    assert_eq!(words[2].keyword, Keyword::BINARY);
    for word in &words[3..] {
        assert_eq!(word.keyword, Keyword::NoKeyword);
        assert_eq!(word.quote_style, Some('`'));
    }
    assert_eq!(mysql_8_0_46_keyword("ASCII").unwrap().reserved, false);
    assert_eq!(mysql_8_0_46_keyword("UNICODE").unwrap().reserved, false);
    assert_eq!(mysql_8_0_46_keyword("BINARY").unwrap().reserved, true);
}

#[test]
fn mysql_8_0_46_keyword_registry_is_complete_sorted_and_unique() {
    assert_eq!(MYSQL_8_0_46_KEYWORDS.len(), 751);
    assert!(
        MYSQL_8_0_46_KEYWORDS
            .windows(2)
            .all(|pair| pair[0].word < pair[1].word)
    );
    let unique = MYSQL_8_0_46_KEYWORDS
        .iter()
        .map(|keyword| keyword.word)
        .collect::<HashSet<_>>();
    assert_eq!(unique.len(), MYSQL_8_0_46_KEYWORDS.len());
    assert_eq!(
        MYSQL_8_0_46_KEYWORDS
            .iter()
            .filter(|keyword| keyword.reserved)
            .count(),
        262
    );
}

#[test]
fn mysql_8_0_46_keyword_registry_preserves_reservation_status() {
    for word in ["ACCESSIBLE", "ADD", "SELECT", "WINDOW", "ZEROFILL"] {
        assert_eq!(
            mysql_8_0_46_keyword(word).map(|entry| entry.reserved),
            Some(true)
        );
    }
    for word in ["ACCOUNT", "ACTIVE", "CHARSET", "ENGINE", "VISIBLE"] {
        assert_eq!(
            mysql_8_0_46_keyword(word).map(|entry| entry.reserved),
            Some(false)
        );
    }
    assert_eq!(mysql_8_0_46_keyword("NOT_A_MYSQL_KEYWORD"), None);

    let dialect = MySqlDialect {};
    assert!(dialect.is_reserved_for_identifier(Keyword::ADD));
    assert!(!dialect.is_reserved_for_identifier(Keyword::ACCOUNT));
}
