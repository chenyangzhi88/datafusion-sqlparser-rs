use crate::{
    ast::{
        Alignment, Statement, UserDefinedTypeRepresentation,
        UserDefinedTypeSqlDefinitionOption as Option,
    },
    dialect::PostgreSqlDialect,
    parser::Parser,
};

#[test]
fn type_definition_labels_preserve_case_quote_and_unknown_arguments() {
    let sql = r#"CREATE TYPE bogus_type ("Internallength"=4, "Input"=int42_in, "Output"=int42_out, "Alignment"=int4, "Default"=42, "Passedbyvalue", EXTRA='ignored', "input"=real_input)"#;
    let statements = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
    let [Statement::CreateType {
        representation: Some(UserDefinedTypeRepresentation::SqlDefinition { options }),
        ..
    }] = statements.as_slice()
    else {
        panic!("base type")
    };
    assert_eq!(options.len(), 8);
    for (option, expected) in options[..6].iter().zip([
        "Internallength",
        "Input",
        "Output",
        "Alignment",
        "Default",
        "Passedbyvalue",
    ]) {
        let Option::Unrecognized { name, value } = option else {
            panic!("unknown label")
        };
        assert_eq!(name.value, expected);
        assert_eq!(name.quote_style, Some('"'));
        assert_eq!(value.is_some(), expected != "Passedbyvalue");
        assert!(name.span.start.column > 0);
    }
    assert!(
        matches!(&options[6], Option::Unrecognized { name, .. } if name.value == "EXTRA" && name.quote_style.is_none())
    );
    assert!(matches!(&options[7], Option::Input(_)));
    assert_eq!(
        Parser::parse_sql(&PostgreSqlDialect {}, &statements[0].to_string()).unwrap(),
        statements
    );
    assert!(Parser::parse_sql(&PostgreSqlDialect {}, "CREATE TYPE t ('input'=f)").is_err());
}

#[test]
fn type_alignment_uses_native_type_spelling_and_literal_rules() {
    for (argument, expected) in [
        ("integer", Alignment::Int4),
        ("int", Alignment::Int4),
        ("smallint", Alignment::Int2),
        ("double precision", Alignment::Double),
        ("pg_catalog.int4", Alignment::Int4),
        ("'FLOAT8'", Alignment::Double),
        ("'pg_catalog.bpchar'", Alignment::Char),
        ("\"int4\"", Alignment::Int4),
    ] {
        let statements = Parser::parse_sql(
            &PostgreSqlDialect {},
            &format!("CREATE TYPE t (ALIGNMENT={argument})"),
        )
        .unwrap();
        let [Statement::CreateType {
            representation: Some(UserDefinedTypeRepresentation::SqlDefinition { options }),
            ..
        }] = statements.as_slice()
        else {
            panic!("base type")
        };
        assert_eq!(options, &[Option::Alignment(expected)]);
    }
    for argument in ["'integer'", "\"integer\"", "unknown_alignment"] {
        let statements = Parser::parse_sql(
            &PostgreSqlDialect {},
            &format!("CREATE TYPE t (ALIGNMENT={argument})"),
        )
        .unwrap();
        let [Statement::CreateType {
            representation: Some(UserDefinedTypeRepresentation::SqlDefinition { options }),
            ..
        }] = statements.as_slice()
        else {
            panic!("base type")
        };
        assert!(matches!(&options[0], Option::AlignmentName(_)));
        assert_eq!(
            Parser::parse_sql(&PostgreSqlDialect {}, &statements[0].to_string()).unwrap(),
            statements
        );
    }
}

#[test]
fn unknown_definition_attributes_do_not_consume_following_options() {
    for sql in [
        "CREATE TYPE t (UNUSED=+, INPUT=real_input, UNUSED=-4)",
        "CREATE TYPE t (UNUSED='ignored', UNUSED=unused_type(4), OUTPUT=real_output)",
    ] {
        let statements = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
        assert_eq!(
            Parser::parse_sql(&PostgreSqlDialect {}, &statements[0].to_string()).unwrap(),
            statements
        );
    }
    assert!(Parser::parse_sql(&PostgreSqlDialect {}, "CREATE TYPE t (UNUSED=1+2)").is_err());
}
