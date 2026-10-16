// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
#[cfg(not(feature = "std"))]
use alloc::string::ToString;

use core::any::TypeId;

use crate::{
    ast::{BinaryOperator, Expr, LockTable, LockTableType, Statement, UnaryOperator},
    dialect::{Dialect, Precedence},
    keywords::Keyword,
    mysql_keywords::mysql_8_0_46_keyword,
    parser::{Parser, ParserError},
};

use super::keywords;

const RESERVED_FOR_TABLE_ALIAS_MYSQL: &[Keyword] = &[
    Keyword::USE,
    Keyword::IGNORE,
    Keyword::FORCE,
    Keyword::STRAIGHT_JOIN,
];

/// A [`Dialect`] for [MySQL](https://www.mysql.com/)
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MySqlDialect {}

/// Session SQL-mode switches that affect MySQL tokenization or expression
/// parsing. The wrapper preserves [`MySqlDialect`]'s type identity, so parser
/// branches guarded by `dialect_of!(.. is MySqlDialect)` keep working.
#[derive(Debug, Clone, Copy)]
struct MySqlDialectWithOptions {
    ansi_quotes: bool,
    pipes_as_concat: bool,
    no_backslash_escapes: bool,
}

impl MySqlDialect {
    /// Construct a MySQL dialect for the lexical SQL modes of one session.
    ///
    /// Keeping these switches on the dialect instance avoids process-global
    /// parser state and makes concurrent sessions deterministic.
    pub fn with_options(
        ansi_quotes: bool,
        pipes_as_concat: bool,
        no_backslash_escapes: bool,
    ) -> impl Dialect {
        MySqlDialectWithOptions {
            ansi_quotes,
            pipes_as_concat,
            no_backslash_escapes,
        }
    }
}

fn parse_mysql_infix(
    parser: &mut Parser,
    expr: &Expr,
    precedence: u8,
    pipes_as_concat: bool,
) -> Option<Result<Expr, ParserError>> {
    let operator = if parser.parse_keyword(Keyword::SOUNDS) {
        if let Err(error) = parser.expect_keyword_is(Keyword::LIKE) {
            return Some(Err(error));
        }
        BinaryOperator::Custom("SOUNDS LIKE".into())
    } else if parser.parse_keyword(Keyword::DIV) {
        BinaryOperator::MyIntegerDivide
    } else if parser.parse_keyword(Keyword::MOD) {
        BinaryOperator::Modulo
    } else if parser.consume_token(&crate::tokenizer::Token::StringConcat) {
        if pipes_as_concat {
            BinaryOperator::StringConcat
        } else {
            BinaryOperator::Or
        }
    } else {
        return None;
    };
    Some(
        parser
            .parse_subexpr(precedence)
            .map(|right| Expr::BinaryOp {
                left: Box::new(expr.clone()),
                op: operator,
                right: Box::new(right),
            }),
    )
}

const MYSQL_CONCAT_PRECEDENCE: u8 = 45;

fn parse_mysql_prefix(
    parser: &mut Parser,
    pipes_as_concat: bool,
) -> Option<Result<Expr, ParserError>> {
    use crate::tokenizer::Token;

    let operator = match &parser.peek_token_ref().token {
        Token::ExclamationMark => UnaryOperator::BangNot,
        Token::Plus if pipes_as_concat => UnaryOperator::Plus,
        Token::Minus if pipes_as_concat => UnaryOperator::Minus,
        Token::Tilde if pipes_as_concat => UnaryOperator::BitwiseNot,
        _ => return None,
    };
    parser.next_token();
    Some(
        parser
            .parse_subexpr(MYSQL_CONCAT_PRECEDENCE)
            .map(|expression| Expr::UnaryOp {
                op: operator,
                expr: Box::new(expression),
            }),
    )
}

