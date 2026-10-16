use crate::{
    ast::{Spanned, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};

#[test]
fn copy_where_round_trip_retains_target_options_and_expression_span() {
    for sql in [
        "COPY public.items (a,b) FROM STDIN WITH (FORMAT csv, DELIMITER ';') WHERE a > 2 AND b IS NOT NULL",
        "COPY public.items FROM '/tmp/items' CSV HEADER WHERE a = 5",
        "COPY public.items FROM PROGRAM 'cat /tmp/items' WHERE tableoid = 'public.items'::regclass",
    ] {
        let statements = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
        let [
            Statement::Copy {
                where_clause: Some(predicate),
                to: false,
                ..
            },
        ] = statements.as_slice()
        else {
            panic!("COPY WHERE: {statements:?}");
        };
        assert_ne!(predicate.span(), crate::tokenizer::Span::empty());
        assert!(statements[0].span().end >= predicate.span().end);
        let formatted = statements[0].to_string();
        let reparsed = Parser::parse_sql(&PostgreSqlDialect {}, &formatted).unwrap();
        assert_eq!(formatted, reparsed[0].to_string());
    }
}

#[test]
fn copy_inline_data_starts_after_the_where_expression() {
    let statements = Parser::parse_sql(
        &PostgreSqlDialect {},
        "COPY items FROM STDIN WHERE a > 1;\n1\n2\n\\.\n",
    )
    .unwrap();
    let [Statement::Copy {
        where_clause: Some(_),
        values,
        ..
    }] = statements.as_slice()
    else {
        panic!("COPY WHERE: {statements:?}");
    };
    // The existing inline TSV representation retains its leading newline.
    assert_eq!(
        values,
        &[
            Some(String::new()),
            Some("1".to_owned()),
            Some("2".to_owned())
        ]
    );
    assert!(statements[0].to_string().contains(" WHERE a > 1;"));
}

#[test]
fn copy_to_rejects_where() {
    let error =
        Parser::parse_sql(&PostgreSqlDialect {}, "COPY items TO STDOUT WHERE a > 1").unwrap_err();
    assert!(error
        .to_string()
        .contains("WHERE clause not allowed with COPY TO"));
}
