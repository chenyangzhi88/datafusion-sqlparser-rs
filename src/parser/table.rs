// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;

impl Parser<'_> {
    pub(super) fn parse_parenthesized_table_factor(&mut self) -> Result<TableFactor, ParserError> {
        // A left paren introduces either a derived table (i.e., a subquery)
        // or a nested join. It's nearly impossible to determine ahead of
        // time which it is... so we just try to parse both.
        //
        // Here's an example that demonstrates the complexity:
        //                     /-------------------------------------------------------\
        //                     | /-----------------------------------\                 |
        //     SELECT * FROM ( ( ( (SELECT 1) UNION (SELECT 2) ) AS t1 NATURAL JOIN t2 ) )
        //                   ^ ^ ^ ^
        //                   | | | |
        //                   | | | |
        //                   | | | (4) belongs to a SetExpr::Query inside the subquery
        //                   | | (3) starts a derived table (subquery)
        //                   | (2) starts a nested join
        //                   (1) an additional set of parens around a nested join
        //

        // If the recently consumed '(' starts a derived table, the call to
        // `parse_derived_table_factor` below will return success after parsing the
        // subquery, followed by the closing ')', and the alias of the derived table.
        // In the example above this is case (3).
        if let Some(mut table) =
            self.maybe_parse(|parser| parser.parse_derived_table_factor(NotLateral))?
        {
            while let Some(kw) = self.parse_one_of_keywords(&[Keyword::PIVOT, Keyword::UNPIVOT]) {
                table = match kw {
                    Keyword::PIVOT => self.parse_pivot_table_factor(table)?,
                    Keyword::UNPIVOT => self.parse_unpivot_table_factor(table)?,
                    unexpected_keyword => {
                        return Err(ParserError::ParserError(format!(
                            "Internal parser error: unexpected keyword `{unexpected_keyword}` in pivot/unpivot"
                        )));
                    }
                }
            }
            return Ok(table);
        }

        // A parsing error from `parse_derived_table_factor` indicates that the '(' we've
        // recently consumed does not start a derived table (cases 1, 2, or 4).
        // `maybe_parse` will ignore such an error and rewind to be after the opening '('.

        // Inside the parentheses we expect to find an (A) table factor
        // followed by some joins or (B) another level of nesting.
        let mut table_and_joins = self.parse_table_and_joins()?;

        #[allow(clippy::if_same_then_else)]
        if !table_and_joins.joins.is_empty() {
            self.expect_token(&Token::RParen)?;
            let alias = self.maybe_parse_table_alias()?;
            Ok(TableFactor::NestedJoin {
                table_with_joins: Box::new(table_and_joins),
                alias,
            }) // (A)
        } else if let TableFactor::NestedJoin {
            table_with_joins: _,
            alias: _,
        } = &table_and_joins.relation
        {
            // (B): `table_and_joins` (what we found inside the parentheses)
            // is a nested join `(foo JOIN bar)`, not followed by other joins.
            self.expect_token(&Token::RParen)?;
            let alias = self.maybe_parse_table_alias()?;
            Ok(TableFactor::NestedJoin {
                table_with_joins: Box::new(table_and_joins),
                alias,
            })
        } else if dialect_of!(self is MySqlDialect)
            && matches!(&table_and_joins.relation, TableFactor::Table { .. })
        {
            // Native MySQL parentheses preserve this same base Table_ref,
            // including its inner alias. An outer alias belongs to neither
            // this reduction nor this Source, unlike Snowflake below.
            self.expect_token(&Token::RParen)?;
            Ok(table_and_joins.relation)
        } else if self.dialect.supports_parens_around_table_factor() {
            // Dialect-specific behavior: Snowflake diverges from the
            // standard and from most of the other implementations by
            // allowing extra parentheses not only around a join (B), but
            // around lone table names (e.g. `FROM (mytable [AS alias])`)
            // and around derived tables (e.g. `FROM ((SELECT ...)
            // [AS alias])`) as well.
            self.expect_token(&Token::RParen)?;

            if let Some(outer_alias) = self.maybe_parse_table_alias()? {
                // Snowflake also allows specifying an alias *after* parens
                // e.g. `FROM (mytable) AS alias`
                match &mut table_and_joins.relation {
                    TableFactor::Derived { alias, .. }
                    | TableFactor::Table { alias, .. }
                    | TableFactor::Function { alias, .. }
                    | TableFactor::RowsFrom { alias, .. }
                    | TableFactor::UNNEST { alias, .. }
                    | TableFactor::JsonTable { alias, .. }
                    | TableFactor::XmlTable { alias, .. }
                    | TableFactor::OpenJsonTable { alias, .. }
                    | TableFactor::TableFunction { alias, .. }
                    | TableFactor::Pivot { alias, .. }
                    | TableFactor::Unpivot { alias, .. }
                    | TableFactor::MatchRecognize { alias, .. }
                    | TableFactor::SemanticView { alias, .. }
                    | TableFactor::NestedJoin { alias, .. } => {
                        // but not `FROM (mytable AS alias1) AS alias2`.
                        if let Some(inner_alias) = alias {
                            return Err(ParserError::ParserError(format!(
                                "duplicate alias {inner_alias}"
                            )));
                        }
                        // Act as if the alias was specified normally next
                        // to the table name: `(mytable) AS alias` ->
                        // `(mytable AS alias)`
                        alias.replace(outer_alias);
                    }
                };
            }
            // Do not store the extra set of parens in the AST
            Ok(table_and_joins.relation)
        } else {
            // The SQL spec prohibits derived tables and bare tables from
            // appearing alone in parentheses (e.g. `FROM (mytable)`)
            self.expected_ref("joined table", self.peek_token_ref())
        }
    }

}