impl Dialect for MySqlDialect {
    fn is_identifier_start(&self, ch: char) -> bool {
        // See https://dev.mysql.com/doc/refman/8.0/en/identifiers.html.
        // Identifiers which begin with a digit are recognized while tokenizing numbers,
        // so they can be distinguished from exponent numeric literals.
        // MySQL also implements non ascii utf-8 charecters
        ch.is_alphabetic()
            || ch == '_'
            || ch == '$'
            || ch == '@'
            || ('\u{0080}'..='\u{ffff}').contains(&ch)
            || !ch.is_ascii()
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        self.is_identifier_start(ch) || ch.is_ascii_digit() ||
        // MySQL implements Unicode characters in identifiers.
        !ch.is_ascii()
    }

    fn is_delimited_identifier_start(&self, ch: char) -> bool {
        ch == '`'
    }

    fn identifier_quote_style(&self, _identifier: &str) -> Option<char> {
        Some('`')
    }

    // See https://dev.mysql.com/doc/refman/8.0/en/string-literals.html#character-escape-sequences
    fn supports_string_literal_backslash_escape(&self) -> bool {
        true
    }

    /// see <https://dev.mysql.com/doc/refman/8.4/en/string-functions.html#function_concat>
    fn supports_string_literal_concatenation(&self) -> bool {
        true
    }

    fn ignores_wildcard_escapes(&self) -> bool {
        true
    }

    fn supports_numeric_prefix(&self) -> bool {
        true
    }

    fn supports_bitwise_shift_operators(&self) -> bool {
        true
    }

    /// see <https://dev.mysql.com/doc/refman/8.4/en/comments.html>
    fn supports_multiline_comment_hints(&self) -> bool {
        true
    }

    fn parse_infix(
        &self,
        parser: &mut crate::parser::Parser,
        expr: &crate::ast::Expr,
        precedence: u8,
    ) -> Option<Result<crate::ast::Expr, ParserError>> {
        parse_mysql_infix(parser, expr, precedence, false)
    }

    fn parse_prefix(&self, parser: &mut Parser) -> Option<Result<Expr, ParserError>> {
        parse_mysql_prefix(parser, false)
    }

    fn get_next_precedence(&self, parser: &Parser) -> Option<Result<u8, ParserError>> {
        match &parser.peek_token_ref().token {
            crate::tokenizer::Token::Word(word)
                if matches!(word.keyword, Keyword::DIV | Keyword::MOD) =>
            {
                Some(Ok(self.prec_value(Precedence::MulDivModOp)))
            }
            crate::tokenizer::Token::Word(word)
                if word.keyword == Keyword::SOUNDS
                    && matches!(
                        &parser.peek_nth_token_ref(1).token,
                        crate::tokenizer::Token::Word(next) if next.keyword == Keyword::LIKE
                    ) =>
            {
                Some(Ok(self.prec_value(Precedence::Like)))
            }
            crate::tokenizer::Token::StringConcat => Some(Ok(self.prec_value(Precedence::Or))),
            crate::tokenizer::Token::Word(word) if word.keyword == Keyword::XOR => Some(Ok(7)),
            // MySQL's JSON extraction operators are aliases for JSON_EXTRACT
            // and JSON_UNQUOTE(JSON_EXTRACT(...)).  They therefore bind like
            // function application, before comparison operators.  The shared
            // PostgreSQL precedence is deliberately lower than `=`, which
            // would parse `doc->>'$.name' = 'value'` with the comparison as the
            // JSON path argument.
            crate::tokenizer::Token::Arrow | crate::tokenizer::Token::LongArrow => {
                Some(Ok(self.prec_value(Precedence::DoubleColon)))
            }
            _ => None,
        }
    }

    fn supports_bang_not_operator(&self) -> bool {
        true
    }

    fn parse_statement(&self, parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
        if parser.parse_keywords(&[Keyword::LOCK, Keyword::TABLES]) {
            Some(parse_lock_tables(parser))
        } else if parser.parse_keywords(&[Keyword::UNLOCK, Keyword::TABLES]) {
            Some(parse_unlock_tables(parser))
        } else {
            None
        }
    }

    fn require_interval_qualifier(&self) -> bool {
        true
    }

    fn supports_limit_comma(&self) -> bool {
        true
    }

