use crate::{
    ast::{Expr, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};

#[test]
fn search_and_cycle_preserve_clause_fields_and_roundtrip() {
    for suffix in [
        "SEARCH BREADTH FIRST BY k, v SET seq",
        "SEARCH DEPTH FIRST BY k, v SET seq",
        "SEARCH DEPTH FIRST BY k SET seq CYCLE k, v SET cyclic USING path",
        "CYCLE k SET cyclic TO 'yes' DEFAULT 'no' USING path",
        "CYCLE k SET cyclic TO -1 DEFAULT +1 USING path",
    ] {
        let sql = format!(
            "WITH RECURSIVE t(k,v) AS (SELECT 1,2 UNION ALL SELECT k+1,v FROM t WHERE k<3) {suffix} SELECT * FROM t"
        );
        let parsed = Parser::parse_sql(&PostgreSqlDialect {}, &sql).unwrap();
        let Statement::Query(query) = &parsed[0] else {
            panic!("query expected");
        };
        let cte = &query.with.as_ref().unwrap().cte_tables[0];
        assert_eq!(cte.search.is_some(), suffix.starts_with("SEARCH"));
        assert_eq!(cte.cycle.is_some(), suffix.contains("CYCLE"));
        if let Some(search) = &cte.search {
            assert_eq!(search.breadth_first, suffix.starts_with("SEARCH BREADTH"));
            assert_eq!(search.sequence_column.value, "seq");
            assert!(!search.columns.is_empty());
        }
        let again = Parser::parse_sql(&PostgreSqlDialect {}, &parsed[0].to_string()).unwrap();
        assert_eq!(parsed, again);
    }
}

#[test]
fn current_of_is_a_positioned_mutation_predicate_not_an_identifier() {
    for sql in [
        "UPDATE t SET k=2 WHERE CURRENT OF \"cursor name\"",
        "DELETE FROM t WHERE CURRENT OF c",
    ] {
        let parsed = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
        let predicate = match &parsed[0] {
            Statement::Update(update) => update.selection.as_ref().unwrap(),
            Statement::Delete(delete) => delete.selection.as_ref().unwrap(),
            _ => panic!("mutation expected"),
        };
        assert!(matches!(predicate, Expr::CurrentOf { .. }));
        assert_eq!(
            parsed,
            Parser::parse_sql(&PostgreSqlDialect {}, &parsed[0].to_string()).unwrap()
        );
    }
    for sql in [
        "SELECT * FROM t WHERE CURRENT OF c",
        "DELETE FROM t WHERE CURRENT OF $0",
        "DELETE FROM t WHERE CURRENT OF $2",
        "UPDATE t SET k=1 WHERE CURRENT OF c AND true",
    ] {
        assert!(
            Parser::parse_sql(&PostgreSqlDialect {}, sql).is_err(),
            "{sql}"
        );
    }
}
