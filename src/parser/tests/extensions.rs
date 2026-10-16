use crate::{
    ast::{AlterExtensionOperation, ExtensionMember, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};
#[test]
fn alter_extension_preserves_structured_whole_object_identity() {
    for source in [
        "ALTER EXTENSION e UPDATE",
        "ALTER EXTENSION e UPDATE TO '2.0'",
        "ALTER EXTENSION e SET SCHEMA destination",
        "ALTER EXTENSION e ADD TABLE public.t",
        "ALTER EXTENSION e DROP FUNCTION public.f(IN int, OUT text)",
        "ALTER EXTENSION e ADD AGGREGATE public.a(int ORDER BY text)",
        "ALTER EXTENSION e ADD OPERATOR public.!(NONE, int)",
        "ALTER EXTENSION e ADD OPERATOR CLASS public.ops USING btree",
        "ALTER EXTENSION e ADD CAST (int AS text)",
        "ALTER EXTENSION e ADD TRANSFORM FOR integer LANGUAGE plpgsql",
        "ALTER EXTENSION e ADD TEXT SEARCH CONFIGURATION public.cfg",
        "ALTER EXTENSION e ADD PROCEDURAL LANGUAGE plpgsql",
        "ALTER EXTENSION e ADD TYPE public.t[]",
    ] {
        let statements = Parser::parse_sql(&PostgreSqlDialect {}, source).unwrap();
        assert!(
            matches!(statements[0], Statement::AlterExtension(_)),
            "{source}"
        );
        assert_eq!(
            Parser::parse_sql(&PostgreSqlDialect {}, &statements[0].to_string()).unwrap(),
            statements,
            "{source}"
        );
    }
    let statements = Parser::parse_sql(
        &PostgreSqlDialect {},
        "ALTER EXTENSION e ADD AGGREGATE a(int ORDER BY text)",
    )
    .unwrap();
    let Statement::AlterExtension(extension) = &statements[0] else {
        panic!("{statements:?}")
    };
    let AlterExtensionOperation::Member {
        object: ExtensionMember::Routine { routine, .. },
        ..
    } = &extension.operation
    else {
        panic!("{extension:?}")
    };
    assert_eq!(routine.args.as_ref().unwrap().len(), 2);
}
#[test]
fn alter_extension_rejects_multiple_versions_and_invalid_member_grammar() {
    for sql in [
        "ALTER EXTENSION e UPDATE TO '2' TO '3'",
        "ALTER EXTENSION e ADD",
        "ALTER EXTENSION e SET SCHEMA",
        "ALTER EXTENSION e ADD CAST (int text)",
        "ALTER EXTENSION e ADD OPERATOR CLASS ops",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, sql).is_err(),
            "{sql}"
        );
    }
}