    /// See: <https://dev.mysql.com/doc/refman/8.4/en/create-table-select.html>
    fn supports_create_table_select(&self) -> bool {
        true
    }

    /// See: <https://dev.mysql.com/doc/refman/8.4/en/insert.html>
    fn supports_insert_set(&self) -> bool {
        true
    }

    fn supports_user_host_grantee(&self) -> bool {
        true
    }

    fn is_table_factor_alias(&self, explicit: bool, kw: &Keyword, _parser: &mut Parser) -> bool {
        explicit
            || (!keywords::RESERVED_FOR_TABLE_ALIAS.contains(kw)
                && !RESERVED_FOR_TABLE_ALIAS_MYSQL.contains(kw))
    }

    fn is_select_item_alias(&self, explicit: bool, kw: &Keyword, parser: &mut Parser) -> bool {
        // MySQL permits END as an unquoted select-item alias. A stored-program
        // END is reached only after the statement delimiter, so accepting it
        // here does not consume a properly delimited block terminator.
        *kw != Keyword::WINDOW
            && (*kw == Keyword::END || explicit || self.is_column_alias(kw, parser))
    }

    fn is_column_alias(&self, kw: &Keyword, _parser: &mut Parser) -> bool {
        *kw != Keyword::WINDOW && !keywords::RESERVED_FOR_COLUMN_ALIAS.contains(kw)
    }

    fn is_reserved_for_identifier(&self, kw: Keyword) -> bool {
        kw != Keyword::NoKeyword
            && mysql_8_0_46_keyword(&kw.to_string()).is_some_and(|keyword| keyword.reserved)
    }

    fn supports_table_hints(&self) -> bool {
        true
    }

    fn requires_single_line_comment_whitespace(&self) -> bool {
        true
    }

    fn supports_match_against(&self) -> bool {
        true
    }

    fn supports_select_modifiers(&self) -> bool {
        true
    }

    fn supports_set_names(&self) -> bool {
        true
    }

    fn supports_set_names_collation(&self) -> bool {
        true
    }

    fn supports_comma_separated_set_assignments(&self) -> bool {
        true
    }

    /// See: <https://dev.mysql.com/doc/refman/8.4/en/update.html>
    fn supports_update_order_by(&self) -> bool {
        true
    }

    fn supports_data_type_signed_suffix(&self) -> bool {
        true
    }

    fn supports_cross_join_constraint(&self) -> bool {
        true
    }

    /// See: <https://dev.mysql.com/doc/refman/8.4/en/expressions.html>
    fn supports_double_ampersand_operator(&self) -> bool {
        true
    }

    /// Deprecated functionality by MySQL but still supported
    /// See: <https://dev.mysql.com/doc/refman/8.4/en/cast-functions.html#operator_binary>
    fn supports_binary_kw_as_cast(&self) -> bool {
        true
    }

    fn supports_comment_optimizer_hint(&self) -> bool {
        true
    }

    /// See: <https://dev.mysql.com/doc/refman/8.4/en/create-table.html>
    fn supports_constraint_keyword_without_name(&self) -> bool {
        true
    }

    /// See: <https://dev.mysql.com/doc/refman/8.4/en/create-table.html>
    fn supports_key_column_option(&self) -> bool {
        true
    }

    fn supports_group_by_with_modifier(&self) -> bool {
        true
    }
}

impl Dialect for MySqlDialectWithOptions {
    fn dialect(&self) -> TypeId {
        TypeId::of::<MySqlDialect>()
    }

    fn is_identifier_start(&self, ch: char) -> bool {
        MySqlDialect {}.is_identifier_start(ch)
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        MySqlDialect {}.is_identifier_part(ch)
    }

    fn is_delimited_identifier_start(&self, ch: char) -> bool {
        ch == '`' || (self.ansi_quotes && ch == '"')
    }

    fn identifier_quote_style(&self, identifier: &str) -> Option<char> {
        MySqlDialect {}.identifier_quote_style(identifier)
    }

