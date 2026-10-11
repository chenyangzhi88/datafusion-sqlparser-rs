use crate::ast::helpers::stmt_create_table::CreateTableBuilder;
use crate::ast::{Ident, ObjectName, Statement};
use crate::parser::ParserError;

#[test]
pub fn test_from_valid_statement() {
    let builder = CreateTableBuilder::new(ObjectName::from(vec![Ident::new("table_name")]));

    let create_table = builder.clone().build();
    let stmt: Statement = create_table.into();

    assert_eq!(builder, CreateTableBuilder::try_from(stmt).unwrap());
}

#[test]
pub fn test_from_invalid_statement() {
    let stmt = Statement::Commit {
        completion: Default::default(),
        end: false,
        modifier: None,
    };

    assert_eq!(
        CreateTableBuilder::try_from(stmt).unwrap_err(),
        ParserError::ParserError(
            "Expected create table statement, but received: COMMIT".to_owned()
        )
    );
}
