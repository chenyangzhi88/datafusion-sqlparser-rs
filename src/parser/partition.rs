//! PostgreSQL partition key identities and bound syntax.
use super::*;
impl Parser<'_> {
    pub(super) fn parse_postgres_table_partition_by(
        &mut self,
    ) -> Result<TablePartitionBy, ParserError> {
        let strategy = self.parse_identifier()?;
        self.expect_token(&Token::LParen)?;
        let keys = self.parse_comma_separated(|parser| {
            let start = parser.peek_token();
            let expression = parser.parse_expr()?;
            let key = match &expression {
                Expr::Collate { expr, .. } => expr.as_ref(),
                expression => expression,
            };
            let valid = match key {
                Expr::Identifier(_) | Expr::Nested(_) => true,
                Expr::Function(function) => function.over.is_none(),
                _ => false,
            };
            if !valid {
                return parser.expected(
                    "a partition column, function, or parenthesized expression",
                    start,
                );
            }
            let operator_class = if matches!(parser.peek_token_ref().token, Token::Word(_)) {
                Some(parser.parse_object_name(false)?)
            } else {
                None
            };
            Ok(PartitionKey {
                expression,
                operator_class,
            })
        })?;
        self.expect_token(&Token::RParen)?;
        Ok(TablePartitionBy::PostgreSql { strategy, keys })
    }
    /// Parse [ForValues] of a `PARTITION OF` clause.
    ///
    /// Parses: `FOR VALUES partition_bound_spec | DEFAULT`
    ///
    /// [PostgreSQL](https://www.postgresql.org/docs/current/sql-createtable.html)
    pub fn parse_partition_for_values(&mut self) -> Result<ForValues, ParserError> {
        let default_token = AttachedToken(self.peek_token());
        if self.parse_keyword(Keyword::DEFAULT) {
            return Ok(ForValues::Default {
                token: default_token,
            });
        }

        self.expect_keywords(&[Keyword::FOR, Keyword::VALUES])?;
        let token = AttachedToken(self.peek_token());

        if self.parse_keyword(Keyword::IN) {
            // FOR VALUES IN (expr, ...)
            self.expect_token(&Token::LParen)?;
            if self.peek_token_ref().token == Token::RParen {
                return self.expected_ref("at least one value", self.peek_token_ref());
            }
            let values = self.parse_comma_separated(Parser::parse_expr)?;
            self.expect_token(&Token::RParen)?;
            Ok(ForValues::In { values, token })
        } else if self.parse_keyword(Keyword::FROM) {
            // FOR VALUES FROM (...) TO (...)
            self.expect_token(&Token::LParen)?;
            if self.peek_token_ref().token == Token::RParen {
                return self.expected_ref("at least one value", self.peek_token_ref());
            }
            let from = self.parse_comma_separated(Parser::parse_partition_bound_value)?;
            self.expect_token(&Token::RParen)?;
            self.expect_keyword(Keyword::TO)?;
            self.expect_token(&Token::LParen)?;
            if self.peek_token_ref().token == Token::RParen {
                return self.expected_ref("at least one value", self.peek_token_ref());
            }
            let to = self.parse_comma_separated(Parser::parse_partition_bound_value)?;
            self.expect_token(&Token::RParen)?;
            Ok(ForValues::From { from, to, token })
        } else if self.parse_keyword(Keyword::WITH) {
            // FOR VALUES WITH (MODULUS n, REMAINDER r)
            self.expect_token(&Token::LParen)?;
            self.expect_keyword(Keyword::MODULUS)?;
            let modulus = self.parse_literal_uint()?;
            self.expect_token(&Token::Comma)?;
            self.expect_keyword(Keyword::REMAINDER)?;
            let remainder = self.parse_literal_uint()?;
            self.expect_token(&Token::RParen)?;
            Ok(ForValues::With {
                modulus,
                remainder,
                token,
            })
        } else {
            self.expected_ref("IN, FROM, or WITH after FOR VALUES", self.peek_token_ref())
        }
    }

    /// Parse a single partition bound value (MINVALUE, MAXVALUE, or expression).
    pub(super) fn parse_partition_bound_value(
        &mut self,
    ) -> Result<PartitionBoundValue, ParserError> {
        let token = AttachedToken(self.peek_token());
        if self.parse_keyword(Keyword::MINVALUE) {
            Ok(PartitionBoundValue::MinValue(token))
        } else if self.parse_keyword(Keyword::MAXVALUE) {
            Ok(PartitionBoundValue::MaxValue(token))
        } else {
            Ok(PartitionBoundValue::Expr(self.parse_expr()?))
        }
    }
}