    fn supports_string_literal_backslash_escape(&self) -> bool {
        !self.no_backslash_escapes
    }

    fn supports_string_literal_concatenation(&self) -> bool {
        MySqlDialect {}.supports_string_literal_concatenation()
    }

    fn ignores_wildcard_escapes(&self) -> bool {
        MySqlDialect {}.ignores_wildcard_escapes()
    }

    fn supports_numeric_prefix(&self) -> bool {
        MySqlDialect {}.supports_numeric_prefix()
    }

    fn supports_bitwise_shift_operators(&self) -> bool {
        MySqlDialect {}.supports_bitwise_shift_operators()
    }

    fn supports_multiline_comment_hints(&self) -> bool {
        MySqlDialect {}.supports_multiline_comment_hints()
    }

    fn parse_infix(
        &self,
        parser: &mut Parser,
        expr: &Expr,
        precedence: u8,
    ) -> Option<Result<Expr, ParserError>> {
        parse_mysql_infix(parser, expr, precedence, self.pipes_as_concat)
    }

    fn parse_prefix(&self, parser: &mut Parser) -> Option<Result<Expr, ParserError>> {
        parse_mysql_prefix(parser, self.pipes_as_concat)
    }

    fn get_next_precedence(&self, parser: &Parser) -> Option<Result<u8, ParserError>> {
        match &parser.peek_token_ref().token {
            crate::tokenizer::Token::Word(word)
                if matches!(word.keyword, Keyword::DIV | Keyword::MOD) =>
            {
                Some(Ok(self.prec_value(Precedence::MulDivModOp)))
            }
            crate::tokenizer::Token::Word(word)
                if word.keyword == Keyword::SOUNDS
                    && matches!(
                        &parser.peek_nth_token_ref(1).token,
                        crate::tokenizer::Token::Word(next) if next.keyword == Keyword::LIKE
                    ) =>
            {
                Some(Ok(self.prec_value(Precedence::Like)))
            }
            crate::tokenizer::Token::StringConcat if self.pipes_as_concat => {
                // MySQL places concatenation between unary operators and `^`.
                Some(Ok(MYSQL_CONCAT_PRECEDENCE))
            }
            crate::tokenizer::Token::StringConcat => Some(Ok(self.prec_value(Precedence::Or))),
            crate::tokenizer::Token::Word(word) if word.keyword == Keyword::XOR => Some(Ok(7)),
            crate::tokenizer::Token::Arrow | crate::tokenizer::Token::LongArrow => {
                Some(Ok(self.prec_value(Precedence::DoubleColon)))
            }
            _ => None,
        }
    }

    fn supports_bang_not_operator(&self) -> bool {
        true
    }

    fn parse_statement(&self, parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
        MySqlDialect {}.parse_statement(parser)
    }

    fn require_interval_qualifier(&self) -> bool {
        MySqlDialect {}.require_interval_qualifier()
    }

    fn supports_limit_comma(&self) -> bool {
        MySqlDialect {}.supports_limit_comma()
    }

    fn supports_create_table_select(&self) -> bool {
        MySqlDialect {}.supports_create_table_select()
    }

    fn supports_insert_set(&self) -> bool {
        MySqlDialect {}.supports_insert_set()
    }

    fn supports_user_host_grantee(&self) -> bool {
        MySqlDialect {}.supports_user_host_grantee()
    }

    fn is_table_factor_alias(&self, explicit: bool, kw: &Keyword, parser: &mut Parser) -> bool {
        MySqlDialect {}.is_table_factor_alias(explicit, kw, parser)
    }

    fn is_select_item_alias(&self, explicit: bool, kw: &Keyword, parser: &mut Parser) -> bool {
        MySqlDialect {}.is_select_item_alias(explicit, kw, parser)
    }

    fn is_column_alias(&self, kw: &Keyword, parser: &mut Parser) -> bool {
        MySqlDialect {}.is_column_alias(kw, parser)
    }

    fn is_reserved_for_identifier(&self, kw: Keyword) -> bool {
        MySqlDialect {}.is_reserved_for_identifier(kw)
    }

