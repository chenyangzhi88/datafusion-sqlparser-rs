//! PostgreSQL SQL/XML argument grammar lowers to the exact two-argument routine.
use super::*;
impl Parser<'_> {
    pub(super) fn parse_postgres_xml_exists(
        &mut self,
        name: ObjectName,
    ) -> Result<Function, ParserError> {
        self.expect_token(&Token::LParen)?;
        let first = self.parse_expr()?;
        let mut args = vec![FunctionArg::Unnamed(FunctionArgExpr::Expr(first))];
        if self.parse_keyword(Keyword::PASSING) {
            self.parse_postgres_xml_mechanism()?;
            args.push(FunctionArg::Unnamed(FunctionArgExpr::Expr(
                self.parse_expr()?,
            )));
            self.parse_postgres_xml_mechanism()?;
        } else {
            while self.consume_token(&Token::Comma) {
                args.push(self.parse_function_args()?);
            }
        }
        self.expect_token(&Token::RParen)?;
        Ok(Function {
            mysql_literal_source: None,
            name,
            uses_odbc_syntax: false,
            origin: crate::ast::FunctionOrigin::Source,
            parameters: FunctionArguments::None,
            args: FunctionArguments::List(FunctionArgumentList {
                duplicate_treatment: None,
                args,
                clauses: vec![],
            }),
            filter: None,
            null_treatment: None,
            from_first_last: None,
            over: None,
            within_group: vec![],
        })
    }
    fn parse_postgres_xml_mechanism(&mut self) -> Result<(), ParserError> {
        if self.parse_keyword(Keyword::BY)
            && self
                .parse_one_of_keywords(&[Keyword::REF, Keyword::VALUE])
                .is_none()
        {
            return self.expected_ref("REF or VALUE", self.peek_token_ref());
        }
        Ok(())
    }
}