    fn supports_table_hints(&self) -> bool {
        MySqlDialect {}.supports_table_hints()
    }

    fn requires_single_line_comment_whitespace(&self) -> bool {
        MySqlDialect {}.requires_single_line_comment_whitespace()
    }

    fn supports_match_against(&self) -> bool {
        MySqlDialect {}.supports_match_against()
    }

    fn supports_select_modifiers(&self) -> bool {
        MySqlDialect {}.supports_select_modifiers()
    }

    fn supports_set_names(&self) -> bool {
        MySqlDialect {}.supports_set_names()
    }

    fn supports_set_names_identifier(&self) -> bool {
        MySqlDialect {}.supports_set_names_identifier()
    }

    fn supports_set_names_collation(&self) -> bool {
        MySqlDialect {}.supports_set_names_collation()
    }

    fn supports_comma_separated_set_assignments(&self) -> bool {
        MySqlDialect {}.supports_comma_separated_set_assignments()
    }

    fn supports_update_order_by(&self) -> bool {
        MySqlDialect {}.supports_update_order_by()
    }

    fn supports_data_type_signed_suffix(&self) -> bool {
        MySqlDialect {}.supports_data_type_signed_suffix()
    }

    fn supports_cross_join_constraint(&self) -> bool {
        MySqlDialect {}.supports_cross_join_constraint()
    }

    fn supports_double_ampersand_operator(&self) -> bool {
        MySqlDialect {}.supports_double_ampersand_operator()
    }

    fn supports_binary_kw_as_cast(&self) -> bool {
        MySqlDialect {}.supports_binary_kw_as_cast()
    }

    fn supports_comment_optimizer_hint(&self) -> bool {
        MySqlDialect {}.supports_comment_optimizer_hint()
    }

    fn supports_constraint_keyword_without_name(&self) -> bool {
        MySqlDialect {}.supports_constraint_keyword_without_name()
    }

    fn supports_key_column_option(&self) -> bool {
        MySqlDialect {}.supports_key_column_option()
    }

    fn supports_group_by_with_modifier(&self) -> bool {
        true
    }
}

/// `LOCK TABLES`
/// <https://dev.mysql.com/doc/refman/8.0/en/lock-tables.html>
fn parse_lock_tables(parser: &mut Parser) -> Result<Statement, ParserError> {
    let tables = parser.parse_comma_separated(parse_lock_table)?;
    Ok(Statement::LockTables { tables })
}

// tbl_name [[AS] alias] lock_type
fn parse_lock_table(parser: &mut Parser) -> Result<LockTable, ParserError> {
    let table = parser.parse_identifier()?;
    let alias =
        parser.parse_optional_alias(&[Keyword::READ, Keyword::WRITE, Keyword::LOW_PRIORITY])?;
    let lock_type = parse_lock_tables_type(parser)?;

    Ok(LockTable {
        table,
        alias,
        lock_type,
    })
}

// READ [LOCAL] | [LOW_PRIORITY] WRITE
fn parse_lock_tables_type(parser: &mut Parser) -> Result<LockTableType, ParserError> {
    if parser.parse_keyword(Keyword::READ) {
        if parser.parse_keyword(Keyword::LOCAL) {
            Ok(LockTableType::Read { local: true })
        } else {
            Ok(LockTableType::Read { local: false })
        }
    } else if parser.parse_keyword(Keyword::WRITE) {
        Ok(LockTableType::Write {
            low_priority: false,
        })
    } else if parser.parse_keywords(&[Keyword::LOW_PRIORITY, Keyword::WRITE]) {
        Ok(LockTableType::Write { low_priority: true })
    } else {
        parser.expected_ref("an lock type in LOCK TABLES", parser.peek_token_ref())
    }
}

/// UNLOCK TABLES
/// <https://dev.mysql.com/doc/refman/8.0/en/lock-tables.html>
fn parse_unlock_tables(_parser: &mut Parser) -> Result<Statement, ParserError> {
    Ok(Statement::UnlockTables)
}
