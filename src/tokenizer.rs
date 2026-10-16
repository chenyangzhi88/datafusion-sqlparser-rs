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

//! SQL Tokenizer
//!
//! The tokenizer (a.k.a. lexer) converts a string into a sequence of tokens.
//!
//! The tokens then form the input for the parser, which outputs an Abstract Syntax Tree (AST).

#[cfg(not(feature = "std"))]
use alloc::{
    borrow::ToOwned,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::num::NonZeroU8;
use core::str::Chars;
use core::{cmp, fmt};
use core::{iter::Peekable, str};

#[cfg(not(feature = "std"))]
use alloc::sync::Arc;
#[cfg(feature = "std")]
use std::sync::Arc;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "visitor")]
use sqlparser_derive::{Visit, VisitMut};

use crate::dialect::Dialect;
use crate::dialect::{
    BigQueryDialect, DuckDbDialect, GenericDialect, MySqlDialect, PostgreSqlDialect,
    SnowflakeDialect,
};
use crate::keywords::{Keyword, ALL_KEYWORDS, ALL_KEYWORDS_INDEX};
use crate::{
    ast::{
        DollarQuotedString, MySqlText, QuoteDelimitedString, SqlCppPiece, SqlCppPieceRange,
        SqlSourceAdmission, SqlSourceOccurrence, SqlSourceOwner,
    },
    dialect::HiveDialect,
};

/// Actual MySQL binary-literal lexical production. It is not a Source
/// capability, decoded-character domain, or a statement permission.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub enum MySqlHexLiteralKind {
    /// Unquoted HEX_NUM (0x...), including native odd-nibble padding.
    HexNumber,
    /// Quoted HEX_STRING (X'...').
    HexQuoted,
    /// Unquoted BIN_NUM (0b...).
    BitNumber,
    /// Quoted BIN_STRING (B'...').
    BitQuoted,
}

/// SQL Token enumeration
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub enum Token {
    /// Actual MySQL character bytes; no Unicode mirror or compiler origin.
    MySqlText(MySqlText),
    /// An end-of-file marker, not a real token
    EOF,
    /// A keyword (like SELECT) or an optionally quoted SQL identifier
    Word(Word),
    /// An unsigned numeric literal
    Number(String, bool),
    /// A character that could not be tokenized
    Char(char),
    /// Single quoted string: i.e: 'string'
    SingleQuotedString(String),
    /// Double quoted string: i.e: "string"
    DoubleQuotedString(String),
    /// Triple single quoted strings: Example '''abc'''
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    TripleSingleQuotedString(String),
    /// Triple double quoted strings: Example """abc"""
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    TripleDoubleQuotedString(String),
    /// Dollar quoted string: i.e: $$string$$ or $tag_name$string$tag_name$
    DollarQuotedString(DollarQuotedString),
    /// Byte string literal: i.e: b'string' or B'string' (note that some backends, such as
    /// PostgreSQL, may treat this syntax as a bit string literal instead, i.e: b'10010101')
    SingleQuotedByteStringLiteral(String),
    /// Byte string literal: i.e: b"string" or B"string"
    DoubleQuotedByteStringLiteral(String),
    /// Triple single quoted literal with byte string prefix. Example `B'''abc'''`
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    TripleSingleQuotedByteStringLiteral(String),
    /// Triple double quoted literal with byte string prefix. Example `B"""abc"""`
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    TripleDoubleQuotedByteStringLiteral(String),
    /// Single quoted literal with raw string prefix. Example `R'abc'`
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    SingleQuotedRawStringLiteral(String),
    /// Double quoted literal with raw string prefix. Example `R"abc"`
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    DoubleQuotedRawStringLiteral(String),
    /// Triple single quoted literal with raw string prefix. Example `R'''abc'''`
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    TripleSingleQuotedRawStringLiteral(String),
    /// Triple double quoted literal with raw string prefix. Example `R"""abc"""`
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    TripleDoubleQuotedRawStringLiteral(String),
    /// "National" string literal: i.e: N'string'
    NationalStringLiteral(String),
    /// Quote delimited literal. Examples `Q'{ab'c}'`, `Q'|ab'c|'`, `Q'|ab|c|'`
    /// [Oracle](https://docs.oracle.com/en/database/oracle/oracle-database/21/sqlrf/Literals.html#GUID-1824CBAA-6E16-4921-B2A6-112FB02248DA)
    QuoteDelimitedStringLiteral(QuoteDelimitedString),
    /// "Nationa" quote delimited literal. Examples `NQ'{ab'c}'`, `NQ'|ab'c|'`, `NQ'|ab|c|'`
    /// [Oracle](https://docs.oracle.com/en/database/oracle/oracle-database/21/sqlrf/Literals.html#GUID-1824CBAA-6E16-4921-B2A6-112FB02248DA)
    NationalQuoteDelimitedStringLiteral(QuoteDelimitedString),
    /// "escaped" string literal, which are an extension to the SQL standard: i.e: e'first \n second' or E 'first \n second'
    EscapedStringLiteral(String),
    /// Unicode string literal: i.e: U&'first \000A second'
    UnicodeStringLiteral(String),
    /// Hexadecimal string literal: i.e.: X'deadbeef'
    HexStringLiteral(String),
    /// MySQL's actual lexer retains the grammar distinction required by AS.
    /// Digits remain the existing canonical hexadecimal value; raw spelling
    /// and source authority belong only to the admitted token occurrence.
    MySqlHexStringLiteral(String, MySqlHexLiteralKind),
    /// Comma
    Comma,
    /// Whitespace (space, tab, etc)
    Whitespace(Whitespace),
    /// Double equals sign `==`
    DoubleEq,
    /// Equality operator `=`
    Eq,
    /// Not Equals operator `<>` (or `!=` in some dialects)
    Neq,
    /// Less Than operator `<`
    Lt,
    /// Greater Than operator `>`
    Gt,
    /// Less Than Or Equals operator `<=`
    LtEq,
    /// Greater Than Or Equals operator `>=`
    GtEq,
    /// Spaceship operator <=>
    Spaceship,
    /// Plus operator `+`
    Plus,
    /// Minus operator `-`
    Minus,
    /// Multiplication operator `*`
    Mul,
    /// Division operator `/`
    Div,
    /// Integer division operator `//` in DuckDB
    DuckIntDiv,
    /// Modulo Operator `%`
    Mod,
    /// String concatenation `||`
    StringConcat,
    /// Left parenthesis `(`
    LParen,
    /// Right parenthesis `)`
    RParen,
    /// Period (used for compound identifiers or projections into nested types)
    Period,
    /// Colon `:`
    Colon,
    /// DoubleColon `::` (used for casting in PostgreSQL)
    DoubleColon,
    /// Assignment `:=` (used for keyword argument in DuckDB macros and some functions, and for variable declarations in DuckDB and Snowflake)
    Assignment,
    /// SemiColon `;` used as separator for COPY and payload
    SemiColon,
    /// Backslash `\` used in terminating the COPY payload with `\.`
    Backslash,
    /// Left bracket `[`
    LBracket,
    /// Right bracket `]`
    RBracket,
    /// Ampersand `&`
    Ampersand,
    /// Pipe `|`
    Pipe,
    /// Caret `^`
    Caret,
    /// Left brace `{`
    LBrace,
    /// Right brace `}`
    RBrace,
    /// Right Arrow `=>`
    RArrow,
    /// Sharp `#` used for PostgreSQL Bitwise XOR operator, also PostgreSQL/Redshift geometrical unary/binary operator (Number of points in path or polygon/Intersection)
    Sharp,
    /// `##` PostgreSQL/Redshift geometrical binary operator (Point of closest proximity)
    DoubleSharp,
    /// Tilde `~` used for PostgreSQL Bitwise NOT operator or case sensitive match regular expression operator
    Tilde,
    /// `~*` , a case insensitive match regular expression operator in PostgreSQL
    TildeAsterisk,
    /// `!~` , a case sensitive not match regular expression operator in PostgreSQL
    ExclamationMarkTilde,
    /// `!~*` , a case insensitive not match regular expression operator in PostgreSQL
    ExclamationMarkTildeAsterisk,
    /// `~~`, a case sensitive match pattern operator in PostgreSQL
    DoubleTilde,
    /// `~~*`, a case insensitive match pattern operator in PostgreSQL
    DoubleTildeAsterisk,
    /// `!~~`, a case sensitive not match pattern operator in PostgreSQL
    ExclamationMarkDoubleTilde,
    /// `!~~*`, a case insensitive not match pattern operator in PostgreSQL
    ExclamationMarkDoubleTildeAsterisk,
    /// `<<`, a bitwise shift left operator in PostgreSQL
    ShiftLeft,
    /// `>>`, a bitwise shift right operator in PostgreSQL
    ShiftRight,
    /// `&&`, an overlap operator in PostgreSQL
    Overlap,
    /// Exclamation Mark `!` used for PostgreSQL factorial operator
    ExclamationMark,
    /// Double Exclamation Mark `!!` used for PostgreSQL prefix factorial operator
    DoubleExclamationMark,
    /// AtSign `@` used for PostgreSQL abs operator, also PostgreSQL/Redshift geometrical unary/binary operator (Center, Contained or on)
    AtSign,
    /// `^@`, a "starts with" string operator in PostgreSQL
    CaretAt,
    /// `|/`, a square root math operator in PostgreSQL
    PGSquareRoot,
    /// `||/`, a cube root math operator in PostgreSQL
    PGCubeRoot,
    /// `?` or `$` , a prepared statement arg placeholder
    Placeholder(String),
    /// `->`, used as a operator to extract json field in PostgreSQL
    Arrow,
    /// `->>`, used as a operator to extract json field as text in PostgreSQL
    LongArrow,
    /// `#>`, extracts JSON sub-object at the specified path
    HashArrow,
    /// `@-@` PostgreSQL/Redshift geometrical unary operator (Length or circumference)
    AtDashAt,
    /// `?-` PostgreSQL/Redshift geometrical unary/binary operator (Is horizontal?/Are horizontally aligned?)
    QuestionMarkDash,
    /// `&<` PostgreSQL/Redshift geometrical binary operator (Overlaps to left?)
    AmpersandLeftAngleBracket,
    /// `&>` PostgreSQL/Redshift geometrical binary operator (Overlaps to right?)`
    AmpersandRightAngleBracket,
    /// `&<|` PostgreSQL/Redshift geometrical binary operator (Does not extend above?)`
    AmpersandLeftAngleBracketVerticalBar,
    /// `|&>` PostgreSQL/Redshift geometrical binary operator (Does not extend below?)`
    VerticalBarAmpersandRightAngleBracket,
    /// `<->` PostgreSQL/Redshift geometrical binary operator (Distance between)
    TwoWayArrow,
    /// `<^` PostgreSQL/Redshift geometrical binary operator (Is below?)
    LeftAngleBracketCaret,
    /// `>^` PostgreSQL/Redshift geometrical binary operator (Is above?)
    RightAngleBracketCaret,
    /// `?#` PostgreSQL/Redshift geometrical binary operator (Intersects or overlaps)
    QuestionMarkSharp,
    /// `?-|` PostgreSQL/Redshift geometrical binary operator (Is perpendicular?)
    QuestionMarkDashVerticalBar,
    /// `?||` PostgreSQL/Redshift geometrical binary operator (Are parallel?)
    QuestionMarkDoubleVerticalBar,
    /// `~=` PostgreSQL/Redshift geometrical binary operator (Same as)
    TildeEqual,
    /// `<<| PostgreSQL/Redshift geometrical binary operator (Is strictly below?)
    ShiftLeftVerticalBar,
    /// `|>> PostgreSQL/Redshift geometrical binary operator (Is strictly above?)
    VerticalBarShiftRight,
    /// `|> BigQuery pipe operator
    VerticalBarRightAngleBracket,
    /// `#>>`, extracts JSON sub-object at the specified path as text
    HashLongArrow,
    /// jsonb @> jsonb -> boolean: Test whether left json contains the right json
    AtArrow,
    /// jsonb <@ jsonb -> boolean: Test whether right json contains the left json
    ArrowAt,
    /// jsonb #- text[] -> jsonb: Deletes the field or array element at the specified
    /// path, where path elements can be either field keys or array indexes.
    HashMinus,
    /// jsonb @? jsonpath -> boolean: Does JSON path return any item for the specified
    /// JSON value?
    AtQuestion,
    /// jsonb @@ jsonpath → boolean: Returns the result of a JSON path predicate check
    /// for the specified JSON value. Only the first item of the result is taken into
    /// account. If the result is not Boolean, then NULL is returned.
    AtAt,
    /// jsonb ? text -> boolean: Checks whether the string exists as a top-level key within the
    /// jsonb object
    Question,
    /// jsonb ?& text[] -> boolean: Check whether all members of the text array exist as top-level
    /// keys within the jsonb object
    QuestionAnd,
    /// jsonb ?| text[] -> boolean: Check whether any member of the text array exists as top-level
    /// keys within the jsonb object
    QuestionPipe,
    /// Custom binary operator
    /// This is used to represent any custom binary operator that is not part of the SQL standard.
    /// PostgreSQL allows defining custom binary operators using CREATE OPERATOR.
    CustomBinaryOperator(String),
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            // Diagnostic description, never a replacement SQL source.
            Token::MySqlText(_) => f.write_str("MySQL character literal"),
            Token::EOF => f.write_str("EOF"),
            Token::Word(ref w) => write!(f, "{w}"),
            Token::Number(ref n, l) => write!(f, "{}{long}", n, long = if *l { "L" } else { "" }),
            Token::Char(ref c) => write!(f, "{c}"),
            Token::SingleQuotedString(ref s) => write!(f, "'{s}'"),
            Token::TripleSingleQuotedString(ref s) => write!(f, "'''{s}'''"),
            Token::DoubleQuotedString(ref s) => write!(f, "\"{s}\""),
            Token::TripleDoubleQuotedString(ref s) => write!(f, "\"\"\"{s}\"\"\""),
            Token::DollarQuotedString(ref s) => write!(f, "{s}"),
            Token::NationalStringLiteral(ref s) => write!(f, "N'{s}'"),
            Token::QuoteDelimitedStringLiteral(ref s) => s.fmt(f),
            Token::NationalQuoteDelimitedStringLiteral(ref s) => write!(f, "N{s}"),
            Token::EscapedStringLiteral(ref s) => write!(f, "E'{s}'"),
            Token::UnicodeStringLiteral(ref s) => write!(f, "U&'{s}'"),
            Token::HexStringLiteral(ref s) => write!(f, "X'{s}'"),
            Token::MySqlHexStringLiteral(s, MySqlHexLiteralKind::HexNumber) => write!(f, "0x{s}"),
            Token::MySqlHexStringLiteral(s, _) => write!(f, "X'{s}'"),
            Token::SingleQuotedByteStringLiteral(ref s) => write!(f, "B'{s}'"),
            Token::TripleSingleQuotedByteStringLiteral(ref s) => write!(f, "B'''{s}'''"),
            Token::DoubleQuotedByteStringLiteral(ref s) => write!(f, "B\"{s}\""),
            Token::TripleDoubleQuotedByteStringLiteral(ref s) => write!(f, "B\"\"\"{s}\"\"\""),
            Token::SingleQuotedRawStringLiteral(ref s) => write!(f, "R'{s}'"),
            Token::DoubleQuotedRawStringLiteral(ref s) => write!(f, "R\"{s}\""),
            Token::TripleSingleQuotedRawStringLiteral(ref s) => write!(f, "R'''{s}'''"),
            Token::TripleDoubleQuotedRawStringLiteral(ref s) => write!(f, "R\"\"\"{s}\"\"\""),
            Token::Comma => f.write_str(","),
            Token::Whitespace(ws) => write!(f, "{ws}"),
            Token::DoubleEq => f.write_str("=="),
            Token::Spaceship => f.write_str("<=>"),
            Token::Eq => f.write_str("="),
            Token::Neq => f.write_str("<>"),
            Token::Lt => f.write_str("<"),
            Token::Gt => f.write_str(">"),
            Token::LtEq => f.write_str("<="),
            Token::GtEq => f.write_str(">="),
            Token::Plus => f.write_str("+"),
            Token::Minus => f.write_str("-"),
            Token::Mul => f.write_str("*"),
            Token::Div => f.write_str("/"),
            Token::DuckIntDiv => f.write_str("//"),
            Token::StringConcat => f.write_str("||"),
            Token::Mod => f.write_str("%"),
            Token::LParen => f.write_str("("),
            Token::RParen => f.write_str(")"),
            Token::Period => f.write_str("."),
            Token::Colon => f.write_str(":"),
            Token::DoubleColon => f.write_str("::"),
            Token::Assignment => f.write_str(":="),
            Token::SemiColon => f.write_str(";"),
            Token::Backslash => f.write_str("\\"),
            Token::LBracket => f.write_str("["),
            Token::RBracket => f.write_str("]"),
            Token::Ampersand => f.write_str("&"),
            Token::Caret => f.write_str("^"),
            Token::Pipe => f.write_str("|"),
            Token::LBrace => f.write_str("{"),
            Token::RBrace => f.write_str("}"),
            Token::RArrow => f.write_str("=>"),
            Token::Sharp => f.write_str("#"),
            Token::DoubleSharp => f.write_str("##"),
            Token::ExclamationMark => f.write_str("!"),
            Token::DoubleExclamationMark => f.write_str("!!"),
            Token::Tilde => f.write_str("~"),
            Token::TildeAsterisk => f.write_str("~*"),
            Token::ExclamationMarkTilde => f.write_str("!~"),
            Token::ExclamationMarkTildeAsterisk => f.write_str("!~*"),
            Token::DoubleTilde => f.write_str("~~"),
            Token::DoubleTildeAsterisk => f.write_str("~~*"),
            Token::ExclamationMarkDoubleTilde => f.write_str("!~~"),
            Token::ExclamationMarkDoubleTildeAsterisk => f.write_str("!~~*"),
            Token::AtSign => f.write_str("@"),
            Token::CaretAt => f.write_str("^@"),
            Token::ShiftLeft => f.write_str("<<"),
            Token::ShiftRight => f.write_str(">>"),
            Token::Overlap => f.write_str("&&"),
            Token::PGSquareRoot => f.write_str("|/"),
            Token::PGCubeRoot => f.write_str("||/"),
            Token::AtDashAt => f.write_str("@-@"),
            Token::QuestionMarkDash => f.write_str("?-"),
            Token::AmpersandLeftAngleBracket => f.write_str("&<"),
            Token::AmpersandRightAngleBracket => f.write_str("&>"),
            Token::AmpersandLeftAngleBracketVerticalBar => f.write_str("&<|"),
            Token::VerticalBarAmpersandRightAngleBracket => f.write_str("|&>"),
            Token::VerticalBarRightAngleBracket => f.write_str("|>"),
            Token::TwoWayArrow => f.write_str("<->"),
            Token::LeftAngleBracketCaret => f.write_str("<^"),
            Token::RightAngleBracketCaret => f.write_str(">^"),
            Token::QuestionMarkSharp => f.write_str("?#"),
            Token::QuestionMarkDashVerticalBar => f.write_str("?-|"),
            Token::QuestionMarkDoubleVerticalBar => f.write_str("?||"),
            Token::TildeEqual => f.write_str("~="),
            Token::ShiftLeftVerticalBar => f.write_str("<<|"),
            Token::VerticalBarShiftRight => f.write_str("|>>"),
            Token::Placeholder(ref s) => write!(f, "{s}"),
            Token::Arrow => write!(f, "->"),
            Token::LongArrow => write!(f, "->>"),
            Token::HashArrow => write!(f, "#>"),
            Token::HashLongArrow => write!(f, "#>>"),
            Token::AtArrow => write!(f, "@>"),
            Token::ArrowAt => write!(f, "<@"),
            Token::HashMinus => write!(f, "#-"),
            Token::AtQuestion => write!(f, "@?"),
            Token::AtAt => write!(f, "@@"),
            Token::Question => write!(f, "?"),
            Token::QuestionAnd => write!(f, "?&"),
            Token::QuestionPipe => write!(f, "?|"),
            Token::CustomBinaryOperator(s) => f.write_str(s),
        }
    }
}

impl Token {
    /// Create a `Token::Word` from an unquoted `keyword`.
    ///
    /// The lookup is case-insensitive; unknown values become `Keyword::NoKeyword`.
    pub fn make_keyword(keyword: &str) -> Self {
        Token::make_word(keyword, None)
    }

    /// Create a `Token::Word` from `word` with an optional `quote_style`.
    ///
    /// When `quote_style` is `None`, the parser attempts a case-insensitive keyword
    /// lookup and sets the `Word::keyword` accordingly.
    pub fn make_word(word: &str, quote_style: Option<char>) -> Self {
        Token::Word(Word {
            keyword: keyword_lookup(word, quote_style),
            value: word.to_string(),
            quote_style,
        })
    }

    /// Like [`Self::make_word`] but takes ownership of the word `String`,
    /// avoiding an extra allocation when the caller already has an owned value.
    fn make_word_owned(word: String, quote_style: Option<char>) -> Self {
        Token::Word(Word {
            keyword: keyword_lookup(&word, quote_style),
            value: word,
            quote_style,
        })
    }
}

/// Case-insensitive keyword lookup using binary search over [`ALL_KEYWORDS`].
fn keyword_lookup(word: &str, quote_style: Option<char>) -> Keyword {
    if quote_style.is_some() {
        return Keyword::NoKeyword;
    }
    ALL_KEYWORDS
        .binary_search_by(|probe| {
            let probe = probe.as_bytes();
            let word = word.as_bytes();
            for (p, w) in probe.iter().zip(word.iter()) {
                let cmp = p.cmp(&w.to_ascii_uppercase());
                if cmp != core::cmp::Ordering::Equal {
                    return cmp;
                }
            }
            probe.len().cmp(&word.len())
        })
        .map_or(Keyword::NoKeyword, |x| ALL_KEYWORDS_INDEX[x])
}

/// A keyword (like SELECT) or an optionally quoted SQL identifier
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub struct Word {
    /// The value of the token, without the enclosing quotes, and with the
    /// escape sequences (if any) processed (TODO: escapes are not handled)
    pub value: String,
    /// An identifier can be "quoted" (&lt;delimited identifier> in ANSI parlance).
    /// The standard and most implementations allow using double quotes for this,
    /// but some implementations support other quoting styles as well (e.g. \[MS SQL])
    pub quote_style: Option<char>,
    /// If the word was not quoted and it matched one of the known keywords,
    /// this will have one of the values from dialect::keywords, otherwise empty
    pub keyword: Keyword,
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.quote_style {
            Some(s) if s == '"' || s == '[' || s == '`' => {
                write!(f, "{}{}{}", s, self.value, Word::matching_end_quote(s))
            }
            None => f.write_str(&self.value),
            _ => panic!("Unexpected quote_style!"),
        }
    }
}

impl Word {
    fn matching_end_quote(ch: char) -> char {
        match ch {
            '"' => '"', // ANSI and most dialects
            '[' => ']', // MS SQL
            '`' => '`', // MySQL
            _ => panic!("unexpected quoting style!"),
        }
    }
}

/// Represents whitespace in the input: spaces, newlines, tabs and comments.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub enum Whitespace {
    /// Admitted original comment bytes, including their real delimiters.
    MySqlComment(SqlSourceOccurrence),
    /// A single space character.
    Space,
    /// A newline character.
    Newline,
    /// A tab character.
    Tab,
    /// A single-line comment (e.g. `-- comment` or `# comment`).
    /// The `comment` field contains the text, and `prefix` contains the comment prefix.
    SingleLineComment {
        /// The content of the comment (without the prefix).
        comment: String,
        /// The prefix used for the comment (for example `--` or `#`).
        prefix: String,
    },

    /// A multi-line comment (without the `/* ... */` delimiters).
    MultiLineComment(String),
}

impl fmt::Display for Whitespace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Whitespace::MySqlComment(_) => f.write_str("MySQL source comment"),
            Whitespace::Space => f.write_str(" "),
            Whitespace::Newline => f.write_str("\n"),
            Whitespace::Tab => f.write_str("\t"),
            Whitespace::SingleLineComment { prefix, comment } => write!(f, "{prefix}{comment}"),
            Whitespace::MultiLineComment(s) => write!(f, "/*{s}*/"),
        }
    }
}

/// Location in input string
///
/// # Create an "empty" (unknown) `Location`
/// ```
/// # use sqlparser::tokenizer::Location;
/// let location = Location::empty();
/// ```
///
/// # Create a `Location` from a line and column
/// ```
/// # use sqlparser::tokenizer::Location;
/// let location = Location::new(1, 1);
/// ```
///
/// # Create a `Location` from a pair
/// ```
/// # use sqlparser::tokenizer::Location;
/// let location = Location::from((1, 1));
/// ```
#[derive(Eq, PartialEq, Hash, Clone, Copy, Ord, PartialOrd)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub struct Location {
    /// Line number, starting from 1.
    ///
    /// Note: Line 0 is used for empty spans
    pub line: u64,
    /// Line column, starting from 1.
    ///
    /// Note: Column 0 is used for empty spans
    pub column: u64,
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.line == 0 {
            return Ok(());
        }
        write!(f, " at Line: {}, Column: {}", self.line, self.column)
    }
}

impl fmt::Debug for Location {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Location({},{})", self.line, self.column)
    }
}

impl Location {
    /// Return an "empty" / unknown location
    pub fn empty() -> Self {
        Self { line: 0, column: 0 }
    }

    /// Create a new `Location` for a given line and column
    pub fn new(line: u64, column: u64) -> Self {
        Self { line, column }
    }

    /// Create a new location for a given line and column
    ///
    /// Alias for [`Self::new`]
    // TODO: remove / deprecate in favor of` `new` for consistency?
    pub fn of(line: u64, column: u64) -> Self {
        Self::new(line, column)
    }

    /// Combine self and `end` into a new `Span`
    pub fn span_to(self, end: Self) -> Span {
        Span { start: self, end }
    }
}

impl From<(u64, u64)> for Location {
    fn from((line, column): (u64, u64)) -> Self {
        Self { line, column }
    }
}

/// A span represents a linear portion of the input string (start, end)
///
/// See [Spanned](crate::ast::Spanned) for more information.
#[derive(Eq, PartialEq, Hash, Clone, PartialOrd, Ord, Copy)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub struct Span {
    /// Start `Location` (inclusive).
    pub start: Location,
    /// End `Location` (inclusive).
    pub end: Location,
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Span({:?}..{:?})", self.start, self.end)
    }
}

impl Span {
    // An empty span (0, 0) -> (0, 0)
    // We need a const instance for pattern matching
    const EMPTY: Span = Self::empty();

    /// Create a new span from a start and end [`Location`]
    pub fn new(start: Location, end: Location) -> Span {
        Span { start, end }
    }

    /// Returns an empty span `(0, 0) -> (0, 0)`
    ///
    /// Empty spans represent no knowledge of source location
    /// See [Spanned](crate::ast::Spanned) for more information.
    pub const fn empty() -> Span {
        Span {
            start: Location { line: 0, column: 0 },
            end: Location { line: 0, column: 0 },
        }
    }

    /// Returns the smallest Span that contains both `self` and `other`
    /// If either span is [Span::empty], the other span is returned
    ///
    /// # Examples
    /// ```
    /// # use sqlparser::tokenizer::{Span, Location};
    /// // line 1, column1 -> line 2, column 5
    /// let span1 = Span::new(Location::new(1, 1), Location::new(2, 5));
    /// // line 2, column 3 -> line 3, column 7
    /// let span2 = Span::new(Location::new(2, 3), Location::new(3, 7));
    /// // Union of the two is the min/max of the two spans
    /// // line 1, column 1 -> line 3, column 7
    /// let union = span1.union(&span2);
    /// assert_eq!(union, Span::new(Location::new(1, 1), Location::new(3, 7)));
    /// ```
    pub fn union(&self, other: &Span) -> Span {
        // If either span is empty, return the other
        // this prevents propagating (0, 0) through the tree
        match (self, other) {
            (&Span::EMPTY, _) => *other,
            (_, &Span::EMPTY) => *self,
            _ => Span {
                start: cmp::min(self.start, other.start),
                end: cmp::max(self.end, other.end),
            },
        }
    }

    /// Same as [Span::union] for `Option<Span>`
    ///
    /// If `other` is `None`, `self` is returned
    pub fn union_opt(&self, other: &Option<Span>) -> Span {
        match other {
            Some(other) => self.union(other),
            None => *self,
        }
    }

    /// Return the [Span::union] of all spans in the iterator
    ///
    /// If the iterator is empty, an empty span is returned
    ///
    /// # Example
    /// ```
    /// # use sqlparser::tokenizer::{Span, Location};
    /// let spans = vec![
    ///     Span::new(Location::new(1, 1), Location::new(2, 5)),
    ///     Span::new(Location::new(2, 3), Location::new(3, 7)),
    ///     Span::new(Location::new(3, 1), Location::new(4, 2)),
    /// ];
    /// // line 1, column 1 -> line 4, column 2
    /// assert_eq!(
    ///   Span::union_iter(spans),
    ///   Span::new(Location::new(1, 1), Location::new(4, 2))
    /// );
    pub fn union_iter<I: IntoIterator<Item = Span>>(iter: I) -> Span {
        iter.into_iter()
            .reduce(|acc, item| acc.union(&item))
            .unwrap_or(Span::empty())
    }
}

/// Backwards compatibility struct for [`TokenWithSpan`]
#[deprecated(since = "0.53.0", note = "please use `TokenWithSpan` instead")]
pub type TokenWithLocation = TokenWithSpan;

/// A [Token] with [Span] attached to it
///
/// This is used to track the location of a token in the input string
///
/// # Examples
/// ```
/// # use sqlparser::tokenizer::{Location, Span, Token, TokenWithSpan};
/// // commas @ line 1, column 10
/// let tok1 = TokenWithSpan::new(
///   Token::Comma,
///   Span::new(Location::new(1, 10), Location::new(1, 11)),
/// );
/// assert_eq!(tok1, Token::Comma); // can compare the token
///
/// // commas @ line 2, column 20
/// let tok2 = TokenWithSpan::new(
///   Token::Comma,
///   Span::new(Location::new(2, 20), Location::new(2, 21)),
/// );
/// // same token but different locations are not equal
/// assert_ne!(tok1, tok2);
/// ```
#[derive(Debug, Clone, Hash, Ord, PartialOrd, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
/// A `Token` together with its `Span` (location in the source).
pub struct TokenWithSpan {
    /// The token value.
    pub token: Token,
    /// The span covering the token in the input.
    pub span: Span,
}

impl TokenWithSpan {
    /// Create a new [`TokenWithSpan`] from a [`Token`] and a [`Span`]
    pub fn new(token: Token, span: Span) -> Self {
        Self { token, span }
    }

    /// Wrap a token with an empty span
    pub fn wrap(token: Token) -> Self {
        Self::new(token, Span::empty())
    }

    /// Wrap a token with a location from `start` to `end`
    pub fn at(token: Token, start: Location, end: Location) -> Self {
        Self::new(token, Span::new(start, end))
    }

    /// Return an EOF token with no location
    pub fn new_eof() -> Self {
        Self::wrap(Token::EOF)
    }
}

impl PartialEq<Token> for TokenWithSpan {
    fn eq(&self, other: &Token) -> bool {
        &self.token == other
    }
}

impl PartialEq<TokenWithSpan> for Token {
    fn eq(&self, other: &TokenWithSpan) -> bool {
        self == &other.token
    }
}

impl fmt::Display for TokenWithSpan {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.token.fmt(f)
    }
}

/// An error reported by the tokenizer, with a human-readable `message` and a `location`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenizerError {
    /// A descriptive error message.
    pub message: String,
    /// The `Location` where the error was detected.
    pub location: Location,
    /// Actual source producer failure; ordinary lexical diagnostics use Message.
    pub source_kind: MySqlSourceErrorKind,
}

/// A source service failure retains its real reason without inspecting its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MySqlSourceErrorKind {
    /// Ordinary representation or lexical diagnostic.
    Message,
    /// A caller's actual allocation budget refused admission.
    ResourceExhausted,
    /// The actual cancellation authority fired.
    Cancelled,
    /// The original armed deadline expired.
    DeadlineExceeded,
    /// Actual json_attribute syntax failure, before DDL contextualization.
    InvalidJsonAttribute,
    /// Actual native depth failure; the first status is3157, not3980.
    JsonAttributeDepth,
    /// Actual cross-charset TEXT_STRING_validated conversion failure3854.
    CharacterConversionFailed,
}

/// The one error value carried from native source services to the parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MySqlSourceError {
    /// Original producer message, not a classified or reparsed spelling.
    pub message: String,
    /// Original producer reason.
    pub kind: MySqlSourceErrorKind,
}
impl From<String> for MySqlSourceError {
    fn from(message: String) -> Self {
        Self {
            message,
            kind: MySqlSourceErrorKind::Message,
        }
    }
}
impl From<&str> for MySqlSourceError {
    fn from(message: &str) -> Self {
        String::from(message).into()
    }
}
impl fmt::Display for MySqlSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl core::error::Error for MySqlSourceError {}
impl TokenizerError {
    fn source(error: MySqlSourceError, location: Location) -> Self {
        Self {
            message: error.message,
            source_kind: error.kind,
            location,
        }
    }
}

/// Stable marker used when a cooperative parser cancellation interrupts lexing.
pub const QUERY_CANCELLED_MESSAGE: &str = "query parsing was cancelled";

/// A cheap cooperative cancellation probe shared by the tokenizer and parser.
pub type CancellationCheck = Arc<dyn Fn() -> bool + Send + Sync>;

impl fmt::Display for TokenizerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.message, self.location,)
    }
}

impl core::error::Error for TokenizerError {}

struct State<'a> {
    raw: Option<MySqlByteState<'a>>,
    peekable: Peekable<Chars<'a>>,
    line: u64,
    col: u64,
    cancellation: Option<CancellationCheck>,
    characters_since_cancellation_check: usize,
    cancelled: bool,
}

impl<'a> State<'a> {
    /// return the next character and advance the stream
    pub fn next(&mut self) -> Option<char> {
        if self.cancelled {
            return None;
        }
        if self
            .raw
            .as_ref()
            .is_some_and(|raw| raw.error.borrow().is_some())
        {
            return None;
        }
        if self.characters_since_cancellation_check >= 1024 {
            self.characters_since_cancellation_check = 0;
            if self.cancellation.as_ref().is_some_and(|check| check()) {
                self.cancelled = true;
                if let Some(raw) = &self.raw {
                    raw.failure(MySqlSourceError {
                        message: QUERY_CANCELLED_MESSAGE.into(),
                        kind: MySqlSourceErrorKind::Cancelled,
                    });
                }
                return None;
            }
        }
        let next = if let Some(raw) = &mut self.raw {
            let ch = raw.ascii_at(0).ok().flatten()?;
            if let Err(message) = raw.advance_bytes(1) {
                raw.failure(message.into());
                return None;
            }
            Some(ch)
        } else {
            self.peekable.next()
        };
        match next {
            None => None,
            Some(s) => {
                self.characters_since_cancellation_check += 1;
                if s == '\n' {
                    self.line += 1;
                    self.col = 1;
                } else {
                    self.col += 1;
                }
                Some(s)
            }
        }
    }

    /// return the next character but do not advance the stream
    pub fn peek(&mut self) -> Option<&char> {
        if self.cancelled {
            return None;
        }
        if self
            .raw
            .as_ref()
            .is_some_and(|raw| raw.error.borrow().is_some())
        {
            return None;
        }
        if let Some(raw) = &mut self.raw {
            raw.peeked_ascii = raw.ascii_at(0).ok().flatten();
            raw.peeked_ascii.as_ref()
        } else {
            self.peekable.peek()
        }
    }

    fn peek_nth(&self, nth: usize) -> Option<char> {
        self.raw.as_ref().map_or_else(
            || self.peekable.clone().nth(nth),
            |raw| raw.ascii_at(nth).ok().flatten(),
        )
    }

    fn ascii_lookahead(&self) -> StateLookahead<'_, 'a> {
        match &self.raw {
            Some(raw) => StateLookahead::Bytes {
                raw,
                offset: raw.offset,
                done: false,
            },
            None => StateLookahead::Unicode(self.peekable.clone()),
        }
    }

    /// Return the current `Location` (line and column)
    pub fn location(&self) -> Location {
        if let Some(raw) = &self.raw {
            return raw.location;
        }
        Location {
            line: self.line,
            column: self.col,
        }
    }
}

/// Native facts supplied by the caller. The parser never recreates ctype,
/// Unicode categories, MB width or validity from Rust character predicates.
#[derive(Clone, Copy, Debug)]
pub struct MySqlLexicalUnit {
    /// Actual source bytes consumed by this native unit, bounded by the input.
    pub width: u8,
    /// Native expected character width, retained separately for incomplete or
    /// malformed input; it is not a Rust UTF-8 width estimate.
    pub expected_width: u8,
    /// Native initialized main-map lexical state for the unit's first byte.
    /// It is a native fact, not a second tokenizer's token classification.
    pub state: u8,
    /// Whether the native charset recognizes this complete unit as valid.
    pub valid: bool,
    /// Whether the unit is one ASCII syntax byte, not a multibyte trail byte.
    pub ascii: bool,
    /// Native charset whitespace classification of the first byte.
    pub space: bool,
    /// Native control classification, kept distinct from whitespace.
    pub control: bool,
    /// Native identifier-map fact; this does not grant name or compiler authority.
    pub identifier: bool,
}

/// Caller-owned native charset/conversion and allocation services for the
/// existing tokenizer's byte input. The adapter must capture the current source
/// context, preserve original malformed bytes, and report real failures instead
/// of repairing them with Unicode replacement or treating them as EOF.
/// Budget guards and conversion diagnostics belong to the caller's actual owners;
/// this trait neither creates a second lexer nor authorizes SQL execution.
/// Native keyword identity issued only by the actual hint consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MySqlHintKeyword {
    /// Fixed native BKA hint keyword.
    Bka,
    /// Fixed native BNL hint keyword.
    Bnl,
    /// Fixed native DUPSWEEDOUT hint keyword.
    Dupsweedout,
    /// Fixed native FIRSTMATCH hint keyword.
    Firstmatch,
    /// Fixed native INTOEXISTS hint keyword.
    Intoexists,
    /// Fixed native LOOSESCAN hint keyword.
    Loosescan,
    /// Fixed native MATERIALIZATION hint keyword.
    Materialization,
    /// Fixed native MAX_EXECUTION_TIME hint keyword.
    MaxExecutionTime,
    /// Fixed native NO_BKA hint keyword.
    NoBka,
    /// Fixed native NO_BNL hint keyword.
    NoBnl,
    /// Fixed native NO_ICP hint keyword.
    NoIcp,
    /// Fixed native NO_MRR hint keyword.
    NoMrr,
    /// Fixed native NO_RANGE_OPTIMIZATION hint keyword.
    NoRangeOptimization,
    /// Fixed native NO_SEMIJOIN hint keyword.
    NoSemijoin,
    /// Fixed native MRR hint keyword.
    Mrr,
    /// Fixed native QB_NAME hint keyword.
    QbName,
    /// Fixed native SEMIJOIN hint keyword.
    Semijoin,
    /// Fixed native SET_VAR hint keyword.
    SetVar,
    /// Fixed native SUBQUERY hint keyword.
    Subquery,
    /// Fixed native MERGE hint keyword.
    Merge,
    /// Fixed native NO_MERGE hint keyword.
    NoMerge,
    /// Fixed native JOIN_PREFIX hint keyword.
    JoinPrefix,
    /// Fixed native JOIN_SUFFIX hint keyword.
    JoinSuffix,
    /// Fixed native JOIN_ORDER hint keyword.
    JoinOrder,
    /// Fixed native JOIN_FIXED_ORDER hint keyword.
    JoinFixedOrder,
    /// Fixed native INDEX_MERGE hint keyword.
    IndexMerge,
    /// Fixed native NO_INDEX_MERGE hint keyword.
    NoIndexMerge,
    /// Fixed native RESOURCE_GROUP hint keyword.
    ResourceGroup,
    /// Fixed native SKIP_SCAN hint keyword.
    SkipScan,
    /// Fixed native NO_SKIP_SCAN hint keyword.
    NoSkipScan,
    /// Fixed native HASH_JOIN hint keyword.
    HashJoin,
    /// Fixed native NO_HASH_JOIN hint keyword.
    NoHashJoin,
    /// Fixed native INDEX hint keyword.
    Index,
    /// Fixed native NO_INDEX hint keyword.
    NoIndex,
    /// Fixed native JOIN_INDEX hint keyword.
    JoinIndex,
    /// Fixed native NO_JOIN_INDEX hint keyword.
    NoJoinIndex,
    /// Fixed native GROUP_INDEX hint keyword.
    GroupIndex,
    /// Fixed native NO_GROUP_INDEX hint keyword.
    NoGroupIndex,
    /// Fixed native ORDER_INDEX hint keyword.
    OrderIndex,
    /// Fixed native NO_ORDER_INDEX hint keyword.
    NoOrderIndex,
    /// Fixed native DERIVED_CONDITION_PUSHDOWN hint keyword.
    DerivedConditionPushdown,
    /// Fixed native NO_DERIVED_CONDITION_PUSHDOWN hint keyword.
    NoDerivedConditionPushdown,
}
impl MySqlHintKeyword {
    /// Borrow fixed native keyword metadata; this alone grants no Source role.
    pub fn from_word(word: &str) -> Option<Self> {
        if word.eq_ignore_ascii_case("BKA") {
            return Some(Self::Bka);
        }
        if word.eq_ignore_ascii_case("BNL") {
            return Some(Self::Bnl);
        }
        if word.eq_ignore_ascii_case("DUPSWEEDOUT") {
            return Some(Self::Dupsweedout);
        }
        if word.eq_ignore_ascii_case("FIRSTMATCH") {
            return Some(Self::Firstmatch);
        }
        if word.eq_ignore_ascii_case("INTOEXISTS") {
            return Some(Self::Intoexists);
        }
        if word.eq_ignore_ascii_case("LOOSESCAN") {
            return Some(Self::Loosescan);
        }
        if word.eq_ignore_ascii_case("MATERIALIZATION") {
            return Some(Self::Materialization);
        }
        if word.eq_ignore_ascii_case("MAX_EXECUTION_TIME") {
            return Some(Self::MaxExecutionTime);
        }
        if word.eq_ignore_ascii_case("NO_BKA") {
            return Some(Self::NoBka);
        }
        if word.eq_ignore_ascii_case("NO_BNL") {
            return Some(Self::NoBnl);
        }
        if word.eq_ignore_ascii_case("NO_ICP") {
            return Some(Self::NoIcp);
        }
        if word.eq_ignore_ascii_case("NO_MRR") {
            return Some(Self::NoMrr);
        }
        if word.eq_ignore_ascii_case("NO_RANGE_OPTIMIZATION") {
            return Some(Self::NoRangeOptimization);
        }
        if word.eq_ignore_ascii_case("NO_SEMIJOIN") {
            return Some(Self::NoSemijoin);
        }
        if word.eq_ignore_ascii_case("MRR") {
            return Some(Self::Mrr);
        }
        if word.eq_ignore_ascii_case("QB_NAME") {
            return Some(Self::QbName);
        }
        if word.eq_ignore_ascii_case("SEMIJOIN") {
            return Some(Self::Semijoin);
        }
        if word.eq_ignore_ascii_case("SET_VAR") {
            return Some(Self::SetVar);
        }
        if word.eq_ignore_ascii_case("SUBQUERY") {
            return Some(Self::Subquery);
        }
        if word.eq_ignore_ascii_case("MERGE") {
            return Some(Self::Merge);
        }
        if word.eq_ignore_ascii_case("NO_MERGE") {
            return Some(Self::NoMerge);
        }
        if word.eq_ignore_ascii_case("JOIN_PREFIX") {
            return Some(Self::JoinPrefix);
        }
        if word.eq_ignore_ascii_case("JOIN_SUFFIX") {
            return Some(Self::JoinSuffix);
        }
        if word.eq_ignore_ascii_case("JOIN_ORDER") {
            return Some(Self::JoinOrder);
        }
        if word.eq_ignore_ascii_case("JOIN_FIXED_ORDER") {
            return Some(Self::JoinFixedOrder);
        }
        if word.eq_ignore_ascii_case("INDEX_MERGE") {
            return Some(Self::IndexMerge);
        }
        if word.eq_ignore_ascii_case("NO_INDEX_MERGE") {
            return Some(Self::NoIndexMerge);
        }
        if word.eq_ignore_ascii_case("RESOURCE_GROUP") {
            return Some(Self::ResourceGroup);
        }
        if word.eq_ignore_ascii_case("SKIP_SCAN") {
            return Some(Self::SkipScan);
        }
        if word.eq_ignore_ascii_case("NO_SKIP_SCAN") {
            return Some(Self::NoSkipScan);
        }
        if word.eq_ignore_ascii_case("HASH_JOIN") {
            return Some(Self::HashJoin);
        }
        if word.eq_ignore_ascii_case("NO_HASH_JOIN") {
            return Some(Self::NoHashJoin);
        }
        if word.eq_ignore_ascii_case("INDEX") {
            return Some(Self::Index);
        }
        if word.eq_ignore_ascii_case("NO_INDEX") {
            return Some(Self::NoIndex);
        }
        if word.eq_ignore_ascii_case("JOIN_INDEX") {
            return Some(Self::JoinIndex);
        }
        if word.eq_ignore_ascii_case("NO_JOIN_INDEX") {
            return Some(Self::NoJoinIndex);
        }
        if word.eq_ignore_ascii_case("GROUP_INDEX") {
            return Some(Self::GroupIndex);
        }
        if word.eq_ignore_ascii_case("NO_GROUP_INDEX") {
            return Some(Self::NoGroupIndex);
        }
        if word.eq_ignore_ascii_case("ORDER_INDEX") {
            return Some(Self::OrderIndex);
        }
        if word.eq_ignore_ascii_case("NO_ORDER_INDEX") {
            return Some(Self::NoOrderIndex);
        }
        if word.eq_ignore_ascii_case("DERIVED_CONDITION_PUSHDOWN") {
            return Some(Self::DerivedConditionPushdown);
        }
        if word.eq_ignore_ascii_case("NO_DERIVED_CONDITION_PUSHDOWN") {
            return Some(Self::NoDerivedConditionPushdown);
        }
        None
    }
    fn digest_token(self) -> u16 {
        match self {
            Self::Bka => 1002,
            Self::Bnl => 1003,
            Self::Dupsweedout => 1004,
            Self::Firstmatch => 1005,
            Self::Intoexists => 1006,
            Self::Loosescan => 1007,
            Self::Materialization => 1008,
            Self::MaxExecutionTime => 1000,
            Self::NoBka => 1009,
            Self::NoBnl => 1010,
            Self::NoIcp => 1011,
            Self::NoMrr => 1012,
            Self::NoRangeOptimization => 1013,
            Self::NoSemijoin => 1014,
            Self::Mrr => 1015,
            Self::QbName => 1016,
            Self::Semijoin => 1017,
            Self::SetVar => 1027,
            Self::Subquery => 1018,
            Self::Merge => 1019,
            Self::NoMerge => 1020,
            Self::JoinPrefix => 1021,
            Self::JoinSuffix => 1022,
            Self::JoinOrder => 1023,
            Self::JoinFixedOrder => 1024,
            Self::IndexMerge => 1025,
            Self::NoIndexMerge => 1026,
            Self::ResourceGroup => 1001,
            Self::SkipScan => 1028,
            Self::NoSkipScan => 1029,
            Self::HashJoin => 1030,
            Self::NoHashJoin => 1031,
            Self::Index => 1039,
            Self::NoIndex => 1040,
            Self::JoinIndex => 1041,
            Self::NoJoinIndex => 1042,
            Self::GroupIndex => 1043,
            Self::NoGroupIndex => 1044,
            Self::OrderIndex => 1045,
            Self::NoOrderIndex => 1046,
            Self::DerivedConditionPushdown => 1047,
            Self::NoDerivedConditionPushdown => 1048,
        }
    }
}
/// A token kind demanded by the same native hint consumer, never a supplied
/// arbitrary token number. Semantic bytes borrow that token's actual owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MySqlHintTokenKind {
    /// Actual fixed keyword lookup.
    Keyword(MySqlHintKeyword),
    /// Native numeric/scale/float lexical role.
    Number,
    /// Native IDENT; adjacent at-sign is a lexer fact.
    Identifier {
        /// Whether the actual next byte is the adjacent native at-sign.
        adjacent_at: bool,
    },
    /// Native query-block name, including its original at-sign role.
    QueryBlock,
    /// Native quoted text role.
    Text,
    /// One genuinely demanded native single-byte fallback character.
    Symbol(u8),
    /// Original comment's checked closing delimiter.
    Close,
    /// Actual native scanner error; it is not appended to the digest.
    Error,
}

/// The original Source owner's borrowed native conversion/admission services.
/// Defaults supply no Hint issuer and never infer one from retained text.
pub trait MySqlSourceAdapter {
    /// Consume one eligible original comment through the existing hint factory.
    /// The cursor supplies a same-call typed sink; None explicitly means no
    /// genuine service, retaining the original unbound business representation.
    fn consume_optimizer_hint(
        &self,
        _source: &SqlSourceOccurrence,
        _emit: &mut dyn FnMut(MySqlHintTokenKind, &[u8]) -> Result<(), MySqlSourceError>,
    ) -> Result<Option<crate::ast::OptimizerHintStyle>, MySqlSourceError> {
        Ok(None)
    }

    /// The original command owner opts into canonical collection using its
    /// admitted parser limit and actual native client charset number. None
    /// (including disabled instrumentation) allocates no digest payload.
    /// This is observation configuration, not SQL or PSI authority.
    fn canonical_digest_config(&self) -> Option<MySqlDigestConfig> {
        None
    }
    /// Hashes only this cursor's native canonical token bytes, never raw SQL.
    /// A source without the actual hash service remains explicitly unavailable.
    fn canonical_digest_hash(&self, _tokens: &[u8]) -> Option<[u8; 32]> {
        None
    }

    /// Borrow the original native charset registry lookup for an underscore
    /// word. None means this adapter has no such fact, not an unknown charset.
    fn canonical_charset_introducer(&self, _name: &str) -> Option<bool> {
        None
    }
    /// Reports the native JSON_TABLE ON ERROR/ON EMPTY grammar reduction to
    /// the caller's existing condition owner. String/typed parsers do not
    /// manufacture Source diagnostics; adapters without one leave it empty.
    fn json_table_on_error_before_empty(&self) {}
    /// Reports a completed native character-type grammar reduction. Only
    /// the attached caller's existing diagnostic owner publishes conditions.
    fn deprecated_character_syntax(&self, _syntax: &str, _replacement: &str) {}
    /// Reports the actual charset-name reduction or demanded unquoted lexer
    /// introducer. The adapter validates the native charset before warning.
    fn deprecated_character_set(&self, _charset: &str) {}

    /// A completed collation-name reduction, checked by the actual registry.
    fn deprecated_collation(&self, _collation: &str) {}

    /// A completed numeric type/field-option syntax reduction.
    fn deprecated_numeric_syntax(&self, _syntax: &str) {}
    /// Returns the captured native client charset name, not one inferred by
    /// successfully decoding the input as UTF-8.
    fn client_charset(&self) -> &str;
    /// Returns the native minimum byte width. MySQL source admission requires
    /// one; wider connection/result charsets do not make wider client SQL legal.
    fn client_min_width(&self) -> u8;
    /// Returns native facts for the first demanded unit of this borrowed input.
    /// Implementations may batch bounded facts internally but must not latch a
    /// future unit's error before it is demanded or prelex a later-mode tail.
    fn lexical_unit(&self, input: &[u8]) -> Result<MySqlLexicalUnit, MySqlSourceError>;
    /// The native IDENT_sys / TEXT_STRING_sys role conversion, not lossy UTF8.
    fn system_identifier(
        &self,
        bytes: &[u8],
        quoted: bool,
    ) -> Result<(String, SqlSourceAdmission), MySqlSourceError>;
    /// TEXT_STRING_sys conversion is NOT IDENT_sys well-formed validation.
    /// Legacy String-only grammar callers cannot retain malformed system bytes;
    /// adapters must report that representation boundary, never return lossy text.
    fn system_text(
        &self,
        text: &MySqlText,
    ) -> Result<(String, SqlSourceAdmission), MySqlSourceError>;
    /// Borrow the genuine single TEXT_STRING_sys conversion as bytes. A
    /// native grammar receiver retains this same admission until its late
    /// validation; a foreign adapter must not invent System issuance.
    fn system_text_value(
        &self,
        _text: &MySqlText,
    ) -> Result<(Vec<u8>, SqlSourceAdmission), MySqlSourceError> {
        Err("System value requires the actual native Source service".into())
    }
    /// One actual quoted output-alias TEXT_STRING_validated reduction, with
    /// its original admission on success or a native fatal condition.
    fn projection_alias_text(
        &self,
        _text: &MySqlText,
    ) -> Result<(Vec<u8>, SqlSourceAdmission), (MySqlSourceError, Option<SqlSourceAdmission>)> {
        Err((
            "output alias requires the actual validated Source service".into(),
            None,
        ))
    }
    /// The genuine single TEXT_STRING_sys JSON reduction. The original
    /// System carrier retains every byte; a foreign adapter has no issuer.
    fn json_attribute(
        &self,
        _text: &MySqlText,
    ) -> Result<(Vec<u8>, SqlSourceAdmission), (MySqlSourceError, Option<SqlSourceAdmission>)> {
        Err((
            "JSON attribute requires the actual native Source service".into(),
            None,
        ))
    }
    /// Native TEXT_STRING_literal conversion into the captured connection
    /// character domain. Unlike TEXT_STRING_sys, its result is raw bytes and
    /// need not be representable as UTF-8. The actual adapter admits before
    /// conversion/allocation and returns that same lease; neither this method
    /// nor a text role alone grants grammar or execution authority. HEX_NUM and
    /// BIN_NUM retain their distinct token/grammar conversion, not this path.
    /// Adapters without a real connection-domain service explicitly refuse it.
    fn text_string(
        &self,
        _text: &MySqlText,
    ) -> Result<(Vec<u8>, SqlSourceAdmission), MySqlSourceError> {
        Err("TEXT_STRING requires the actual connection-byte source adapter".into())
    }
    /// Acquires an existing-budget lease before allocating the requested byte
    /// capacity. Returned guards must release their charge when the final owner
    /// drops; allocation/header charges are explicit, never just used length.
    fn admit(&self, capacity: usize) -> Result<SqlSourceAdmission, MySqlSourceError>;
    /// Admits an original or transformed source allocation including its owner
    /// header and shared-reference counters. The default checks size overflow
    /// and delegates to [`Self::admit`] before allocation.
    fn admit_source(&self, capacity: usize) -> Result<SqlSourceAdmission, MySqlSourceError> {
        let bytes = capacity
            .checked_add(core::mem::size_of::<SqlSourceOwner>())
            .and_then(|bytes| bytes.checked_add(2 * core::mem::size_of::<usize>()))
            .ok_or_else(|| String::from("source owner admission overflow"))?;
        self.admit(bytes)
    }
}

#[cfg(all(test, feature = "std"))]
#[path = "tests/mod.rs"]
mod mysql_source_tests;

struct MySqlByteState<'a> {
    source: SqlSourceOccurrence,
    adapter: &'a dyn MySqlSourceAdapter,
    offset: usize,
    peeked_ascii: Option<char>,
    in_version_comment: bool,
    has_comment: bool,
    token_start: usize,
    cpp_space: bool,
    allocations: Vec<SqlSourceAdmission>,
    allocations_admission: Option<SqlSourceAdmission>,
    identifier_text: Option<MySqlText>,
    // Issued by the original comment FSM/native space units, never a rescan.
    empty_hint_comment: bool,
    // A version comment can disappear into CPP without an emitted whitespace
    // token. Its original FSM entry still breaks physical keyword lookahead.
    digest_comment_barrier: bool,
    // One first-error ledger. Native service errors gain their original
    // tokenizer location at the same FSM/cursor boundary as before.
    error: core::cell::RefCell<Option<MySqlCursorFailure>>,
    cancellation: Option<CancellationCheck>,
    work: core::cell::Cell<usize>,
    location: Location,
    token_location: Location,
}

// A native adapter has no Location argument. Keep its first payload until
// the existing FSM/cursor attaches the original diagnostic location, then
// retain the complete TokenizerError in this same slot, never a second cache.
enum MySqlCursorFailure {
    Source(MySqlSourceError),
    Tokenizer(TokenizerError),
    Admitted(TokenizerError, SqlSourceAdmission),
}

impl MySqlCursorFailure {
    fn message(&self) -> &str {
        match self {
            Self::Source(error) => &error.message,
            Self::Tokenizer(error) => &error.message,
            Self::Admitted(error, _) => &error.message,
        }
    }
}

enum StateLookahead<'s, 'q> {
    Unicode(Peekable<Chars<'q>>),
    Bytes {
        raw: &'s MySqlByteState<'q>,
        offset: usize,
        done: bool,
    },
}

impl Iterator for StateLookahead<'_, '_> {
    type Item = char;
    fn next(&mut self) -> Option<char> {
        match self {
            Self::Unicode(chars) => chars.next(),
            Self::Bytes { raw, offset, done } => {
                if *done {
                    return None;
                }
                let Some(unit) = raw.unit_at(*offset).ok().flatten() else {
                    *done = true;
                    return None;
                };
                if !unit.ascii || unit.width != 1 {
                    *done = true;
                    return None;
                }
                let value = char::from(raw.bytes()[*offset]);
                *offset += 1;
                Some(value)
            }
        }
    }
}

impl MySqlByteState<'_> {
    fn failure(&self, error: MySqlSourceError) -> String {
        let mut first = self.error.borrow_mut();
        if first.is_none() {
            *first = Some(MySqlCursorFailure::Source(error));
        }
        first.as_ref().expect("first source error").message().into()
    }
    fn record_error(&self, error: TokenizerError) -> TokenizerError {
        let mut first = self.error.borrow_mut();
        let selected = match first.as_ref() {
            Some(MySqlCursorFailure::Tokenizer(original)) => return original.clone(),
            Some(MySqlCursorFailure::Admitted(_, _)) => {
                // The admitted fatal JSON branch has a distinct inline loan
                // handback below. Ordinary cursor recovery cannot discard it.
                return TokenizerError::source(
                    "terminal Source condition requires its admitted handback".into(),
                    error.location,
                );
            }
            Some(MySqlCursorFailure::Source(original)) => {
                TokenizerError::source(original.clone(), error.location)
            }
            None => error,
        };
        *first = Some(MySqlCursorFailure::Tokenizer(selected.clone()));
        selected
    }
    fn first_error(&self, location: Location) -> Option<TokenizerError> {
        let selected = match self.error.borrow().as_ref() {
            Some(MySqlCursorFailure::Tokenizer(original)) => return Some(original.clone()),
            Some(MySqlCursorFailure::Admitted(_, _)) => {
                return Some(TokenizerError::source(
                    "terminal Source condition requires its admitted handback".into(),
                    location,
                ));
            }
            Some(MySqlCursorFailure::Source(original)) => {
                TokenizerError::source(original.clone(), location)
            }
            None => return None,
        };
        Some(self.record_error(selected))
    }
    fn check_error(&self) -> Result<(), String> {
        match self.error.borrow().as_ref() {
            Some(error) => Err(error.message().into()),
            None => Ok(()),
        }
    }
    fn token_error(&self, message: String, location: Location) -> TokenizerError {
        self.record_error(TokenizerError {
            message,
            location,
            source_kind: MySqlSourceErrorKind::Message,
        })
    }
    fn admit(&self, capacity: usize) -> Result<SqlSourceAdmission, String> {
        self.check_error()?;
        self.adapter
            .admit(capacity)
            .map_err(|error| self.failure(error))
    }
    fn admit_source(&self, capacity: usize) -> Result<SqlSourceAdmission, String> {
        self.check_error()?;
        self.adapter
            .admit_source(capacity)
            .map_err(|error| self.failure(error))
    }
    fn system_identifier(
        &self,
        bytes: &[u8],
        quoted: bool,
    ) -> Result<(String, SqlSourceAdmission), String> {
        self.check_error()?;
        self.adapter
            .system_identifier(bytes, quoted)
            .map_err(|error| self.failure(error))
    }
    fn admit_text_domain(&mut self, introducer: usize) -> Result<(), String> {
        self.check_error()?;
        let bytes = self
            .adapter
            .client_charset()
            .len()
            .checked_add(introducer)
            .ok_or_else(|| self.failure("text domain allocation overflow".into()))?;
        let admission = self.admit(bytes)?;
        self.retain_admission(admission)
    }
    fn retain_admission(&mut self, admission: SqlSourceAdmission) -> Result<(), String> {
        self.check_error()?;
        if self.allocations.len() == self.allocations.capacity() {
            let capacity = self
                .allocations
                .capacity()
                .max(8)
                .checked_mul(2)
                .ok_or_else(|| self.failure("source admission capacity overflow".into()))?;
            let charge = capacity
                .checked_mul(core::mem::size_of::<SqlSourceAdmission>())
                .ok_or_else(|| self.failure("source admission allocation overflow".into()))?;
            let replacement = self.admit(charge)?;
            self.allocations
                .reserve_exact(capacity - self.allocations.len());
            self.allocations_admission = Some(replacement);
        }
        self.allocations.push(admission);
        Ok(())
    }
    fn bytes(&self) -> &[u8] {
        self.source.bytes().expect("checked source admission")
    }
    fn unit_at(&self, offset: usize) -> Result<Option<MySqlLexicalUnit>, String> {
        // A first failure must prevent both native lexical work and later probes.
        self.check_error()?;
        let work = self.work.get().wrapping_add(1);
        self.work.set(work);
        if work.is_multiple_of(1024) && self.cancellation.as_ref().is_some_and(|check| check()) {
            return Err(self.failure(MySqlSourceError {
                message: QUERY_CANCELLED_MESSAGE.into(),
                kind: MySqlSourceErrorKind::Cancelled,
            }));
        }
        let Some(input) = self.bytes().get(offset..).filter(|input| !input.is_empty()) else {
            return Ok(None);
        };
        let unit = self
            .adapter
            .lexical_unit(input)
            .map_err(|error| self.failure(error))?;
        if unit.width == 0 || usize::from(unit.width) > input.len() {
            return Err(self.failure("native lexical unit is outside the source slice".into()));
        }
        Ok(Some(unit))
    }
    fn advance_bytes(&mut self, bytes: usize) -> Result<(), String> {
        self.check_error()?;
        let end = self
            .offset
            .checked_add(bytes)
            .ok_or_else(|| self.failure("raw source advance overflow".into()))?;
        while self.offset < end {
            let unit = self
                .unit_at(self.offset)?
                .ok_or_else(|| self.failure("raw source advance is outside source".into()))?;
            if self.offset + usize::from(unit.width) > end {
                return Err(self.failure("raw source advance splits a native unit".into()));
            }
            if unit.ascii && self.bytes()[self.offset] == b'\n' {
                self.location.line += 1;
                self.location.column = 1;
            } else {
                self.location.column += 1;
            }
            self.offset += usize::from(unit.width);
        }
        Ok(())
    }
    // get_text consumes one byte after an ASCII backslash, even when that
    // escaped byte could otherwise begin a native MB unit. This is not the
    // continuation of a unit recognized before the backslash.
    fn advance_escaped_byte(&mut self) {
        if self.error.borrow().is_some() {
            return;
        }
        if self.bytes()[self.offset] == b'\n' {
            self.location.line += 1;
            self.location.column = 1;
        } else {
            self.location.column += 1;
        }
        self.offset += 1;
    }
    fn ascii_at(&self, delta: usize) -> Result<Option<char>, String> {
        let mut offset = self.offset;
        for _ in 0..delta {
            let Some(unit) = self.unit_at(offset)? else {
                return Ok(None);
            };
            if !unit.ascii || unit.width != 1 {
                return Ok(None);
            }
            offset += 1;
        }
        let Some(unit) = self.unit_at(offset)? else {
            return Ok(None);
        };
        Ok((unit.ascii && unit.width == 1).then(|| char::from(self.bytes()[offset])))
    }
    fn occurrence(&self, start: usize, end: usize) -> Result<SqlSourceOccurrence, String> {
        self.check_error()?;
        self.source
            .slice(start..end)
            .map_err(|message| self.failure(message.into()))
    }
}

/// Bounded observation settings captured by the original Source adapter.
#[derive(Clone, Copy, Debug)]
pub struct MySqlDigestConfig {
    /// Original parser max_digest_length, not the separate PFS copy limit.
    pub capacity: usize,
    /// Actual native client CHARSET_INFO number.
    pub charset_number: u16,
    /// Captured IGNORE_SPACE/ANSI lexical behavior.
    pub ignore_space: bool,
    /// Captured HIGH_NOT_PRECEDENCE lexical behavior.
    pub high_not_precedence: bool,
    /// Captured PIPES_AS_CONCAT lexical behavior.
    pub pipes_as_concat: bool,
}

/// A reached Source route without a proven native canonical token production.
/// This is not a SQL failure and must never fall back to hashing raw SQL.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MySqlDigestUnavailable {
    /// Original digest allocation could not be admitted.
    Resource,
    /// Caller supplied no actual canonical hash service.
    HashService,
    /// No native mapping for this actual lexical production.
    Token,
    /// Original hint comment needs its actual hint-parser token callbacks.
    Hint,
    /// Native variable/hostname token role has not been issued by this grammar.
    VariableRole,
    /// Grammar lookahead/reduction cannot establish the native reduction order.
    GrammarLookahead,
    /// Original identifier byte-domain facts are unavailable.
    Identifier,
}

/// Canonical facts of one successfully parsed original Source occurrence.
/// Immutable, admitted, nonserialized and without execution/PSI authority.
#[derive(Clone, Debug)]
pub enum MySqlCanonicalDigestReceipt {
    /// Actual payload with its original capacity/header admission.
    Collected(Arc<MySqlCanonicalDigest>),
    /// Reached configuration without admitted payload; no heap allocation.
    Unavailable(MySqlDigestUnavailable),
}

/// Canonical token payload of one successfully parsed original occurrence.
#[derive(Debug)]
pub struct MySqlCanonicalDigest {
    tokens: Vec<u8>,
    hash: Option<[u8; 32]>,
    charset_number: u16,
    full: bool,
    unavailable: Option<MySqlDigestUnavailable>,
    _admission: Option<SqlSourceAdmission>,
}

impl MySqlCanonicalDigest {
    /// Actual collected native token bytes, not raw SQL or AST Display.
    pub fn token_bytes(&self) -> &[u8] {
        &self.tokens
    }
    /// Hash is absent whenever any native production is unavailable.
    pub fn hash(&self) -> Option<&[u8; 32]> {
        self.hash.as_ref()
    }
    /// Native charset used to render identifier bytes; not part of the hash.
    pub fn charset_number(&self) -> u16 {
        self.charset_number
    }
    /// Native bounded collector stopped appending; parsing still continued.
    pub fn is_full(&self) -> bool {
        self.full
    }
    /// Specific unavailable route, distinct from native configured truncation.
    pub fn unavailable(&self) -> Option<MySqlDigestUnavailable> {
        self.unavailable
    }
}

struct MySqlDigestWord {
    text: SqlSourceOccurrence,
    native: Option<(u16, bool)>,
    end: usize,
    null_source: Option<core::ops::Range<usize>>,
    system_variable: bool,
    // The shared grammar can demand this original close before committing
    // NULL. Native state 819 default-reduces first: retain the single pending
    // word relation until its genuine commit, without appending/replaying ')'.
    closing_parenthesis: bool,
}

struct MySqlDigestCollector {
    config: MySqlDigestConfig,
    tokens: Vec<u8>,
    full: bool,
    last_identifier: usize,
    last_period_end: Option<usize>,
    pending_word: Option<MySqlDigestWord>,
    // Hold the original demanded period until its physical successor proves
    // IDENT_SEP; this does not demand speculative bytes or a second token.
    pending_period: Option<usize>,
    // 0: normal, 1: after @, 2: after @@, 3: same ASCII HOSTNAME tail.
    variable_role: u8,
    variable_end: usize,
    hintable: bool,
    last_null_source: Option<core::ops::Range<usize>>,
    last_reduced_source: Option<core::ops::Range<usize>>,
    unavailable: Option<MySqlDigestUnavailable>,
    admission: Option<SqlSourceAdmission>,
}

impl MySqlDigestCollector {
    fn unavailable(config: MySqlDigestConfig, why: MySqlDigestUnavailable) -> Self {
        Self {
            config,
            tokens: Vec::new(),
            full: false,
            last_identifier: 0,
            last_period_end: None,
            pending_word: None,
            pending_period: None,
            variable_role: 0,
            variable_end: 0,
            hintable: false,
            last_null_source: None,
            last_reduced_source: None,
            unavailable: Some(why),
            admission: None,
        }
    }
    fn reject(&mut self, why: MySqlDigestUnavailable) {
        self.unavailable.get_or_insert(why);
        self.pending_word = None;
    }
    fn previous(&self, count: usize) -> u16 {
        let width = count * 2;
        if self.tokens.len() < self.last_identifier + width {
            return 1111;
        }
        let offset = self.tokens.len() - width;
        u16::from_le_bytes([self.tokens[offset], self.tokens[offset + 1]])
    }
    fn store(&mut self, token: u16) {
        if self.tokens.len() + 2 > self.config.capacity {
            self.full = true;
            return;
        }
        self.tokens.extend_from_slice(&token.to_le_bytes());
    }
    fn identifier(&mut self, text: &SqlSourceOccurrence) {
        let Ok(bytes) = text.bytes() else {
            self.reject(MySqlDigestUnavailable::Identifier);
            return;
        };
        self.identifier_bytes(bytes, 1106);
    }
    fn identifier_bytes(&mut self, bytes: &[u8], tag: u16) {
        // The native identifier length wire is unsigned 16-bit. Do not wrap a
        // product-accepted larger identifier into a forged native receipt.
        if bytes.len() > u16::MAX as usize {
            self.reject(MySqlDigestUnavailable::Identifier);
            return;
        }
        if self
            .tokens
            .len()
            .saturating_add(4)
            .saturating_add(bytes.len())
            > self.config.capacity
        {
            self.full = true;
            return;
        }
        self.store(tag);
        self.tokens
            .extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        self.tokens.extend_from_slice(bytes);
        self.last_identifier = self.tokens.len();
    }
    fn push(&mut self, mut token: u16, numeric: bool) {
        if self.full || self.unavailable.is_some() {
            return;
        }
        if numeric {
            loop {
                if !matches!(self.previous(1), 43 | 45)
                    || !mysql_digest_start_expr(self.previous(2))
                {
                    break;
                }
                self.tokens.truncate(self.tokens.len() - 2);
            }
        }
        if numeric || token == 1100 {
            token = 1100;
            if self.previous(1) == 44 && matches!(self.previous(2), 1100 | 1101) {
                self.tokens.truncate(self.tokens.len() - 4);
                token = 1101;
            }
        } else if token == 41 && self.previous(2) == 40 {
            let kind = match self.previous(1) {
                1100 => Some((1102, 1103)),
                1101 => Some((1104, 1105)),
                _ => None,
            };
            if let Some((row, list)) = kind {
                self.tokens.truncate(self.tokens.len() - 4);
                token = row;
                if self.previous(1) == 44
                    && matches!(self.previous(2), 1102 | 1103 | 1104 | 1105)
                    && matches!(
                        (row, self.previous(2)),
                        (1102, 1102 | 1103) | (1104, 1104 | 1105)
                    )
                {
                    self.tokens.truncate(self.tokens.len() - 4);
                    token = list;
                } else if self.previous(1) == 504 {
                    self.tokens.truncate(self.tokens.len() - 2);
                    token = 1110;
                }
            }
        } else if token == 734 && self.previous(1) == 892 {
            // Native lexer emits WITH_ROLLUP_SYM instead of WITH+ROLLUP.
            self.tokens.truncate(self.tokens.len() - 2);
            token = 894;
        }
        self.store(token);
    }
    fn flush_word(&mut self, following: Option<(&Token, usize)>) {
        let Some(word) = self.pending_word.take() else {
            return;
        };
        if self.full || self.unavailable.is_some() {
            return;
        }
        let as_function = following.is_some_and(|(next, start)| {
            matches!(next, Token::LParen) && (self.config.ignore_space || start == word.end)
        });
        match word.native {
            Some((id, function)) if !function || as_function => {
                let id = if id == 622 && self.config.high_not_precedence {
                    621
                } else {
                    id
                };
                self.push(id, false);
                self.hintable = matches!(id, 385 | 496 | 711 | 748 | 864);
                if id == 627 {
                    self.last_null_source = word.null_source;
                }
            }
            _ => self.identifier(&word.text),
        }
        if word.closing_parenthesis {
            self.push(41, false);
        }
    }
    fn finish_period(&mut self, following: Option<(&Token, &SqlSourceOccurrence)>) {
        let Some(end) = self.pending_period.take() else {
            return;
        };
        // sql_lex.cc 1554/1604: an immediately following native ident unit
        // suppresses the preceding keyword. Quoting/space/comments do not.
        // This raw Word issuer already proved the actual native units.
        let identifier = following.is_some_and(|(token, source)| {
            source.range().start == end
                && matches!(token, Token::Word(word) if word.quote_style.is_none())
                // The shared FSM's @name/@@name packing is a Word, but '@'
                // is MY_LEX_USER_END, never a native identifier-map unit.
                && source.bytes().is_ok_and(|bytes| !bytes.starts_with(b"@"))
        });
        if identifier {
            if let Some(word) = self.pending_word.as_mut() {
                if !word.system_variable {
                    word.native = None;
                }
            }
        } else if following.is_some_and(|(token, source)| {
            source.range().start == end && matches!(token, Token::Number(..))
        }) {
            // The shared decimal scanner can span several native IDENT_SEP
            // tokens. Its one-to-one native geometry is not proved here.
            self.reject(MySqlDigestUnavailable::GrammarLookahead);
            return;
        }
        self.flush_word(None);
        self.push(46, false);
        self.last_period_end = Some(end);
        self.hintable = false;
    }

    fn observe_variable(
        &mut self,
        token: &Token,
        source: &SqlSourceOccurrence,
        identifier: Option<&MySqlText>,
    ) -> bool {
        let range = source.range();
        if self.variable_role == 0 {
            return false;
        }
        if self.variable_role == 3 {
            if range.start == self.variable_end
                && (matches!(token, Token::Word(word) if word.quote_style.is_none())
                    || matches!(token, Token::Number(..) | Token::Period))
            {
                if source.bytes().is_ok_and(|bytes| {
                    bytes.iter().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'$')
                    })
                }) {
                    self.variable_end = range.end;
                    return true;
                }
                self.reject(MySqlDigestUnavailable::VariableRole);
                return true;
            }
            self.variable_role = 0;
            return false;
        }
        if self.variable_role == 2 {
            self.variable_role = 0;
            // IDENT_OR_KEYWORD does not skip whitespace, recognize charset
            // introducers, or admit function-only symbols after @@.
            if range.start != self.variable_end || !matches!(token, Token::Word(_)) {
                self.reject(MySqlDigestUnavailable::VariableRole);
                return true;
            }
            return false;
        }
        self.variable_role = 0;
        match token {
            Token::Word(word) if range.start == self.variable_end && word.quote_style.is_some() => {
                if let Some(text) = identifier.and_then(MySqlText::canonical_identifier_occurrence)
                {
                    self.identifier(text);
                } else {
                    self.reject(MySqlDigestUnavailable::Identifier);
                }
                true
            }
            Token::Word(_) | Token::Number(..) | Token::Period
                if range.start == self.variable_end =>
            {
                // Finite ASCII HOSTNAME subset, not a Unicode/alnum fallback.
                // Native byte-ctype proof for MB hostnames remains unavailable.
                if source.bytes().is_ok_and(|bytes| {
                    !bytes.is_empty()
                        && bytes.iter().all(|byte| {
                            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'$')
                        })
                }) {
                    self.push(1100, false);
                    self.variable_role = 3;
                    self.variable_end = range.end;
                } else {
                    self.reject(MySqlDigestUnavailable::VariableRole);
                }
                true
            }
            Token::MySqlText(_) if range.start == self.variable_end => false,
            _ => {
                // HOSTNAME starts without skipping spaces/comments; its empty
                // token also reduces to GENERIC_VALUE in the native digest.
                self.push(1100, false);
                false
            }
        }
    }

    fn observe(
        &mut self,
        token: &Token,
        source: &SqlSourceOccurrence,
        identifier: Option<&MySqlText>,
        adapter: &dyn MySqlSourceAdapter,
        empty_hint_comment: bool,
        comment_barrier: bool,
        hint_consumed: bool,
    ) {
        if self.full || self.unavailable.is_some() {
            return;
        }
        if matches!(token, Token::RParen)
            && self.pending_word.as_mut().is_some_and(|word| {
                if word.null_source.is_some() && !word.system_variable && !word.closing_parenthesis
                {
                    word.closing_parenthesis = true;
                    true
                } else {
                    false
                }
            })
        {
            // This is the original demanded token. Its only native append is
            // after the real NULL commit (or the unreduced keyword flush).
            return;
        }
        self.finish_period(Some((token, source)));
        if self.full || self.unavailable.is_some() {
            return;
        }
        if comment_barrier {
            self.flush_word(None);
            self.hintable = false;
        }
        if let Token::Whitespace(Whitespace::MySqlComment(comment)) = token {
            // IGNORE_SPACE skips lexer whitespace, not a comment before '('.
            self.flush_word(None);
            self.observe_variable(token, source, identifier);
            if !hint_consumed
                && self.hintable
                && comment
                    .bytes()
                    .is_ok_and(|bytes| bytes.starts_with(b"/*+") && !empty_hint_comment)
            {
                // Nonempty tokens/recovery must come from the genuine hint
                // consumer, never a second parse of this retained comment.
                self.reject(MySqlDigestUnavailable::Hint);
            }
            self.hintable = false;
            return;
        }
        if matches!(token, Token::Whitespace(_)) {
            self.observe_variable(token, source, identifier);
            return;
        }
        if matches!(token, Token::Period)
            && self
                .pending_word
                .as_ref()
                .is_some_and(|word| word.end == source.range().start)
        {
            self.pending_period = Some(source.range().end);
            return;
        }
        self.flush_word(Some((token, source.range().start)));
        self.hintable = false;
        let system_variable = self.variable_role == 2;
        if self.observe_variable(token, source, identifier) {
            return;
        }
        if self.full || self.unavailable.is_some() {
            return;
        }
        match token {
            Token::Word(word) => {
                let Some(text) = identifier else {
                    self.reject(MySqlDigestUnavailable::Identifier);
                    return;
                };
                let Some(text) = text.canonical_identifier_occurrence() else {
                    self.reject(MySqlDigestUnavailable::Identifier);
                    return;
                };
                // The shared MySQL FSM can return one Word for @name or
                // @@name. Split only the prefix actually present in that
                // admitted token occurrence; never use the AST variable name.
                if word.quote_style.is_none()
                    && text.bytes().is_ok_and(|bytes| bytes.starts_with(b"@"))
                {
                    let bytes = text.bytes().expect("attached raw identifier");
                    self.push(64, false);
                    if self.full {
                        return;
                    }
                    if bytes.starts_with(b"@@") {
                        self.push(64, false);
                        if self.full {
                            return;
                        }
                        if bytes.len() == 2 {
                            // A bundled bare @@ is the same admitted prefix
                            // as AtAt, not a failed identifier. The next real
                            // token owns IDENT_OR_KEYWORD/quoted-name proof.
                            self.variable_role = 2;
                            self.variable_end = source.range().end;
                            return;
                        }
                        if !bytes[2..]
                            .iter()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
                        {
                            self.reject(MySqlDigestUnavailable::VariableRole);
                            return;
                        }
                        let Ok(name) = text.slice(2..bytes.len()) else {
                            self.reject(MySqlDigestUnavailable::Identifier);
                            return;
                        };
                        let native =
                            mysql_digest_symbol(&bytes[2..]).filter(|(_, function)| !function);
                        self.pending_word = Some(MySqlDigestWord {
                            text: name,
                            native,
                            end: source.range().end,
                            null_source: (native == Some((627, false))).then(|| source.range()),
                            system_variable: true,
                            closing_parenthesis: false,
                        });
                    } else if bytes.len() == 1 {
                        // A demanded bare '@' still enters native HOSTNAME;
                        // its empty token is committed at the next demand/EOF.
                        self.variable_role = 1;
                        self.variable_end = source.range().end;
                    } else if bytes.len() > 1
                        && bytes[1..]
                            .iter()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
                    {
                        self.push(1100, false);
                        self.variable_role = 3;
                        self.variable_end = source.range().end;
                    } else {
                        self.reject(MySqlDigestUnavailable::VariableRole);
                    }
                    return;
                }
                let after_period = self.last_period_end == Some(source.range().start);
                if word.quote_style.is_none() && !after_period && !system_variable {
                    if let Some(name) = word.value.strip_prefix('_') {
                        match adapter.canonical_charset_introducer(name) {
                            Some(true) => {
                                self.push(852, false);
                                return;
                            }
                            Some(false) => {}
                            None => {
                                self.reject(MySqlDigestUnavailable::Token);
                                return;
                            }
                        }
                    }
                }
                let Ok(bytes) = text.bytes() else {
                    self.reject(MySqlDigestUnavailable::Identifier);
                    return;
                };
                let native = if word.quote_style.is_none() && !after_period {
                    mysql_digest_symbol(bytes).filter(|(_, function)| !system_variable || !function)
                } else {
                    None
                };
                self.pending_word = Some(MySqlDigestWord {
                    text: text.clone(),
                    native,
                    end: source.range().end,
                    null_source: (native == Some((627, false))).then(|| source.range()),
                    system_variable,
                    closing_parenthesis: false,
                });
            }
            Token::Number(..) | Token::MySqlHexStringLiteral(..) => self.push(1100, true),
            Token::MySqlText(text) => {
                if text.introducer().is_some_and(|name| name.starts_with('_')) {
                    self.push(852, false);
                }
                self.push(1100, false);
            }
            Token::Placeholder(text) if text == "?" => self.push(1100, false),
            Token::AtSign | Token::AtAt => {
                self.push(64, false);
                if self.full {
                    return;
                }
                if matches!(token, Token::AtAt) {
                    self.push(64, false);
                    if self.full {
                        return;
                    }
                    self.variable_role = 2;
                } else {
                    self.variable_role = 1;
                }
                self.variable_end = source.range().end;
            }
            Token::StringConcat => self.push(
                if self.config.pipes_as_concat {
                    644
                } else {
                    642
                },
                false,
            ),
            Token::Eq => self.push(415, false),
            Token::Neq => self.push(614, false),
            Token::Lt => self.push(549, false),
            Token::Gt => self.push(469, false),
            Token::LtEq => self.push(522, false),
            Token::GtEq => self.push(456, false),
            Token::Spaceship => self.push(416, false),
            Token::ShiftLeft => self.push(759, false),
            Token::ShiftRight => self.push(760, false),
            Token::Overlap => self.push(273, false),
            Token::Assignment => self.push(757, false),
            Token::Arrow => self.push(514, false),
            Token::LongArrow => self.push(907, false),
            Token::ExclamationMark => self.push(621, false),
            Token::Plus => self.push(43, false),
            Token::Minus => self.push(45, false),
            Token::Mul => self.push(42, false),
            Token::Div => self.push(47, false),
            Token::Mod => self.push(37, false),
            Token::Comma => self.push(44, false),
            Token::LParen => self.push(40, false),
            Token::RParen => self.push(41, false),
            Token::Period => {
                self.push(46, false);
                self.last_period_end = Some(source.range().end);
            }
            Token::SemiColon => self.push(59, false),
            Token::Ampersand => self.push(38, false),
            Token::Pipe => self.push(124, false),
            Token::Caret => self.push(94, false),
            Token::Tilde => self.push(126, false),
            Token::Colon => self.push(58, false),
            Token::LBrace => self.push(123, false),
            Token::RBrace => self.push(125, false),
            Token::LBracket => self.push(91, false),
            Token::RBracket => self.push(93, false),
            Token::Char(character)
                if character.is_ascii()
                    && !matches!(*character, '\0' | '?' | '@' | '#' | '\\')
                    && source
                        .bytes()
                        .is_ok_and(|bytes| bytes == [*character as u8]) =>
            {
                self.push(*character as u16, false);
            }
            Token::EOF => {}
            _ => self.reject(MySqlDigestUnavailable::Token),
        }
    }
    fn hint_token(&mut self, kind: MySqlHintTokenKind, bytes: &[u8], started: &mut bool) {
        if self.full || self.unavailable.is_some() {
            return;
        }
        if matches!(kind, MySqlHintTokenKind::Error) {
            return;
        }
        if matches!(kind, MySqlHintTokenKind::Close) {
            if *started {
                self.push(1109, false);
            }
            return;
        }
        if !*started {
            self.push(1108, false);
            *started = true;
        }
        match kind {
            MySqlHintTokenKind::Keyword(keyword) => self.push(keyword.digest_token(), false),
            MySqlHintTokenKind::Number => self.push(1100, true),
            MySqlHintTokenKind::Text => self.push(1100, false),
            MySqlHintTokenKind::Identifier { adjacent_at } => {
                self.identifier_bytes(bytes, if adjacent_at { 1107 } else { 1106 })
            }
            MySqlHintTokenKind::QueryBlock => {
                self.push(64, false);
                self.identifier_bytes(bytes, 1106);
            }
            MySqlHintTokenKind::Symbol(byte) => self.push(u16::from(byte), false),
            MySqlHintTokenKind::Close | MySqlHintTokenKind::Error => {}
        }
    }
    fn reduce_null(&mut self, source: core::ops::Range<usize>) {
        if self.full || self.unavailable.is_some() {
            return;
        }
        let pending_null = self
            .pending_word
            .as_ref()
            .is_some_and(|word| word.null_source.as_ref() == Some(&source));
        let closing_parenthesis = self
            .pending_word
            .as_mut()
            .filter(|word| word.null_source.as_ref() == Some(&source))
            .is_some_and(|word| core::mem::take(&mut word.closing_parenthesis));
        if pending_null {
            self.flush_word(None);
        }
        // A different pending word is the original following token (for
        // example an alias), not this literal's commit. Leave its existing
        // function/identifier lookahead relation intact; native state 819
        // reduces NULL before that word's eventual append.
        // Native digest_reduce_token returns immediately when the attempted
        // original NULL append has filled storage, retaining that exact prefix.
        if self.full || self.unavailable.is_some() {
            return;
        }
        if self.last_null_source.as_ref() != Some(&source) {
            self.reject(MySqlDigestUnavailable::GrammarLookahead);
            return;
        }
        let trailing = if self.previous(1) == 627 {
            self.tokens.truncate(self.tokens.len() - 2);
            None
        } else if self.previous(2) == 627 {
            // A close that escaped the one pending-word relation before
            // commitment has already been appended. Rewinding/replaying it
            // would invent another native lookahead/reduction sequence.
            if self.previous(1) == 41 {
                self.reject(MySqlDigestUnavailable::GrammarLookahead);
                return;
            }
            let next = self.previous(1);
            self.tokens.truncate(self.tokens.len() - 4);
            Some(next)
        } else {
            self.reject(MySqlDigestUnavailable::GrammarLookahead);
            return;
        };
        self.push(1100, false);
        if let Some(next) = trailing {
            self.store(next);
        }
        if closing_parenthesis {
            // Native shifted NULL enters default state 819 before yylex can
            // issue ')'. Append the held original token exactly once through
            // its normal row/IN production, never a duplicate store/replay.
            self.push(41, false);
        }
        self.last_reduced_source = Some(source);
        self.last_null_source = None;
    }
}

// Derived from the complete fixed 8.0.46 sql/lex.h and numbered sql_yacc.yy.
// Only maps already produced Source tokens: no lexer/resolver or SQL authority.
const MYSQL_DIGEST_SYMBOLS: &[(&[u8], u16, bool)] = &[
    (b"!=", 614, false),
    (b"&&", 273, false),
    (b"<", 549, false),
    (b"<<", 759, false),
    (b"<=", 522, false),
    (b"<=>", 416, false),
    (b"<>", 614, false),
    (b"=", 415, false),
    (b">", 469, false),
    (b">=", 456, false),
    (b">>", 760, false),
    (b"ACCESSIBLE", 259, false),
    (b"ACCOUNT", 260, false),
    (b"ACTION", 261, false),
    (b"ACTIVE", 973, false),
    (b"ADD", 262, false),
    (b"ADDDATE", 263, true),
    (b"ADMIN", 910, false),
    (b"AFTER", 264, false),
    (b"AGAINST", 265, false),
    (b"AGGREGATE", 266, false),
    (b"ALGORITHM", 267, false),
    (b"ALL", 268, false),
    (b"ALTER", 269, false),
    (b"ALWAYS", 270, false),
    (b"ANALYZE", 272, false),
    (b"AND", 274, false),
    (b"ANY", 275, false),
    (b"ARRAY", 976, false),
    (b"AS", 276, false),
    (b"ASC", 277, false),
    (b"ASCII", 278, false),
    (b"ASENSITIVE", 279, false),
    (b"ASSIGN_GTIDS_TO_ANONYMOUS_TRANSACTIONS", 1161, false),
    (b"AT", 280, false),
    (b"ATTRIBUTE", 1153, false),
    (b"AUTHENTICATION", 1191, false),
    (b"AUTOEXTEND_SIZE", 281, false),
    (b"AUTO_INCREMENT", 282, false),
    (b"AVG", 284, false),
    (b"AVG_ROW_LENGTH", 283, false),
    (b"BACKUP", 285, false),
    (b"BEFORE", 286, false),
    (b"BEGIN", 287, false),
    (b"BETWEEN", 288, false),
    (b"BIGINT", 289, false),
    (b"BINARY", 290, false),
    (b"BINLOG", 291, false),
    (b"BIT", 295, false),
    (b"BIT_AND", 293, true),
    (b"BIT_OR", 294, true),
    (b"BIT_XOR", 296, true),
    (b"BLOB", 297, false),
    (b"BLOCK", 298, false),
    (b"BOOL", 300, false),
    (b"BOOLEAN", 299, false),
    (b"BOTH", 301, false),
    (b"BTREE", 302, false),
    (b"BUCKETS", 929, false),
    (b"BULK", 1201, false),
    (b"BY", 303, false),
    (b"BYTE", 304, false),
    (b"CACHE", 305, false),
    (b"CALL", 306, false),
    (b"CASCADE", 307, false),
    (b"CASCADED", 308, false),
    (b"CASE", 309, false),
    (b"CAST", 310, true),
    (b"CATALOG_NAME", 311, false),
    (b"CHAIN", 312, false),
    (b"CHALLENGE_RESPONSE", 1198, false),
    (b"CHANGE", 313, false),
    (b"CHANGED", 314, false),
    (b"CHANNEL", 315, false),
    (b"CHAR", 317, false),
    (b"CHARACTER", 317, false),
    (b"CHARSET", 316, false),
    (b"CHECK", 319, false),
    (b"CHECKSUM", 318, false),
    (b"CIPHER", 320, false),
    (b"CLASS_ORIGIN", 321, false),
    (b"CLIENT", 322, false),
    (b"CLONE", 931, false),
    (b"CLOSE", 323, false),
    (b"COALESCE", 324, false),
    (b"CODE", 325, false),
    (b"COLLATE", 326, false),
    (b"COLLATION", 327, false),
    (b"COLUMN", 329, false),
    (b"COLUMNS", 328, false),
    (b"COLUMN_FORMAT", 330, false),
    (b"COLUMN_NAME", 331, false),
    (b"COMMENT", 332, false),
    (b"COMMIT", 334, false),
    (b"COMMITTED", 333, false),
    (b"COMPACT", 335, false),
    (b"COMPLETION", 336, false),
    (b"COMPONENT", 914, false),
    (b"COMPRESSED", 337, false),
    (b"COMPRESSION", 338, false),
    (b"CONCURRENT", 340, false),
    (b"CONDITION", 341, false),
    (b"CONNECTION", 342, false),
    (b"CONSISTENT", 343, false),
    (b"CONSTRAINT", 344, false),
    (b"CONSTRAINT_CATALOG", 345, false),
    (b"CONSTRAINT_NAME", 346, false),
    (b"CONSTRAINT_SCHEMA", 347, false),
    (b"CONTAINS", 348, false),
    (b"CONTEXT", 349, false),
    (b"CONTINUE", 350, false),
    (b"CONVERT", 351, false),
    (b"COUNT", 352, true),
    (b"CPU", 353, false),
    (b"CREATE", 354, false),
    (b"CROSS", 355, false),
    (b"CUBE", 356, false),
    (b"CUME_DIST", 932, false),
    (b"CURDATE", 357, true),
    (b"CURRENT", 358, false),
    (b"CURRENT_DATE", 357, false),
    (b"CURRENT_TIME", 362, false),
    (b"CURRENT_TIMESTAMP", 623, false),
    (b"CURRENT_USER", 359, false),
    (b"CURSOR", 360, false),
    (b"CURSOR_NAME", 361, false),
    (b"CURTIME", 362, true),
    (b"DATA", 366, false),
    (b"DATABASE", 363, false),
    (b"DATABASES", 364, false),
    (b"DATAFILE", 365, false),
    (b"DATE", 370, false),
    (b"DATETIME", 367, false),
    (b"DATE_ADD", 368, true),
    (b"DATE_SUB", 369, true),
    (b"DAY", 375, false),
    (b"DAY_HOUR", 371, false),
    (b"DAY_MICROSECOND", 372, false),
    (b"DAY_MINUTE", 373, false),
    (b"DAY_SECOND", 374, false),
    (b"DEALLOCATE", 376, false),
    (b"DEC", 378, false),
    (b"DECIMAL", 378, false),
    (b"DECLARE", 379, false),
    (b"DEFAULT", 380, false),
    (b"DEFAULT_AUTH", 381, false),
    (b"DEFINER", 382, false),
    (b"DEFINITION", 969, false),
    (b"DELAYED", 383, false),
    (b"DELAY_KEY_WRITE", 384, false),
    (b"DELETE", 385, false),
    (b"DENSE_RANK", 933, false),
    (b"DESC", 386, false),
    (b"DESCRIBE", 387, false),
    (b"DESCRIPTION", 970, false),
    (b"DETERMINISTIC", 389, false),
    (b"DIAGNOSTICS", 390, false),
    (b"DIRECTORY", 391, false),
    (b"DISABLE", 392, false),
    (b"DISCARD", 393, false),
    (b"DISK", 394, false),
    (b"DISTINCT", 395, false),
    (b"DISTINCTROW", 395, false),
    (b"DIV", 396, false),
    (b"DO", 398, false),
    (b"DOUBLE", 397, false),
    (b"DROP", 399, false),
    (b"DUAL", 400, false),
    (b"DUMPFILE", 401, false),
    (b"DUPLICATE", 402, false),
    (b"DYNAMIC", 403, false),
    (b"EACH", 404, false),
    (b"ELSE", 405, false),
    (b"ELSEIF", 406, false),
    (b"EMPTY", 954, false),
    (b"ENABLE", 407, false),
    (b"ENCLOSED", 408, false),
    (b"ENCRYPTION", 339, false),
    (b"END", 409, false),
    (b"ENDS", 410, false),
    (b"ENFORCED", 985, false),
    (b"ENGINE", 413, false),
    (b"ENGINES", 412, false),
    (b"ENGINE_ATTRIBUTE", 1154, false),
    (b"ENUM", 414, false),
    (b"ERROR", 417, false),
    (b"ERRORS", 418, false),
    (b"ESCAPE", 420, false),
    (b"ESCAPED", 419, false),
    (b"EVENT", 422, false),
    (b"EVENTS", 421, false),
    (b"EVERY", 423, false),
    (b"EXCEPT", 913, false),
    (b"EXCHANGE", 424, false),
    (b"EXCLUDE", 934, false),
    (b"EXECUTE", 425, false),
    (b"EXISTS", 426, false),
    (b"EXIT", 427, false),
    (b"EXPANSION", 428, false),
    (b"EXPIRE", 429, false),
    (b"EXPLAIN", 387, false),
    (b"EXPORT", 430, false),
    (b"EXTENDED", 431, false),
    (b"EXTENT_SIZE", 432, false),
    (b"EXTRACT", 433, true),
    (b"FACTOR", 1192, false),
    (b"FAILED_LOGIN_ATTEMPTS", 995, false),
    (b"FALSE", 434, false),
    (b"FAST", 435, false),
    (b"FAULTS", 436, false),
    (b"FETCH", 437, false),
    (b"FIELDS", 328, false),
    (b"FILE", 438, false),
    (b"FILE_BLOCK_SIZE", 439, false),
    (b"FILTER", 440, false),
    (b"FINISH", 1193, false),
    (b"FIRST", 441, false),
    (b"FIRST_VALUE", 935, false),
    (b"FIXED", 442, false),
    (b"FLOAT", 444, false),
    (b"FLOAT4", 444, false),
    (b"FLOAT8", 397, false),
    (b"FLUSH", 445, false),
    (b"FOLLOWING", 936, false),
    (b"FOLLOWS", 446, false),
    (b"FOR", 449, false),
    (b"FORCE", 447, false),
    (b"FOREIGN", 448, false),
    (b"FORMAT", 450, false),
    (b"FOUND", 451, false),
    (b"FROM", 452, false),
    (b"FULL", 453, false),
    (b"FULLTEXT", 454, false),
    (b"FUNCTION", 455, false),
    (b"GENERAL", 457, false),
    (b"GENERATE", 1203, false),
    (b"GENERATED", 458, false),
    (b"GEOMCOLLECTION", 460, false),
    (b"GEOMETRY", 461, false),
    (b"GEOMETRYCOLLECTION", 460, false),
    (b"GET", 463, false),
    (b"GET_FORMAT", 462, false),
    (b"GET_MASTER_PUBLIC_KEY", 967, false),
    (b"GET_SOURCE_PUBLIC_KEY", 1162, false),
    (b"GLOBAL", 464, false),
    (b"GRANT", 465, false),
    (b"GRANTS", 466, false),
    (b"GROUP", 467, false),
    (b"GROUPING", 926, false),
    (b"GROUPS", 937, false),
    (b"GROUP_CONCAT", 468, true),
    (b"GROUP_REPLICATION", 459, false),
    (b"GTID_ONLY", 1199, false),
    (b"HANDLER", 470, false),
    (b"HASH", 471, false),
    (b"HAVING", 472, false),
    (b"HELP", 473, false),
    (b"HIGH_PRIORITY", 475, false),
    (b"HISTOGRAM", 928, false),
    (b"HISTORY", 959, false),
    (b"HOST", 476, false),
    (b"HOSTS", 477, false),
    (b"HOUR", 481, false),
    (b"HOUR_MICROSECOND", 478, false),
    (b"HOUR_MINUTE", 479, false),
    (b"HOUR_SECOND", 480, false),
    (b"IDENTIFIED", 483, false),
    (b"IF", 485, false),
    (b"IGNORE", 486, false),
    (b"IGNORE_SERVER_IDS", 487, false),
    (b"IMPORT", 488, false),
    (b"IN", 504, false),
    (b"INACTIVE", 974, false),
    (b"INDEX", 490, false),
    (b"INDEXES", 489, false),
    (b"INFILE", 491, false),
    (b"INITIAL", 1197, false),
    (b"INITIAL_SIZE", 492, false),
    (b"INITIATE", 1194, false),
    (b"INNER", 493, false),
    (b"INOUT", 494, false),
    (b"INSENSITIVE", 495, false),
    (b"INSERT", 496, false),
    (b"INSERT_METHOD", 497, false),
    (b"INSTALL", 499, false),
    (b"INSTANCE", 498, false),
    (b"INT", 502, false),
    (b"INT1", 836, false),
    (b"INT2", 768, false),
    (b"INT3", 584, false),
    (b"INT4", 502, false),
    (b"INT8", 289, false),
    (b"INTEGER", 502, false),
    (b"INTERSECT", 1200, false),
    (b"INTERVAL", 500, false),
    (b"INTO", 501, false),
    (b"INVISIBLE", 911, false),
    (b"INVOKER", 503, false),
    (b"IO", 507, false),
    (b"IO_AFTER_GTIDS", 505, false),
    (b"IO_BEFORE_GTIDS", 506, false),
    (b"IO_THREAD", 702, false),
    (b"IPC", 508, false),
    (b"IS", 509, false),
    (b"ISOLATION", 510, false),
    (b"ISSUER", 511, false),
    (b"ITERATE", 512, false),
    (b"JOIN", 513, false),
    (b"JSON", 515, false),
    (b"JSON_ARRAYAGG", 921, true),
    (b"JSON_OBJECTAGG", 920, true),
    (b"JSON_TABLE", 955, false),
    (b"JSON_VALUE", 1151, false),
    (b"KEY", 518, false),
    (b"KEYRING", 1190, false),
    (b"KEYS", 516, false),
    (b"KEY_BLOCK_SIZE", 517, false),
    (b"KILL", 519, false),
    (b"LAG", 938, false),
    (b"LANGUAGE", 520, false),
    (b"LAST", 521, false),
    (b"LAST_VALUE", 939, false),
    (b"LATERAL", 975, false),
    (b"LEAD", 940, false),
    (b"LEADING", 523, false),
    (b"LEAVE", 525, false),
    (b"LEAVES", 524, false),
    (b"LEFT", 526, false),
    (b"LESS", 527, false),
    (b"LEVEL", 528, false),
    (b"LIKE", 530, false),
    (b"LIMIT", 531, false),
    (b"LINEAR", 532, false),
    (b"LINES", 533, false),
    (b"LINESTRING", 534, false),
    (b"LIST", 535, false),
    (b"LOAD", 536, false),
    (b"LOCAL", 537, false),
    (b"LOCALTIME", 623, false),
    (b"LOCALTIMESTAMP", 623, false),
    (b"LOCK", 540, false),
    (b"LOCKED", 924, false),
    (b"LOCKS", 539, false),
    (b"LOGFILE", 541, false),
    (b"LOGS", 542, false),
    (b"LONG", 546, false),
    (b"LONGBLOB", 543, false),
    (b"LONGTEXT", 544, false),
    (b"LOOP", 547, false),
    (b"LOW_PRIORITY", 548, false),
    (b"MASTER", 571, false),
    (b"MASTER_AUTO_POSITION", 550, false),
    (b"MASTER_BIND", 551, false),
    (b"MASTER_COMPRESSION_ALGORITHMS", 989, false),
    (b"MASTER_CONNECT_RETRY", 552, false),
    (b"MASTER_DELAY", 553, false),
    (b"MASTER_HEARTBEAT_PERIOD", 573, false),
    (b"MASTER_HOST", 554, false),
    (b"MASTER_LOG_FILE", 555, false),
    (b"MASTER_LOG_POS", 556, false),
    (b"MASTER_PASSWORD", 557, false),
    (b"MASTER_PORT", 558, false),
    (b"MASTER_PUBLIC_KEY_PATH", 966, false),
    (b"MASTER_RETRY_COUNT", 559, false),
    (b"MASTER_SSL", 569, false),
    (b"MASTER_SSL_CA", 563, false),
    (b"MASTER_SSL_CAPATH", 561, false),
    (b"MASTER_SSL_CERT", 564, false),
    (b"MASTER_SSL_CIPHER", 565, false),
    (b"MASTER_SSL_CRL", 566, false),
    (b"MASTER_SSL_CRLPATH", 567, false),
    (b"MASTER_SSL_KEY", 568, false),
    (b"MASTER_SSL_VERIFY_SERVER_CERT", 570, false),
    (b"MASTER_TLS_CIPHERSUITES", 992, false),
    (b"MASTER_TLS_VERSION", 562, false),
    (b"MASTER_USER", 572, false),
    (b"MASTER_ZSTD_COMPRESSION_LEVEL", 990, false),
    (b"MATCH", 574, false),
    (b"MAX", 579, true),
    (b"MAXVALUE", 582, false),
    (b"MAX_CONNECTIONS_PER_HOUR", 575, false),
    (b"MAX_QUERIES_PER_HOUR", 576, false),
    (b"MAX_ROWS", 577, false),
    (b"MAX_SIZE", 578, false),
    (b"MAX_UPDATES_PER_HOUR", 580, false),
    (b"MAX_USER_CONNECTIONS", 581, false),
    (b"MEDIUM", 586, false),
    (b"MEDIUMBLOB", 583, false),
    (b"MEDIUMINT", 584, false),
    (b"MEDIUMTEXT", 585, false),
    (b"MEMBER", 977, false),
    (b"MEMORY", 587, false),
    (b"MERGE", 588, false),
    (b"MESSAGE_TEXT", 589, false),
    (b"MICROSECOND", 590, false),
    (b"MID", 811, true),
    (b"MIDDLEINT", 584, false),
    (b"MIGRATE", 591, false),
    (b"MIN", 596, true),
    (b"MINUTE", 594, false),
    (b"MINUTE_MICROSECOND", 592, false),
    (b"MINUTE_SECOND", 593, false),
    (b"MIN_ROWS", 595, false),
    (b"MOD", 600, false),
    (b"MODE", 597, false),
    (b"MODIFIES", 598, false),
    (b"MODIFY", 599, false),
    (b"MONTH", 601, false),
    (b"MULTILINESTRING", 602, false),
    (b"MULTIPOINT", 603, false),
    (b"MULTIPOLYGON", 604, false),
    (b"MUTEX", 605, false),
    (b"MYSQL_ERRNO", 606, false),
    (b"NAME", 608, false),
    (b"NAMES", 607, false),
    (b"NATIONAL", 609, false),
    (b"NATURAL", 610, false),
    (b"NCHAR", 612, false),
    (b"NDB", 613, false),
    (b"NDBCLUSTER", 613, false),
    (b"NESTED", 956, false),
    (b"NETWORK_NAMESPACE", 987, false),
    (b"NEVER", 616, false),
    (b"NEW", 617, false),
    (b"NEXT", 618, false),
    (b"NO", 624, false),
    (b"NODEGROUP", 619, false),
    (b"NONE", 620, false),
    (b"NOT", 622, false),
    (b"NOW", 623, true),
    (b"NOWAIT", 925, false),
    (b"NO_WAIT", 625, false),
    (b"NO_WRITE_TO_BINLOG", 626, false),
    (b"NTH_VALUE", 941, false),
    (b"NTILE", 942, false),
    (b"NULL", 627, false),
    (b"NULLS", 943, false),
    (b"NUMBER", 629, false),
    (b"NUMERIC", 630, false),
    (b"NVARCHAR", 631, false),
    (b"OF", 922, false),
    (b"OFF", 998, false),
    (b"OFFSET", 632, false),
    (b"OJ", 986, false),
    (b"OLD", 984, false),
    (b"ON", 633, false),
    (b"ONE", 634, false),
    (b"ONLY", 635, false),
    (b"OPEN", 636, false),
    (b"OPTIMIZE", 637, false),
    (b"OPTIMIZER_COSTS", 638, false),
    (b"OPTION", 640, false),
    (b"OPTIONAL", 978, false),
    (b"OPTIONALLY", 641, false),
    (b"OPTIONS", 639, false),
    (b"OR", 645, false),
    (b"ORDER", 643, false),
    (b"ORDINALITY", 957, false),
    (b"ORGANIZATION", 971, false),
    (b"OTHERS", 944, false),
    (b"OUT", 648, false),
    (b"OUTER", 646, false),
    (b"OUTFILE", 647, false),
    (b"OVER", 945, false),
    (b"OWNER", 649, false),
    (b"PACK_KEYS", 650, false),
    (b"PAGE", 651, false),
    (b"PARSER", 653, false),
    (b"PARTIAL", 655, false),
    (b"PARTITION", 656, false),
    (b"PARTITIONING", 658, false),
    (b"PARTITIONS", 657, false),
    (b"PASSWORD", 659, false),
    (b"PASSWORD_LOCK_TIME", 994, false),
    (b"PATH", 958, false),
    (b"PERCENT_RANK", 946, false),
    (b"PERSIST", 908, false),
    (b"PERSIST_ONLY", 927, false),
    (b"PHASE", 660, false),
    (b"PLUGIN", 662, false),
    (b"PLUGINS", 663, false),
    (b"PLUGIN_DIR", 661, false),
    (b"POINT", 664, false),
    (b"POLYGON", 665, false),
    (b"PORT", 666, false),
    (b"POSITION", 667, true),
    (b"PRECEDES", 668, false),
    (b"PRECEDING", 947, false),
    (b"PRECISION", 669, false),
    (b"PREPARE", 670, false),
    (b"PRESERVE", 671, false),
    (b"PREV", 672, false),
    (b"PRIMARY", 673, false),
    (b"PRIVILEGES", 674, false),
    (b"PRIVILEGE_CHECKS_USER", 991, false),
    (b"PROCEDURE", 675, false),
    (b"PROCESS", 676, false),
    (b"PROCESSLIST", 677, false),
    (b"PROFILE", 678, false),
    (b"PROFILES", 679, false),
    (b"PROXY", 680, false),
    (b"PURGE", 681, false),
    (b"QUARTER", 682, false),
    (b"QUERY", 683, false),
    (b"QUICK", 684, false),
    (b"RANDOM", 988, false),
    (b"RANGE", 685, false),
    (b"RANK", 948, false),
    (b"READ", 688, false),
    (b"READS", 686, false),
    (b"READ_ONLY", 687, false),
    (b"READ_WRITE", 689, false),
    (b"REAL", 690, false),
    (b"REBUILD", 691, false),
    (b"RECOVER", 692, false),
    (b"RECURSIVE", 915, false),
    (b"REDO_BUFFER_SIZE", 694, false),
    (b"REDUNDANT", 695, false),
    (b"REFERENCE", 972, false),
    (b"REFERENCES", 696, false),
    (b"REGEXP", 697, false),
    (b"REGISTRATION", 1195, false),
    (b"RELAY", 698, false),
    (b"RELAYLOG", 699, false),
    (b"RELAY_LOG_FILE", 700, false),
    (b"RELAY_LOG_POS", 701, false),
    (b"RELAY_THREAD", 702, false),
    (b"RELEASE", 703, false),
    (b"RELOAD", 704, false),
    (b"REMOVE", 705, false),
    (b"RENAME", 706, false),
    (b"REORGANIZE", 707, false),
    (b"REPAIR", 708, false),
    (b"REPEAT", 710, false),
    (b"REPEATABLE", 709, false),
    (b"REPLACE", 711, false),
    (b"REPLICA", 1159, false),
    (b"REPLICAS", 1160, false),
    (b"REPLICATE_DO_DB", 713, false),
    (b"REPLICATE_DO_TABLE", 715, false),
    (b"REPLICATE_IGNORE_DB", 714, false),
    (b"REPLICATE_IGNORE_TABLE", 716, false),
    (b"REPLICATE_REWRITE_DB", 719, false),
    (b"REPLICATE_WILD_DO_TABLE", 717, false),
    (b"REPLICATE_WILD_IGNORE_TABLE", 718, false),
    (b"REPLICATION", 712, false),
    (b"REQUIRE", 720, false),
    (b"REQUIRE_ROW_FORMAT", 993, false),
    (b"REQUIRE_TABLE_PRIMARY_KEY_CHECK", 996, false),
    (b"RESET", 721, false),
    (b"RESIGNAL", 722, false),
    (b"RESOURCE", 963, false),
    (b"RESPECT", 949, false),
    (b"RESTART", 968, false),
    (b"RESTORE", 724, false),
    (b"RESTRICT", 725, false),
    (b"RESUME", 726, false),
    (b"RETAIN", 983, false),
    (b"RETURN", 729, false),
    (b"RETURNED_SQLSTATE", 727, false),
    (b"RETURNING", 999, false),
    (b"RETURNS", 728, false),
    (b"REUSE", 960, false),
    (b"REVERSE", 730, false),
    (b"REVOKE", 731, false),
    (b"RIGHT", 732, false),
    (b"RLIKE", 697, false),
    (b"ROLE", 909, false),
    (b"ROLLBACK", 733, false),
    (b"ROLLUP", 734, false),
    (b"ROTATE", 735, false),
    (b"ROUTINE", 736, false),
    (b"ROW", 739, false),
    (b"ROWS", 737, false),
    (b"ROW_COUNT", 740, false),
    (b"ROW_FORMAT", 738, false),
    (b"ROW_NUMBER", 950, false),
    (b"RTREE", 741, false),
    (b"SAVEPOINT", 742, false),
    (b"SCHEDULE", 743, false),
    (b"SCHEMA", 363, false),
    (b"SCHEMAS", 364, false),
    (b"SCHEMA_NAME", 744, false),
    (b"SECOND", 746, false),
    (b"SECONDARY", 979, false),
    (b"SECONDARY_ENGINE", 980, false),
    (b"SECONDARY_ENGINE_ATTRIBUTE", 1155, false),
    (b"SECONDARY_LOAD", 981, false),
    (b"SECONDARY_UNLOAD", 982, false),
    (b"SECOND_MICROSECOND", 745, false),
    (b"SECURITY", 747, false),
    (b"SELECT", 748, false),
    (b"SENSITIVE", 749, false),
    (b"SEPARATOR", 750, false),
    (b"SERIAL", 752, false),
    (b"SERIALIZABLE", 751, false),
    (b"SERVER", 754, false),
    (b"SESSION", 753, false),
    (b"SESSION_USER", 867, true),
    (b"SET", 756, false),
    (b"SHARE", 758, false),
    (b"SHOW", 761, false),
    (b"SHUTDOWN", 762, false),
    (b"SIGNAL", 763, false),
    (b"SIGNED", 764, false),
    (b"SIMPLE", 765, false),
    (b"SKIP", 923, false),
    (b"SLAVE", 766, false),
    (b"SLOW", 767, false),
    (b"SMALLINT", 768, false),
    (b"SNAPSHOT", 769, false),
    (b"SOCKET", 770, false),
    (b"SOME", 275, false),
    (b"SONAME", 771, false),
    (b"SOUNDS", 772, false),
    (b"SOURCE", 773, false),
    (b"SOURCE_AUTO_POSITION", 1163, false),
    (b"SOURCE_BIND", 1164, false),
    (b"SOURCE_COMPRESSION_ALGORITHMS", 1165, false),
    (b"SOURCE_CONNECTION_AUTO_FAILOVER", 1156, false),
    (b"SOURCE_CONNECT_RETRY", 1166, false),
    (b"SOURCE_DELAY", 1167, false),
    (b"SOURCE_HEARTBEAT_PERIOD", 1168, false),
    (b"SOURCE_HOST", 1169, false),
    (b"SOURCE_LOG_FILE", 1170, false),
    (b"SOURCE_LOG_POS", 1171, false),
    (b"SOURCE_PASSWORD", 1172, false),
    (b"SOURCE_PORT", 1173, false),
    (b"SOURCE_PUBLIC_KEY_PATH", 1174, false),
    (b"SOURCE_RETRY_COUNT", 1175, false),
    (b"SOURCE_SSL", 1176, false),
    (b"SOURCE_SSL_CA", 1177, false),
    (b"SOURCE_SSL_CAPATH", 1178, false),
    (b"SOURCE_SSL_CERT", 1179, false),
    (b"SOURCE_SSL_CIPHER", 1180, false),
    (b"SOURCE_SSL_CRL", 1181, false),
    (b"SOURCE_SSL_CRLPATH", 1182, false),
    (b"SOURCE_SSL_KEY", 1183, false),
    (b"SOURCE_SSL_VERIFY_SERVER_CERT", 1184, false),
    (b"SOURCE_TLS_CIPHERSUITES", 1185, false),
    (b"SOURCE_TLS_VERSION", 1186, false),
    (b"SOURCE_USER", 1187, false),
    (b"SOURCE_ZSTD_COMPRESSION_LEVEL", 1188, false),
    (b"SPATIAL", 774, false),
    (b"SPECIFIC", 775, false),
    (b"SQL", 788, false),
    (b"SQLEXCEPTION", 776, false),
    (b"SQLSTATE", 777, false),
    (b"SQLWARNING", 778, false),
    (b"SQL_AFTER_GTIDS", 779, false),
    (b"SQL_AFTER_MTS_GAPS", 780, false),
    (b"SQL_BEFORE_GTIDS", 781, false),
    (b"SQL_BIG_RESULT", 782, false),
    (b"SQL_BUFFER_RESULT", 783, false),
    (b"SQL_CALC_FOUND_ROWS", 785, false),
    (b"SQL_NO_CACHE", 786, false),
    (b"SQL_SMALL_RESULT", 787, false),
    (b"SQL_THREAD", 789, false),
    (b"SQL_TSI_DAY", 375, false),
    (b"SQL_TSI_HOUR", 481, false),
    (b"SQL_TSI_MINUTE", 594, false),
    (b"SQL_TSI_MONTH", 601, false),
    (b"SQL_TSI_QUARTER", 682, false),
    (b"SQL_TSI_SECOND", 746, false),
    (b"SQL_TSI_WEEK", 887, false),
    (b"SQL_TSI_YEAR", 905, false),
    (b"SRID", 961, false),
    (b"SSL", 790, false),
    (b"STACKED", 791, false),
    (b"START", 794, false),
    (b"STARTING", 792, false),
    (b"STARTS", 793, false),
    (b"STATS_AUTO_RECALC", 795, false),
    (b"STATS_PERSISTENT", 796, false),
    (b"STATS_SAMPLE_PAGES", 797, false),
    (b"STATUS", 798, false),
    (b"STD", 800, true),
    (b"STDDEV", 800, true),
    (b"STDDEV_POP", 800, true),
    (b"STDDEV_SAMP", 799, true),
    (b"STOP", 801, false),
    (b"STORAGE", 802, false),
    (b"STORED", 803, false),
    (b"STRAIGHT_JOIN", 804, false),
    (b"STREAM", 997, false),
    (b"STRING", 805, false),
    (b"ST_COLLECT", 1189, true),
    (b"SUBCLASS_ORIGIN", 806, false),
    (b"SUBDATE", 807, true),
    (b"SUBJECT", 808, false),
    (b"SUBPARTITION", 810, false),
    (b"SUBPARTITIONS", 809, false),
    (b"SUBSTR", 811, true),
    (b"SUBSTRING", 811, true),
    (b"SUM", 812, true),
    (b"SUPER", 813, false),
    (b"SUSPEND", 814, false),
    (b"SWAPS", 815, false),
    (b"SWITCHES", 816, false),
    (b"SYSDATE", 817, true),
    (b"SYSTEM", 964, false),
    (b"SYSTEM_USER", 867, true),
    (b"TABLE", 821, false),
    (b"TABLES", 818, false),
    (b"TABLESPACE", 819, false),
    (b"TABLE_CHECKSUM", 822, false),
    (b"TABLE_NAME", 823, false),
    (b"TEMPORARY", 824, false),
    (b"TEMPTABLE", 825, false),
    (b"TERMINATED", 826, false),
    (b"TEXT", 828, false),
    (b"THAN", 829, false),
    (b"THEN", 830, false),
    (b"THREAD_PRIORITY", 962, false),
    (b"TIES", 951, false),
    (b"TIME", 834, false),
    (b"TIMESTAMP", 831, false),
    (b"TIMESTAMPADD", 832, false),
    (b"TIMESTAMPDIFF", 833, false),
    (b"TINYBLOB", 835, false),
    (b"TINYINT", 836, false),
    (b"TINYTEXT", 837, false),
    (b"TLS", 1152, false),
    (b"TO", 838, false),
    (b"TRAILING", 839, false),
    (b"TRANSACTION", 840, false),
    (b"TRIGGER", 842, false),
    (b"TRIGGERS", 841, false),
    (b"TRIM", 843, true),
    (b"TRUE", 844, false),
    (b"TRUNCATE", 845, false),
    (b"TYPE", 847, false),
    (b"TYPES", 846, false),
    (b"UNBOUNDED", 952, false),
    (b"UNCOMMITTED", 850, false),
    (b"UNDEFINED", 851, false),
    (b"UNDO", 855, false),
    (b"UNDOFILE", 853, false),
    (b"UNDO_BUFFER_SIZE", 854, false),
    (b"UNICODE", 856, false),
    (b"UNINSTALL", 857, false),
    (b"UNION", 858, false),
    (b"UNIQUE", 859, false),
    (b"UNKNOWN", 860, false),
    (b"UNLOCK", 861, false),
    (b"UNREGISTER", 1196, false),
    (b"UNSIGNED", 862, false),
    (b"UNTIL", 863, false),
    (b"UPDATE", 864, false),
    (b"UPGRADE", 865, false),
    (b"URL", 1202, false),
    (b"USAGE", 866, false),
    (b"USE", 869, false),
    (b"USER", 867, false),
    (b"USER_RESOURCES", 723, false),
    (b"USE_FRM", 868, false),
    (b"USING", 870, false),
    (b"UTC_DATE", 871, false),
    (b"UTC_TIME", 873, false),
    (b"UTC_TIMESTAMP", 872, false),
    (b"VALIDATION", 874, false),
    (b"VALUE", 876, false),
    (b"VALUES", 875, false),
    (b"VARBINARY", 877, false),
    (b"VARCHAR", 878, false),
    (b"VARCHARACTER", 878, false),
    (b"VARIABLES", 879, false),
    (b"VARIANCE", 880, true),
    (b"VARYING", 881, false),
    (b"VAR_POP", 880, true),
    (b"VAR_SAMP", 882, true),
    (b"VCPU", 965, false),
    (b"VIEW", 883, false),
    (b"VIRTUAL", 884, false),
    (b"VISIBLE", 912, false),
    (b"WAIT", 885, false),
    (b"WARNINGS", 886, false),
    (b"WEEK", 887, false),
    (b"WEIGHT_STRING", 888, false),
    (b"WHEN", 889, false),
    (b"WHERE", 890, false),
    (b"WHILE", 891, false),
    (b"WINDOW", 953, false),
    (b"WITH", 892, false),
    (b"WITHOUT", 895, false),
    (b"WORK", 896, false),
    (b"WRAPPER", 897, false),
    (b"WRITE", 898, false),
    (b"X509", 899, false),
    (b"XA", 900, false),
    (b"XID", 901, false),
    (b"XML", 902, false),
    (b"XOR", 903, false),
    (b"YEAR", 905, false),
    (b"YEAR_MONTH", 904, false),
    (b"ZEROFILL", 906, false),
    (b"ZONE", 1157, false),
    (b"||", 644, false),
];

fn mysql_digest_ascii_cmp(left: &[u8], right: &[u8]) -> core::cmp::Ordering {
    for (&left, &right) in left.iter().zip(right) {
        let order = left.to_ascii_uppercase().cmp(&right.to_ascii_uppercase());
        if order != core::cmp::Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

fn mysql_digest_symbol(word: &[u8]) -> Option<(u16, bool)> {
    MYSQL_DIGEST_SYMBOLS
        .binary_search_by(|entry| mysql_digest_ascii_cmp(entry.0, word))
        .ok()
        .map(|index| {
            let (_, token, function) = MYSQL_DIGEST_SYMBOLS[index];
            (token, function)
        })
}

fn mysql_digest_start_expr(token: u16) -> bool {
    // Exact set_start_expr_token set from fixed gen_lex_token.cc.
    matches!(
        token,
        40 | 44
            | 423
            | 280
            | 793
            | 410
            | 380
            | 729
            | 485
            | 406
            | 309
            | 889
            | 891
            | 863
            | 748
            | 645
            | 642
            | 903
            | 274
            | 273
            | 622
            | 288
            | 530
            | 697
            | 124
            | 38
            | 759
            | 760
            | 43
            | 45
            | 500
            | 42
            | 47
            | 37
            | 396
            | 600
            | 94
    )
}

/// The raw input shape of the SAME tokenizer State/next_token FSM. Its caller
/// supplies a current client charset and SQL-mode dialect per statement.
pub struct MySqlTokenCursor<'a> {
    tokenizer: Tokenizer<'a>,
    state: State<'a>,
    previous: Option<Token>,
    cpp: Vec<SqlCppPiece>,
    cpp_admission: Option<SqlSourceAdmission>,
    digest: Option<MySqlDigestCollector>,
}

/// One token produced by the same tokenizer FSM, with original byte and CPP
/// provenance. All ranges share the cursor's actual admitted source/ledger.
pub struct MySqlSourceToken {
    /// Grammar token and logical location, without a whole-source decoded mirror.
    pub token: TokenWithSpan,
    /// Checked original byte range consumed for this token.
    pub source: SqlSourceOccurrence,
    /// Half-open contribution indices in the cursor's CPP ledger, including
    /// real inserted spaces rather than text regenerated from this token.
    pub cpp: SqlCppPieceRange,
    /// Native identifier text facts when present, before the grammar assigns
    /// the actual system, validated, alias or other consumer role.
    pub identifier_text: Option<MySqlText>,
    /// Same original comment consumer's bound typed program, never a rescan.
    pub optimizer_hint: Option<crate::ast::OptimizerHintStyle>,
}

impl<'a> MySqlTokenCursor<'a> {
    pub(crate) fn begin_canonical_digest(&mut self) -> Result<(), TokenizerError> {
        let raw = self.state.raw.as_ref().expect("raw cursor");
        if let Some(error) = raw.first_error(self.state.location()) {
            return Err(error);
        }
        let Some(config) = raw
            .adapter
            .canonical_digest_config()
            .filter(|config| config.capacity != 0)
        else {
            return Ok(());
        };
        if self.digest.is_some() {
            return Err(raw.record_error(TokenizerError::source(
                "canonical collection was started twice".into(),
                self.state.location(),
            )));
        }
        let Some(capacity) = config
            .capacity
            .checked_add(core::mem::size_of::<MySqlCanonicalDigest>())
            .and_then(|capacity| capacity.checked_add(2 * core::mem::size_of::<usize>()))
        else {
            self.digest = Some(MySqlDigestCollector::unavailable(
                config,
                MySqlDigestUnavailable::Resource,
            ));
            return Ok(());
        };
        // Observation ResourceExhausted is not a new business parse failure.
        // Other real controls retain the same first-error identity.
        let admission = match raw.adapter.admit(capacity) {
            Ok(admission) => admission,
            Err(error) if error.kind == MySqlSourceErrorKind::ResourceExhausted => {
                self.digest = Some(MySqlDigestCollector::unavailable(
                    config,
                    MySqlDigestUnavailable::Resource,
                ));
                return Ok(());
            }
            Err(error) => {
                return Err(raw.record_error(TokenizerError::source(error, self.state.location())));
            }
        };
        let mut tokens = Vec::new();
        if tokens.try_reserve_exact(config.capacity).is_err() || tokens.capacity() > config.capacity
        {
            self.digest = Some(MySqlDigestCollector::unavailable(
                config,
                MySqlDigestUnavailable::Resource,
            ));
            return Ok(());
        }
        self.digest = Some(MySqlDigestCollector {
            config,
            tokens,
            full: false,
            last_identifier: 0,
            last_period_end: None,
            pending_word: None,
            pending_period: None,
            variable_role: 0,
            variable_end: 0,
            hintable: false,
            last_null_source: None,
            last_reduced_source: None,
            unavailable: None,
            admission: Some(admission),
        });
        Ok(())
    }

    pub(crate) fn reduce_canonical_null(&mut self, source: core::ops::Range<usize>) {
        if let Some(digest) = self.digest.as_mut() {
            digest.reduce_null(source);
        }
    }

    pub(crate) fn canonical_reduction_boundary(&self) -> Option<usize> {
        self.digest.as_ref().and_then(|digest| {
            digest
                .last_reduced_source
                .as_ref()
                .map(|source| source.start)
        })
    }

    pub(crate) fn reject_canonical_rollback(&mut self) {
        if let Some(digest) = self.digest.as_mut() {
            digest.reject(MySqlDigestUnavailable::GrammarLookahead);
        }
    }

    pub(crate) fn rewind_canonical_digest(&mut self, start: usize) {
        if self
            .canonical_reduction_boundary()
            .is_some_and(|source| source >= start)
        {
            self.reject_canonical_rollback();
        }
    }

    pub(crate) fn discard_canonical_empty_statement(&mut self) {
        if let Some(digest) = self.digest.as_mut() {
            digest.tokens.clear();
            digest.full = false;
            digest.last_identifier = 0;
            digest.last_period_end = None;
            digest.pending_word = None;
            digest.pending_period = None;
            digest.variable_role = 0;
            digest.variable_end = 0;
            digest.hintable = false;
            digest.last_null_source = None;
            digest.last_reduced_source = None;
        }
    }

    pub(crate) fn finish_canonical_digest(
        &mut self,
    ) -> Result<Option<MySqlCanonicalDigestReceipt>, TokenizerError> {
        let raw = self.state.raw.as_ref().expect("raw cursor");
        if let Some(error) = raw.first_error(self.state.location()) {
            return Err(error);
        }
        let Some(mut digest) = self.digest.take() else {
            return Ok(None);
        };
        digest.finish_period(None);
        digest.flush_word(None);
        if !digest.full && digest.unavailable.is_none() && digest.variable_role == 1 {
            digest.push(1100, false);
        } else if !digest.full && digest.unavailable.is_none() && digest.variable_role == 2 {
            digest.reject(MySqlDigestUnavailable::VariableRole);
        }
        if !digest.full && digest.unavailable.is_none() && digest.previous(1) == 59 {
            digest.tokens.truncate(digest.tokens.len() - 2);
        }
        let hash = if digest.unavailable.is_none() {
            let hash = raw.adapter.canonical_digest_hash(&digest.tokens);
            if hash.is_none() {
                digest.unavailable = Some(MySqlDigestUnavailable::HashService);
            }
            hash
        } else {
            None
        };
        // Unavailable/resource facts require no new payload allocation: the
        // original admitted source keeps absence explicit. Other unavailable
        // branches already have the same original payload/header lease.
        if digest.admission.is_none() {
            return Ok(Some(MySqlCanonicalDigestReceipt::Unavailable(
                digest
                    .unavailable
                    .expect("unadmitted digest is unavailable"),
            )));
        }
        Ok(Some(MySqlCanonicalDigestReceipt::Collected(Arc::new(
            MySqlCanonicalDigest {
                tokens: digest.tokens,
                hash,
                charset_number: digest.config.charset_number,
                full: digest.full,
                unavailable: digest.unavailable,
                _admission: digest.admission,
            },
        ))))
    }

    /// Reports a comment recognized by this cursor's actual byte FSM. Reading
    /// this fact neither demands another token nor establishes a successful
    /// terminal: the parser must still check the same first-error ledger and
    /// actual EOF. Source bytes, text containing markers, or filled tokens do
    /// not issue this fact, and it grants no SQL/ACL/query-id capability.
    pub fn has_comment(&self) -> bool {
        self.state.raw.as_ref().expect("raw cursor").has_comment
    }

    /// Forwards only a completed native grammar reduction to this cursor's
    /// actual source adapter. No stored AST flag or later execution emits it.
    pub fn json_table_on_error_before_empty(&self) {
        self.state
            .raw
            .as_ref()
            .expect("MySQL cursor owns admitted byte input")
            .adapter
            .json_table_on_error_before_empty();
    }
    /// Forward a completed character grammar reduction; no stored AST flag
    /// or later Item VALUE issues this condition.
    pub fn deprecated_character_syntax(&self, syntax: &str, replacement: &str) {
        self.state
            .raw
            .as_ref()
            .expect("raw cursor")
            .adapter
            .deprecated_character_syntax(syntax, replacement);
    }
    /// Forward this actual charset-name production on the same Source owner.
    pub fn deprecated_character_set(&self, charset: &str) {
        self.state
            .raw
            .as_ref()
            .expect("raw cursor")
            .adapter
            .deprecated_character_set(charset);
    }
    /// Forward the actual collation-name production without storing a flag.
    pub fn deprecated_collation(&self, collation: &str) {
        self.state
            .raw
            .as_ref()
            .expect("raw cursor")
            .adapter
            .deprecated_collation(collation);
    }
    /// Forward the completed numeric grammar production on this Source.
    pub fn deprecated_numeric_syntax(&self, syntax: &str) {
        self.state
            .raw
            .as_ref()
            .expect("raw cursor")
            .adapter
            .deprecated_numeric_syntax(syntax);
    }
    /// Creates a lazy byte cursor borrowing the current native adapter and
    /// mode-aware MySQL dialect over an already attached source occurrence.
    /// Rejects detached input, other dialects and client charsets whose minimum
    /// width is not one. It does not tokenize the tail or create an executor.
    /// Cancellation is checked by the same source FSM and preserves its first
    /// actual adapter/error fact instead of overwriting it with later probes.
    pub fn new(
        dialect: &'a dyn Dialect,
        source: SqlSourceOccurrence,
        adapter: &'a dyn MySqlSourceAdapter,
        cancellation: Option<CancellationCheck>,
    ) -> Result<Self, TokenizerError> {
        source.bytes().map_err(|message| TokenizerError {
            source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
            message,
            location: Location::new(1, 1),
        })?;
        if !dialect.is::<MySqlDialect>() || adapter.client_min_width() != 1 {
            return Err(TokenizerError {
                source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                message: "MySQL source requires an admitted single-minimum-byte client charset"
                    .into(),
                location: Location::new(1, 1),
            });
        }
        Ok(Self {
            tokenizer: Tokenizer::new(dialect, ""),
            state: State {
                raw: Some(MySqlByteState {
                    source,
                    adapter,
                    offset: 0,
                    peeked_ascii: None,
                    in_version_comment: false,
                    has_comment: false,
                    token_start: 0,
                    cpp_space: false,
                    allocations: Vec::new(),
                    allocations_admission: None,
                    identifier_text: None,
                    empty_hint_comment: false,
                    digest_comment_barrier: false,
                    error: core::cell::RefCell::new(None),
                    cancellation: cancellation.clone(),
                    work: core::cell::Cell::new(0),
                    location: Location::new(1, 1),
                    token_location: Location::new(1, 1),
                }),
                peekable: "".chars().peekable(),
                line: 1,
                col: 1,
                cancellation,
                characters_since_cancellation_check: 0,
                cancelled: false,
            },
            previous: None,
            cpp: Vec::new(),
            cpp_admission: None,
            digest: None,
        })
    }

    /// Borrows the current adapter to acquire a capacity lease before a caller
    /// allocation, translating failure at the cursor's actual source location.
    pub fn admit(&self, capacity: usize) -> Result<SqlSourceAdmission, TokenizerError> {
        let location = self.state.location();
        let raw = self.state.raw.as_ref().expect("raw cursor");
        if let Some(error) = raw.first_error(location) {
            return Err(error);
        }
        raw.adapter
            .admit(capacity)
            .map_err(|error| raw.record_error(TokenizerError::source(error, location)))
    }

    pub(crate) fn admit_source(
        &self,
        capacity: usize,
    ) -> Result<SqlSourceAdmission, TokenizerError> {
        let location = self.state.location();
        let raw = self.state.raw.as_ref().expect("raw cursor");
        if let Some(error) = raw.first_error(location) {
            return Err(error);
        }
        raw.adapter
            .admit_source(capacity)
            .map_err(|error| raw.record_error(TokenizerError::source(error, location)))
    }

    /// Parser-owned source structures use the existing admission ledger. The
    /// lease survives with the bundle, including failures before publication.
    pub(crate) fn admit_retained(&mut self, bytes: usize) -> Result<(), TokenizerError> {
        let location = self.state.location();
        let raw = self.state.raw.as_mut().expect("raw cursor");
        if let Some(error) = raw.first_error(location) {
            return Err(error);
        }
        let admission = raw
            .adapter
            .admit(bytes)
            .map_err(|error| raw.record_error(TokenizerError::source(error, location)))?;
        raw.retain_admission(admission)
            .map_err(|message| raw.token_error(message, location))
    }

    pub(crate) fn system_text(&mut self, text: &MySqlText) -> Result<String, TokenizerError> {
        let location = self.state.location();
        let raw = self.state.raw.as_mut().expect("raw cursor");
        if let Some(error) = raw.first_error(location) {
            return Err(error);
        }
        let (value, admission) = raw
            .adapter
            .system_text(text)
            .map_err(|error| raw.record_error(TokenizerError::source(error, location)))?;
        raw.retain_admission(admission)
            .map_err(|message| raw.token_error(message, location))?;
        Ok(value)
    }

    pub(crate) fn system_text_value(
        &mut self,
        text: &MySqlText,
    ) -> Result<(Vec<u8>, SqlSourceAdmission), TokenizerError> {
        let location = self.state.location();
        let raw = self.state.raw.as_mut().expect("raw cursor");
        if let Some(error) = raw.first_error(location) {
            return Err(error);
        }
        let (value, admission) = raw
            .adapter
            .system_text_value(text)
            .map_err(|error| raw.record_error(TokenizerError::source(error, location)))?;
        raw.retain_admission(admission.clone())
            .map_err(|message| raw.token_error(message, location))?;
        Ok((value, admission))
    }

    pub(crate) fn json_attribute(
        &mut self,
        text: &MySqlText,
    ) -> Result<(Vec<u8>, SqlSourceAdmission), (TokenizerError, Option<SqlSourceAdmission>)> {
        self.admitted_system_text(text, true)
    }

    pub(crate) fn projection_alias_text(
        &mut self,
        text: &MySqlText,
    ) -> Result<(Vec<u8>, SqlSourceAdmission), (TokenizerError, Option<SqlSourceAdmission>)> {
        self.admitted_system_text(text, false)
    }

    fn admitted_system_text(
        &mut self,
        text: &MySqlText,
        json_attribute: bool,
    ) -> Result<(Vec<u8>, SqlSourceAdmission), (TokenizerError, Option<SqlSourceAdmission>)> {
        let location = self.state.location();
        let raw = self.state.raw.as_mut().expect("raw cursor");
        if let Some(MySqlCursorFailure::Admitted(original, _)) = raw.error.borrow().as_ref() {
            let admission = raw
                .adapter
                .admit(original.message.len())
                .map_err(|error| (TokenizerError::source(error, location), None))?;
            return Err((original.clone(), Some(admission)));
        }
        if let Some(error) = raw.first_error(location) {
            return Err((error, None));
        }
        let result = if json_attribute {
            raw.adapter.json_attribute(text)
        } else {
            raw.adapter.projection_alias_text(text)
        };
        let (value, admission) = match result {
            Ok(value) => value,
            Err((error, Some(admission))) => {
                let error = TokenizerError::source(error, location);
                // The original first-error slot owns its own actual String
                // copy. Admit before Clone; do not share the returning copy's
                // grant as if it paid two allocations.
                let copy = match raw.adapter.admit(error.message.len()) {
                    Ok(copy) => copy,
                    Err(refusal) => {
                        *raw.error.borrow_mut() =
                            Some(MySqlCursorFailure::Admitted(error, admission));
                        return Err((TokenizerError::source(refusal, location), None));
                    }
                };
                *raw.error.borrow_mut() = Some(MySqlCursorFailure::Admitted(error.clone(), copy));
                return Err((error, Some(admission)));
            }
            Err((error, None)) => {
                return Err((
                    raw.record_error(TokenizerError::source(error, location)),
                    None,
                ));
            }
        };
        raw.retain_admission(admission.clone())
            .map_err(|message| (raw.token_error(message, location), None))?;
        Ok((value, admission))
    }

    /// Borrows the same actual token Source and connection-domain adapter.
    /// Admission remains in the original parser bundle, including conversion
    /// failure/abandonment; no second cursor or converted-text cache is made.
    pub(crate) fn text_string(&mut self, text: &MySqlText) -> Result<Vec<u8>, TokenizerError> {
        let location = self.state.location();
        let raw = self.state.raw.as_mut().expect("raw cursor");
        if let Some(error) = raw.first_error(location) {
            return Err(error);
        }
        let [segment] = text.segments() else {
            return Err(raw.token_error(
                "TEXT_STRING requires one actual literal token".into(),
                location,
            ));
        };
        let original = segment.source();
        let original_range = original.range();
        let current_range = raw.source.range();
        let actual_source = original
            .owner()
            .zip(raw.source.owner())
            .is_some_and(|(original, current)| Arc::ptr_eq(original, current));
        let consumed_end = current_range.start.checked_add(raw.offset);
        if !actual_source
            || original_range.start < current_range.start
            || original_range.end > current_range.end
            || consumed_end.is_none_or(|end| original_range.end > end)
            || segment.introducer().is_some()
            || !matches!(segment.quote(), '\'' | '"')
        {
            return Err(raw.token_error(
                "TEXT_STRING is not the current consumed literal Source".into(),
                location,
            ));
        }
        text.bytes()
            .map_err(|message| raw.token_error(message, location))?;
        let (value, admission) = raw
            .adapter
            .text_string(text)
            .map_err(|error| raw.record_error(TokenizerError::source(error, location)))?;
        raw.retain_admission(admission)
            .map_err(|message| raw.token_error(message, location))?;
        Ok(value)
    }

    /// Demands the next token using the existing FSM and native unit facts.
    /// Multibyte units are consumed before ASCII syntax decisions, so a trail
    /// quote/backslash is not independently reclassified. Real adapter errors
    /// and cooperative cancellation propagate through the first-error ledger;
    /// `None` means actual exhaustion, not swallowed conversion failure.
    /// Original comments/CPP contributions remain in the same admitted ledger.
    pub fn next_token(&mut self) -> Result<Option<MySqlSourceToken>, TokenizerError> {
        let location = self.state.location();
        if let Some(error) = self
            .state
            .raw
            .as_ref()
            .expect("raw cursor")
            .first_error(location)
        {
            return Err(error);
        }
        let result = (|| {
            if self
                .state
                .cancellation
                .as_ref()
                .is_some_and(|check| check())
            {
                return Err(TokenizerError {
                    source_kind: MySqlSourceErrorKind::Cancelled,
                    message: QUERY_CANCELLED_MESSAGE.into(),
                    location: self.state.location(),
                });
            }
            let raw = self.state.raw.as_ref().expect("raw cursor");
            let start = raw.offset;
            let location = self.state.location();
            self.state
                .raw
                .as_mut()
                .expect("raw cursor")
                .digest_comment_barrier = false;
            let token_result = self
                .tokenizer
                .next_token(&mut self.state, self.previous.as_ref());
            if let Some(error) = self
                .state
                .raw
                .as_ref()
                .expect("raw cursor")
                .first_error(location)
            {
                return Err(error);
            }
            let token = token_result?;
            if self.state.cancelled {
                return Err(TokenizerError {
                    source_kind: MySqlSourceErrorKind::Cancelled,
                    message: QUERY_CANCELLED_MESSAGE.into(),
                    location,
                });
            }
            let Some(token) = token else { return Ok(None) };
            let raw = self.state.raw.as_ref().expect("raw cursor");
            let source = raw
                .occurrence(raw.token_start.max(start), raw.offset)
                .map_err(|message| TokenizerError {
                    message,
                    location,
                    source_kind: MySqlSourceErrorKind::Message,
                })?;
            let cpp_space = raw.cpp_space;
            let identifier_text = raw.identifier_text.clone();
            let empty_hint_comment = raw.empty_hint_comment;
            let digest_comment_barrier = raw.digest_comment_barrier;
            let digest_adapter = raw.adapter;
            let token_location = raw.token_location;
            let end_location = raw.location;
            self.state.line = end_location.line;
            self.state.col = end_location.column;
            let begin = self.cpp.len();
            // Capacity is charged before reserve; the retained guard travels with
            // the one statement ledger, never discarded while its AST escapes.
            if self.cpp.len() == self.cpp.capacity() {
                let capacity =
                    self.cpp
                        .capacity()
                        .max(16)
                        .checked_mul(2)
                        .ok_or_else(|| TokenizerError {
                            source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                            message: "CPP capacity overflow".into(),
                            location,
                        })?;
                let admission = self.admit(
                    capacity
                        .checked_mul(core::mem::size_of::<SqlCppPiece>())
                        .ok_or_else(|| TokenizerError {
                            source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                            message: "CPP allocation overflow".into(),
                            location,
                        })?,
                )?;
                self.cpp.reserve_exact(capacity - self.cpp.len());
                self.cpp_admission = Some(admission);
            }
            self.cpp.push(if cpp_space {
                SqlCppPiece::Space
            } else {
                SqlCppPiece::Source(source.clone())
            });
            let eligible_hint = matches!(&token, Token::Whitespace(Whitespace::MySqlComment(comment))
                    if comment.bytes().is_ok_and(|bytes| bytes.starts_with(b"/*+")))
                && matches!(&self.previous, Some(Token::Word(word)) if matches!(word.keyword, Keyword::SELECT | Keyword::INSERT | Keyword::REPLACE | Keyword::UPDATE | Keyword::DELETE));
            let optimizer_hint = if eligible_hint {
                if let Some(digest) = self.digest.as_mut() {
                    digest.flush_word(None);
                }
                let mut started = false;
                digest_adapter
                    .consume_optimizer_hint(&source, &mut |kind, bytes| {
                        if let Some(digest) = self.digest.as_mut() {
                            digest.hint_token(kind, bytes, &mut started);
                        }
                        Ok(())
                    })
                    .map_err(|error| TokenizerError::source(error, location))?
            } else {
                None
            };
            if matches!(&token, Token::Whitespace(Whitespace::MySqlComment(_))) {
                if let Some(Token::Word(word)) = self.previous.as_mut() {
                    word.keyword = Keyword::NoKeyword;
                }
            }
            // This FSM consults only the previous Word/Period class. Do not clone
            // an unbounded literal payload just to retain that lexical relation.
            if !matches!(&token, Token::Whitespace(_)) {
                self.previous = match &token {
                    Token::Word(word) => Some(Token::Word(Word {
                        value: String::new(),
                        quote_style: None,
                        keyword: word.keyword,
                    })),
                    Token::Period => Some(Token::Period),
                    _ => None,
                };
            }
            if let Some(digest) = self.digest.as_mut() {
                digest.observe(
                    &token,
                    &source,
                    identifier_text.as_ref(),
                    digest_adapter,
                    empty_hint_comment,
                    digest_comment_barrier,
                    optimizer_hint.is_some(),
                );
            }
            Ok(Some(MySqlSourceToken {
                token: TokenWithSpan::new(token, token_location.span_to(end_location)),
                source,
                cpp: SqlCppPieceRange {
                    start: begin,
                    end: self.cpp.len(),
                },
                identifier_text,
                optimizer_hint,
            }))
        })();
        result.map_err(|error| {
            self.state
                .raw
                .as_ref()
                .expect("raw cursor")
                .record_error(error)
        })
    }

    /// Borrows this cursor's attached input occurrence and its original owner.
    pub fn source(&self) -> &SqlSourceOccurrence {
        &self.state.raw.as_ref().expect("raw cursor").source
    }
    /// Returns consumed source bytes relative to [`Self::source`], not a decoded
    /// character count or an index into the CPP piece ledger.
    pub fn consumed(&self) -> usize {
        self.state.raw.as_ref().expect("raw cursor").offset
    }
    /// Counts the cursor's retained allocation/container guards so a returned
    /// statement bundle can admit its guard storage before transferring them.
    /// This count does not measure source bytes or grant SQL capability.
    pub fn retained_admission_count(&self) -> usize {
        self.state
            .raw
            .as_ref()
            .expect("raw cursor")
            .allocations
            .len()
            + usize::from(self.cpp_admission.is_some())
            + usize::from(
                self.state
                    .raw
                    .as_ref()
                    .expect("raw cursor")
                    .allocations_admission
                    .is_some(),
            )
    }
    /// Consumes the cursor and transfers its actual CPP ledger with all retained
    /// allocation guards. Tuple order is pieces, individual allocation guards,
    /// CPP Vec-capacity guard, and allocation-guard Vec-capacity guard. Retaining
    /// these with the parsed bundle keeps charges alive; Drop releases them.
    pub fn into_cpp(
        self,
    ) -> (
        Vec<SqlCppPiece>,
        Vec<SqlSourceAdmission>,
        Option<SqlSourceAdmission>,
        Option<SqlSourceAdmission>,
    ) {
        let raw = self.state.raw.expect("raw cursor");
        (
            self.cpp,
            raw.allocations,
            self.cpp_admission,
            raw.allocations_admission,
        )
    }
}

/// Represents how many quote characters enclose a string literal.
#[derive(Copy, Clone)]
enum NumStringQuoteChars {
    /// e.g. `"abc"`, `'abc'`, `r'abc'`
    One,
    /// e.g. `"""abc"""`, `'''abc'''`, `r'''abc'''`
    Many(NonZeroU8),
}

/// Settings for tokenizing a quoted string literal.
struct TokenizeQuotedStringSettings {
    /// The character used to quote the string.
    quote_style: char,
    /// Represents how many quotes characters enclose the string literal.
    num_quote_chars: NumStringQuoteChars,
    /// The number of opening quotes left to consume, before parsing
    /// the remaining string literal.
    /// For example: given initial string `"""abc"""`. If the caller has
    /// already parsed the first quote for some reason, then this value
    /// is set to 1, flagging to look to consume only 2 leading quotes.
    num_opening_quotes_to_consume: u8,
    /// True if the string uses backslash escaping of special characters
    /// e.g `'abc\ndef\'ghi'
    backslash_escape: bool,
}

/// SQL Tokenizer
pub struct Tokenizer<'a> {
    dialect: &'a dyn Dialect,
    query: &'a str,
    /// If true (the default), the tokenizer will un-escape literal
    /// SQL strings See [`Tokenizer::with_unescape`] for more details.
    unescape: bool,
    cancellation: Option<CancellationCheck>,
}

impl<'a> Tokenizer<'a> {
    /// Create a new SQL tokenizer for the specified SQL statement
    ///
    /// ```
    /// # use sqlparser::tokenizer::{Token, Whitespace, Tokenizer};
    /// # use sqlparser::dialect::GenericDialect;
    /// # let dialect = GenericDialect{};
    /// let query = r#"SELECT 'foo'"#;
    ///
    /// // Parsing the query
    /// let tokens = Tokenizer::new(&dialect, &query).tokenize().unwrap();
    ///
    /// assert_eq!(tokens, vec![
    ///   Token::make_word("SELECT", None),
    ///   Token::Whitespace(Whitespace::Space),
    ///   Token::SingleQuotedString("foo".to_string()),
    /// ]);
    pub fn new(dialect: &'a dyn Dialect, query: &'a str) -> Self {
        Self {
            dialect,
            query,
            unescape: true,
            cancellation: None,
        }
    }

    /// Installs a cooperative cancellation probe for long token streams.
    pub fn with_cancellation(mut self, cancellation: CancellationCheck) -> Self {
        self.cancellation = Some(cancellation);
        self
    }

    /// Set unescape mode
    ///
    /// When true (default) the tokenizer unescapes literal values
    /// (for example, `""` in SQL is unescaped to the literal `"`).
    ///
    /// When false, the tokenizer provides the raw strings as provided
    /// in the query.  This can be helpful for programs that wish to
    /// recover the *exact* original query text without normalizing
    /// the escaping
    ///
    /// # Example
    ///
    /// ```
    /// # use sqlparser::tokenizer::{Token, Tokenizer};
    /// # use sqlparser::dialect::GenericDialect;
    /// # let dialect = GenericDialect{};
    /// let query = r#""Foo "" Bar""#;
    /// let unescaped = Token::make_word(r#"Foo " Bar"#, Some('"'));
    /// let original  = Token::make_word(r#"Foo "" Bar"#, Some('"'));
    ///
    /// // Parsing with unescaping (default)
    /// let tokens = Tokenizer::new(&dialect, &query).tokenize().unwrap();
    /// assert_eq!(tokens, vec![unescaped]);
    ///
    /// // Parsing with unescape = false
    /// let tokens = Tokenizer::new(&dialect, &query)
    ///    .with_unescape(false)
    ///    .tokenize().unwrap();
    /// assert_eq!(tokens, vec![original]);
    /// ```
    pub fn with_unescape(mut self, unescape: bool) -> Self {
        self.unescape = unescape;
        self
    }

    /// Tokenize the statement and produce a vector of tokens
    pub fn tokenize(&mut self) -> Result<Vec<Token>, TokenizerError> {
        let twl = self.tokenize_with_location()?;
        Ok(twl.into_iter().map(|t| t.token).collect())
    }

    /// Tokenize the statement and produce a vector of tokens with location information
    pub fn tokenize_with_location(&mut self) -> Result<Vec<TokenWithSpan>, TokenizerError> {
        let mut tokens: Vec<TokenWithSpan> = vec![];
        self.tokenize_with_location_into_buf(&mut tokens)
            .map(|_| tokens)
    }

    /// Tokenize the statement and append tokens with location information into the provided buffer.
    /// If an error is thrown, the buffer will contain all tokens that were successfully parsed before the error.
    pub fn tokenize_with_location_into_buf(
        &mut self,
        buf: &mut Vec<TokenWithSpan>,
    ) -> Result<(), TokenizerError> {
        self.tokenize_with_location_into_buf_with_mapper(buf, |token| token)
    }

    /// Tokenize the statement and produce a vector of tokens, mapping each token
    /// with provided `mapper`
    pub fn tokenize_with_location_into_buf_with_mapper(
        &mut self,
        buf: &mut Vec<TokenWithSpan>,
        mut mapper: impl FnMut(TokenWithSpan) -> TokenWithSpan,
    ) -> Result<(), TokenizerError> {
        let mut state = State {
            raw: None,
            peekable: self.query.chars().peekable(),
            line: 1,
            col: 1,
            cancellation: self.cancellation.clone(),
            characters_since_cancellation_check: 0,
            cancelled: false,
        };

        let mut location = state.location();
        loop {
            if self.cancellation.as_ref().is_some_and(|check| check()) {
                return Err(TokenizerError {
                    source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                    message: QUERY_CANCELLED_MESSAGE.to_owned(),
                    location: state.location(),
                });
            }
            let token = self.next_token(&mut state, buf.last().map(|t| &t.token));
            if state.cancelled {
                return Err(TokenizerError {
                    source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                    message: QUERY_CANCELLED_MESSAGE.to_owned(),
                    location: state.location(),
                });
            }
            let Some(token) = token? else {
                break;
            };
            let span = location.span_to(state.location());

            // Check if this is a multiline comment hint that should be expanded
            match &token {
                Token::Whitespace(Whitespace::MultiLineComment(comment))
                    if self.dialect.supports_multiline_comment_hints()
                        && comment.starts_with('!') =>
                {
                    // Re-tokenize the hints and add them to the buffer
                    self.tokenize_comment_hints(comment, span, buf, &mut mapper)?;
                }
                _ => {
                    buf.push(mapper(TokenWithSpan { token, span }));
                }
            }

            location = state.location();
        }
        Ok(())
    }

    /// Re-tokenize optimizer hints from a multiline comment and add them to the buffer.
    /// For example, `/*!50110 KEY_BLOCK_SIZE = 1024*/` becomes tokens for `KEY_BLOCK_SIZE = 1024`
    fn tokenize_comment_hints(
        &self,
        comment: &str,
        span: Span,
        buf: &mut Vec<TokenWithSpan>,
        mut mapper: impl FnMut(TokenWithSpan) -> TokenWithSpan,
    ) -> Result<(), TokenizerError> {
        // Strip the leading '!' and any version digits (e.g., "50110")
        let hint_content = comment
            .strip_prefix('!')
            .unwrap_or(comment)
            .trim_start_matches(|c: char| c.is_ascii_digit());

        // If there's no content after stripping, nothing to tokenize
        if hint_content.is_empty() {
            return Ok(());
        }

        // Create a new tokenizer for the hint content
        let inner = Tokenizer::new(self.dialect, hint_content).with_unescape(self.unescape);

        // Create a state for tracking position within the hint
        let mut state = State {
            raw: None,
            peekable: hint_content.chars().peekable(),
            line: span.start.line,
            col: span.start.column,
            cancellation: self.cancellation.clone(),
            characters_since_cancellation_check: 0,
            cancelled: false,
        };

        // Tokenize the hint content and add tokens to the buffer
        let mut location = state.location();
        loop {
            let token = inner.next_token(&mut state, buf.last().map(|t| &t.token));
            if state.cancelled {
                return Err(TokenizerError {
                    source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                    message: QUERY_CANCELLED_MESSAGE.to_owned(),
                    location: state.location(),
                });
            }
            let Some(token) = token? else {
                break;
            };
            let token_span = location.span_to(state.location());
            buf.push(mapper(TokenWithSpan {
                token,
                span: token_span,
            }));
            location = state.location();
        }

        Ok(())
    }

    // Tokenize the identifier or keywords in `ch`
    fn tokenize_identifier_or_keyword(
        &self,
        ch: impl IntoIterator<Item = char>,
        chars: &mut State,
    ) -> Result<Option<Token>, TokenizerError> {
        if chars.raw.is_some() {
            let location = chars.location();
            let fail = |message| TokenizerError {
                message,
                location,
                source_kind: MySqlSourceErrorKind::Message,
            };
            let raw = chars.raw.as_mut().expect("raw State");
            while let Some(unit) = raw.unit_at(raw.offset).map_err(&fail)? {
                if !unit.identifier {
                    break;
                }
                raw.advance_bytes(usize::from(unit.width)).map_err(&fail)?;
            }
            let source = raw.occurrence(raw.token_start, raw.offset).map_err(&fail)?;
            let (name, admission) = raw
                .system_identifier(source.bytes().map_err(&fail)?, false)
                .map_err(&fail)?;
            raw.retain_admission(admission).map_err(&fail)?;
            raw.admit_text_domain(0).map_err(&fail)?;
            raw.identifier_text = Some(
                MySqlText::from_occurrence(
                    source.clone(),
                    source,
                    raw.adapter.client_charset().into(),
                    '\0',
                    None,
                )
                .map_err(&fail)?,
            );
            return Ok(Some(Token::make_word_owned(name, None)));
        }
        chars.next(); // consume the first char
        let ch: String = ch.into_iter().collect();
        let word = self.tokenize_word(ch, chars);

        // TODO: implement parsing of exponent here
        if word.chars().all(|x| x.is_ascii_digit() || x == '.') {
            let mut inner_state = State {
                raw: None,
                peekable: word.chars().peekable(),
                line: 0,
                col: 0,
                cancellation: None,
                characters_since_cancellation_check: 0,
                cancelled: false,
            };
            let mut s = peeking_take_while(&mut inner_state, |ch| matches!(ch, '0'..='9' | '.'));
            let s2 = peeking_take_while(chars, |ch| matches!(ch, '0'..='9' | '.'));
            s += s2.as_str();
            return Ok(Some(Token::Number(s, false)));
        }

        Ok(Some(Token::make_word_owned(word, None)))
    }

    /// Get the next token or return None
    fn mysql_source_token(
        &self,
        chars: &mut State,
        prev_token: Option<&Token>,
    ) -> Result<Option<Option<Token>>, TokenizerError> {
        let location = chars.location();
        let fail = |message: String| TokenizerError {
            message,
            location,
            source_kind: MySqlSourceErrorKind::Message,
        };
        let cancellation = chars.cancellation.clone();
        let mut work = 0_usize;
        let mut check = || -> Result<(), TokenizerError> {
            work += 1;
            if work.is_multiple_of(1024) && cancellation.as_ref().is_some_and(|probe| probe()) {
                return Err(TokenizerError {
                    source_kind: MySqlSourceErrorKind::Cancelled,
                    message: QUERY_CANCELLED_MESSAGE.into(),
                    location,
                });
            }
            Ok(())
        };
        let raw = chars.raw.as_mut().expect("raw State");
        loop {
            raw.cpp_space = false;
            raw.identifier_text = None;
            raw.empty_hint_comment = false;
            raw.token_start = raw.offset;
            raw.token_location = raw.location;
            let Some(unit) = raw.unit_at(raw.offset).map_err(&fail)? else {
                if raw.in_version_comment {
                    return Err(fail("unterminated MySQL version comment".into()));
                }
                return Ok(Some(None));
            };
            let byte = raw.bytes()[raw.offset];
            // Native MB unit is recognized before quote/backslash/comment syntax.
            if unit.space && !(unit.ascii && matches!(byte, b' ' | b'\t' | b'\r' | b'\n')) {
                raw.advance_bytes(usize::from(unit.width)).map_err(&fail)?;
                chars.col += 1;
                return Ok(Some(Some(Token::Whitespace(Whitespace::Space))));
            }
            if raw.in_version_comment && raw.bytes()[raw.offset..].starts_with(b"*/") {
                let before = raw.offset;
                raw.advance_bytes(2).map_err(&fail)?;
                raw.in_version_comment = false;
                let next_space = raw
                    .unit_at(raw.offset)
                    .map_err(&fail)?
                    .is_none_or(|next| next.space);
                let previous_space = before == 0
                    || raw
                        .unit_at(before - 1)
                        .map_err(&fail)?
                        .is_some_and(|previous| previous.space);
                if !next_space && !previous_space {
                    raw.token_start = raw.offset;
                    raw.token_location = raw.location;
                    raw.cpp_space = true;
                    return Ok(Some(Some(Token::Whitespace(Whitespace::Space))));
                }
                continue;
            }
            let line_comment = unit.ascii
                && (byte == b'#'
                    || (raw.bytes()[raw.offset..].starts_with(b"--")
                        && raw
                            .unit_at(raw.offset + 2)
                            .map_err(&fail)?
                            // Native yyPeek sees its EOF NUL sentinel here;
                            // no synthetic byte is added to the source owner.
                            .is_none_or(|third| third.space || third.control)));
            if line_comment {
                raw.has_comment = true;
                let start = raw.offset;
                while let Some(next) = raw.unit_at(raw.offset).map_err(&fail)? {
                    check()?;
                    if next.ascii && raw.bytes()[raw.offset] == b'\n' {
                        break;
                    }
                    raw.advance_bytes(usize::from(next.width)).map_err(&fail)?;
                    chars.col += 1;
                }
                return Ok(Some(Some(Token::Whitespace(Whitespace::MySqlComment(
                    raw.occurrence(start, raw.offset).map_err(&fail)?,
                )))));
            }
            if unit.ascii && raw.bytes()[raw.offset..].starts_with(b"/*") {
                raw.has_comment = true;
                let start = raw.offset;
                // Inside an executing special comment, another /*! is an
                // ordinary preserved comment, not another expansion lexer.
                let version = !raw.in_version_comment && raw.bytes()[start..].starts_with(b"/*!");
                if version {
                    raw.digest_comment_barrier = true;
                    raw.advance_bytes(3).map_err(&fail)?;
                    let digits = raw.bytes().get(raw.offset..raw.offset + 5);
                    let number = digits
                        .filter(|digits| digits.iter().all(u8::is_ascii_digit))
                        .map(|digits| {
                            digits
                                .iter()
                                .fold(0_u32, |value, digit| value * 10 + u32::from(digit - b'0'))
                        });
                    if let Some(number) = number {
                        raw.advance_bytes(5).map_err(&fail)?;
                        if number <= 80046 {
                            raw.in_version_comment = true;
                            continue;
                        }
                    } else {
                        raw.in_version_comment = true;
                        continue;
                    }
                } else {
                    raw.advance_bytes(2).map_err(&fail)?;
                }
                let mut nested = false;
                let mut empty_hint = !version && raw.bytes()[start..].starts_with(b"/*+");
                loop {
                    check()?;
                    let Some(next) = raw.unit_at(raw.offset).map_err(&fail)? else {
                        return Err(fail("unterminated MySQL comment".into()));
                    };
                    if next.ascii && raw.bytes()[raw.offset..].starts_with(b"*/") {
                        raw.advance_bytes(2).map_err(&fail)?;
                        if nested {
                            nested = false;
                            continue;
                        }
                        break;
                    }
                    if raw.offset != start + 2 || raw.bytes()[raw.offset] != b'+' {
                        empty_hint &= next.ascii && next.space;
                    }
                    if version
                        && !nested
                        && next.ascii
                        && raw.bytes()[raw.offset..].starts_with(b"/*")
                    {
                        nested = true;
                        raw.advance_bytes(2).map_err(&fail)?;
                        continue;
                    }
                    if next.ascii && raw.bytes()[raw.offset] == b'\n' {
                        chars.line += 1;
                        chars.col = 1;
                    } else {
                        chars.col += 1;
                    }
                    raw.advance_bytes(usize::from(next.width)).map_err(&fail)?;
                }
                if version {
                    continue;
                }
                raw.empty_hint_comment = empty_hint;
                return Ok(Some(Some(Token::Whitespace(Whitespace::MySqlComment(
                    raw.occurrence(start, raw.offset).map_err(&fail)?,
                )))));
            }
            let prefix = if unit.ascii
                && matches!(byte, b'n' | b'N' | b'x' | b'X' | b'b' | b'B')
                && raw.bytes().get(raw.offset + 1) == Some(&b'\'')
            {
                Some(byte)
            } else {
                None
            };
            let quote_offset = raw.offset + usize::from(prefix.is_some());
            let quote = raw.bytes()[quote_offset];
            if prefix.is_some()
                || (unit.ascii && (quote == b'\'' || quote == b'"' || quote == b'`'))
            {
                let start = raw.offset;
                let identifier = prefix.is_none()
                    && self
                        .dialect
                        .is_delimited_identifier_start(char::from(quote));
                raw.advance_bytes(quote_offset + 1 - raw.offset)
                    .map_err(&fail)?;
                let content_start = raw.offset;
                let binary_literal =
                    prefix.is_some_and(|prefix| matches!(prefix, b'x' | b'X' | b'b' | b'B'));
                let mut transformed: Option<(Vec<u8>, SqlSourceAdmission)> = None;
                loop {
                    let Some(next) = raw.unit_at(raw.offset).map_err(&fail)? else {
                        return Err(fail("unterminated MySQL quoted text".into()));
                    };
                    let current = raw.bytes()[raw.offset];
                    let doubled = next.ascii
                        && !binary_literal
                        && current == quote
                        && raw.bytes().get(raw.offset + 1) == Some(&quote);
                    if next.ascii && current == quote && !doubled {
                        let content_end = raw.offset;
                        raw.advance_bytes(1).map_err(&fail)?;
                        let semantic = if let Some((bytes, admission)) = transformed {
                            let len = bytes.len();
                            SqlSourceOwner::adopt(bytes, admission)
                                .occurrence(0..len)
                                .map_err(&fail)?
                        } else {
                            raw.occurrence(content_start, content_end).map_err(&fail)?
                        };
                        raw.admit_text_domain(usize::from(
                            prefix.is_some_and(|prefix| matches!(prefix, b'n' | b'N')),
                        ))
                        .map_err(&fail)?;
                        let text = MySqlText::from_occurrence(
                            semantic,
                            raw.occurrence(start, raw.offset).map_err(&fail)?,
                            raw.adapter.client_charset().into(),
                            char::from(quote),
                            prefix
                                .filter(|prefix| matches!(prefix, b'n' | b'N'))
                                .map(|_| "N".into()),
                        )
                        .map_err(&fail)?;
                        if identifier {
                            let (name, admission) = raw
                                .system_identifier(text.bytes().map_err(&fail)?, true)
                                .map_err(&fail)?;
                            raw.retain_admission(admission).map_err(&fail)?;
                            raw.identifier_text = Some(text);
                            return Ok(Some(Some(Token::make_word_owned(
                                name,
                                Some(char::from(quote)),
                            ))));
                        }
                        if prefix.is_some_and(|prefix| matches!(prefix, b'x' | b'X' | b'b' | b'B'))
                        {
                            let bytes = text.bytes().map_err(&fail)?;
                            let literal = core::str::from_utf8(bytes)
                                .map_err(|_| fail("non-ASCII MySQL binary literal".into()))?;
                            let bit = prefix.is_some_and(|prefix| matches!(prefix, b'b' | b'B'));
                            let capacity = if bit {
                                bytes.len().div_ceil(8).checked_mul(2)
                            } else {
                                Some(bytes.len())
                            }
                            .ok_or_else(|| fail("binary literal capacity overflow".into()))?;
                            let admission = raw.admit(capacity).map_err(&fail)?;
                            let value = if bit {
                                mysql_bit_literal_hex(literal)
                                    .ok_or_else(|| fail("invalid MySQL bit literal".into()))?
                            } else {
                                if !bytes.len().is_multiple_of(2)
                                    || !bytes.iter().all(u8::is_ascii_hexdigit)
                                {
                                    return Err(fail("invalid MySQL hex literal".into()));
                                }
                                literal.into()
                            };
                            raw.retain_admission(admission).map_err(&fail)?;
                            return Ok(Some(Some(Token::MySqlHexStringLiteral(
                                value,
                                if bit {
                                    MySqlHexLiteralKind::BitQuoted
                                } else {
                                    MySqlHexLiteralKind::HexQuoted
                                },
                            ))));
                        }
                        return Ok(Some(Some(Token::MySqlText(text))));
                    }
                    let escaped = next.ascii
                        && current == b'\\'
                        && !identifier
                        && !binary_literal
                        && self.dialect.supports_string_literal_backslash_escape();
                    if doubled || escaped {
                        if transformed.is_none() {
                            let capacity = (raw.offset - content_start)
                                .checked_add(64)
                                .ok_or_else(|| fail("literal capacity overflow".into()))?;
                            let admission = raw.admit_source(capacity).map_err(&fail)?;
                            let mut bytes = Vec::with_capacity(capacity);
                            bytes.extend_from_slice(&raw.bytes()[content_start..raw.offset]);
                            transformed = Some((bytes, admission));
                        }
                        let (bytes, admission) = transformed.as_mut().expect("admitted text");
                        let required = bytes
                            .len()
                            .checked_add(2)
                            .ok_or_else(|| fail("literal capacity overflow".into()))?;
                        if required > bytes.capacity() {
                            let capacity = bytes
                                .capacity()
                                .checked_mul(2)
                                .map(|capacity| capacity.max(required))
                                .ok_or_else(|| fail("literal capacity overflow".into()))?;
                            let replacement = raw.admit_source(capacity).map_err(&fail)?;
                            bytes.reserve_exact(capacity - bytes.len());
                            *admission = replacement;
                        }
                        if doubled {
                            bytes.push(quote);
                            raw.advance_bytes(2).map_err(&fail)?;
                        } else {
                            raw.advance_bytes(1).map_err(&fail)?;
                            let Some(&escaped) = raw.bytes().get(raw.offset) else {
                                return Err(fail("unterminated MySQL escape".into()));
                            };
                            let value = match escaped {
                                b'n' => b'\n',
                                b't' => b'\t',
                                b'r' => b'\r',
                                b'b' => 8,
                                b'0' => 0,
                                b'Z' => 26,
                                b'_' | b'%' => {
                                    bytes.push(b'\\');
                                    escaped
                                }
                                _ => escaped,
                            };
                            bytes.push(value);
                            raw.advance_escaped_byte();
                        }
                    } else {
                        if let Some((bytes, admission)) = &mut transformed {
                            let required = bytes
                                .len()
                                .checked_add(usize::from(next.width))
                                .ok_or_else(|| fail("literal capacity overflow".into()))?;
                            if required > bytes.capacity() {
                                let capacity = bytes
                                    .capacity()
                                    .checked_mul(2)
                                    .map(|capacity| capacity.max(required))
                                    .ok_or_else(|| fail("literal capacity overflow".into()))?;
                                let replacement = raw.admit_source(capacity).map_err(&fail)?;
                                bytes.reserve_exact(capacity - bytes.len());
                                *admission = replacement;
                            }
                            bytes.extend_from_slice(
                                &raw.bytes()[raw.offset..raw.offset + usize::from(next.width)],
                            );
                        }
                        raw.advance_bytes(usize::from(next.width)).map_err(&fail)?;
                    }
                    if next.ascii && current == b'\n' {
                        chars.line += 1;
                        chars.col = 1;
                    } else {
                        chars.col += 1;
                    }
                    chars.characters_since_cancellation_check += 1;
                    if chars.characters_since_cancellation_check >= 1024 {
                        chars.characters_since_cancellation_check = 0;
                        if chars.cancellation.as_ref().is_some_and(|check| check()) {
                            chars.cancelled = true;
                            return Err(TokenizerError {
                                source_kind: MySqlSourceErrorKind::Cancelled,
                                message: QUERY_CANCELLED_MESSAGE.into(),
                                location,
                            });
                        }
                    }
                }
            }
            if unit.identifier && !(unit.ascii && byte.is_ascii_digit()) {
                let start = raw.offset;
                while let Some(next) = raw.unit_at(raw.offset).map_err(&fail)? {
                    check()?;
                    if !next.identifier {
                        break;
                    }
                    raw.advance_bytes(usize::from(next.width)).map_err(&fail)?;
                    chars.col += 1;
                }
                let (name, admission) = raw
                    .system_identifier(&raw.bytes()[start..raw.offset], false)
                    .map_err(&fail)?;
                raw.retain_admission(admission).map_err(&fail)?;
                let source = raw.occurrence(start, raw.offset).map_err(&fail)?;
                raw.admit_text_domain(0).map_err(&fail)?;
                raw.identifier_text = Some(
                    MySqlText::from_occurrence(
                        source.clone(),
                        source,
                        raw.adapter.client_charset().into(),
                        '\0',
                        None,
                    )
                    .map_err(&fail)?,
                );
                if !matches!(prev_token, Some(Token::Period)) {
                    if let Some(charset) = name.strip_prefix('_') {
                        // sql_lex.cc issues the condition when the actual
                        // known charset introducer is demanded, before text
                        // or an adjacent-literal tail is parsed/itemized.
                        raw.adapter.deprecated_character_set(charset);
                    }
                }
                return Ok(Some(Some(Token::make_word_owned(name, None))));
            }
            if !unit.ascii {
                // Invalid source is retained, not replaced by U+FFFD or a fake
                // Char token. Context-specific diagnostic text is a later API.
                return Err(fail("unrecognized native MySQL lexical unit".into()));
            }
            return Ok(None); // Existing ASCII FSM owns operators/numbers/space.
        }
    }

    fn next_token(
        &self,
        chars: &mut State,
        prev_token: Option<&Token>,
    ) -> Result<Option<Token>, TokenizerError> {
        if chars.raw.is_some() {
            if let Some(token) = self.mysql_source_token(chars, prev_token)? {
                return Ok(token);
            }
        }
        match chars.peek() {
            Some(&ch) => match ch {
                ' ' => self.consume_and_return(chars, Token::Whitespace(Whitespace::Space)),
                '\t' => self.consume_and_return(chars, Token::Whitespace(Whitespace::Tab)),
                '\n' => self.consume_and_return(chars, Token::Whitespace(Whitespace::Newline)),
                '\r' => {
                    // Emit a single Whitespace::Newline token for \r and \r\n
                    chars.next();
                    if let Some('\n') = chars.peek() {
                        chars.next();
                    }
                    Ok(Some(Token::Whitespace(Whitespace::Newline)))
                }
                // BigQuery and MySQL use b or B for byte string literal, Postgres for bit strings
                b @ 'B' | b @ 'b' if dialect_of!(self is BigQueryDialect | PostgreSqlDialect | MySqlDialect | GenericDialect) =>
                {
                    let starting_loc = chars.location();
                    chars.next(); // consume
                    match chars.peek() {
                        Some('\'') => {
                            if self.dialect.supports_triple_quoted_string() {
                                return self
                                    .tokenize_single_or_triple_quoted_string::<fn(String) -> Token>(
                                        chars,
                                        '\'',
                                        false,
                                        Token::SingleQuotedByteStringLiteral,
                                        Token::TripleSingleQuotedByteStringLiteral,
                                    );
                            }
                            let s = self.tokenize_single_quoted_string(chars, '\'', false)?;
                            if dialect_of!(self is MySqlDialect) {
                                let Some(hex) = mysql_bit_literal_hex(&s) else {
                                    return self.tokenizer_error(
                                        starting_loc,
                                        "MySQL bit literals may contain only 0 and 1",
                                    );
                                };
                                return Ok(Some(Token::MySqlHexStringLiteral(
                                    hex,
                                    MySqlHexLiteralKind::BitQuoted,
                                )));
                            }
                            Ok(Some(Token::SingleQuotedByteStringLiteral(s)))
                        }
                        Some('\"') => {
                            if self.dialect.supports_triple_quoted_string() {
                                return self
                                    .tokenize_single_or_triple_quoted_string::<fn(String) -> Token>(
                                        chars,
                                        '"',
                                        false,
                                        Token::DoubleQuotedByteStringLiteral,
                                        Token::TripleDoubleQuotedByteStringLiteral,
                                    );
                            }
                            let s = self.tokenize_single_quoted_string(chars, '\"', false)?;
                            Ok(Some(Token::DoubleQuotedByteStringLiteral(s)))
                        }
                        _ => {
                            // regular identifier starting with an "b" or "B"
                            let s = self.tokenize_word(b, chars);
                            Ok(Some(Token::make_word_owned(s, None)))
                        }
                    }
                }
                // BigQuery uses r or R for raw string literal
                b @ 'R' | b @ 'r' if dialect_of!(self is BigQueryDialect | GenericDialect) => {
                    chars.next(); // consume
                    match chars.peek() {
                        Some('\'') => self
                            .tokenize_single_or_triple_quoted_string::<fn(String) -> Token>(
                                chars,
                                '\'',
                                false,
                                Token::SingleQuotedRawStringLiteral,
                                Token::TripleSingleQuotedRawStringLiteral,
                            ),
                        Some('\"') => self
                            .tokenize_single_or_triple_quoted_string::<fn(String) -> Token>(
                                chars,
                                '"',
                                false,
                                Token::DoubleQuotedRawStringLiteral,
                                Token::TripleDoubleQuotedRawStringLiteral,
                            ),
                        _ => {
                            // regular identifier starting with an "r" or "R"
                            let s = self.tokenize_word(b, chars);
                            Ok(Some(Token::make_word_owned(s, None)))
                        }
                    }
                }
                // Redshift uses lower case n for national string literal
                n @ 'N' | n @ 'n' => {
                    chars.next(); // consume, to check the next char
                    match chars.peek() {
                        Some('\'') => {
                            // N'...' - a <national character string literal>
                            let backslash_escape =
                                self.dialect.supports_string_literal_backslash_escape();
                            let s =
                                self.tokenize_single_quoted_string(chars, '\'', backslash_escape)?;
                            Ok(Some(Token::NationalStringLiteral(s)))
                        }
                        Some(&q @ 'q') | Some(&q @ 'Q')
                            if self.dialect.supports_quote_delimited_string() =>
                        {
                            chars.next(); // consume and check the next char
                            if let Some('\'') = chars.peek() {
                                self.tokenize_quote_delimited_string(chars, &[n, q])
                                    .map(|s| Some(Token::NationalQuoteDelimitedStringLiteral(s)))
                            } else {
                                let s = self.tokenize_word(String::from_iter([n, q]), chars);
                                Ok(Some(Token::make_word_owned(s, None)))
                            }
                        }
                        _ => {
                            // regular identifier starting with an "N"
                            let s = self.tokenize_word(n, chars);
                            Ok(Some(Token::make_word_owned(s, None)))
                        }
                    }
                }
                q @ 'Q' | q @ 'q' if self.dialect.supports_quote_delimited_string() => {
                    chars.next(); // consume and check the next char
                    if let Some('\'') = chars.peek() {
                        self.tokenize_quote_delimited_string(chars, &[q])
                            .map(|s| Some(Token::QuoteDelimitedStringLiteral(s)))
                    } else {
                        let s = self.tokenize_word(q, chars);
                        Ok(Some(Token::make_word_owned(s, None)))
                    }
                }
                // PostgreSQL accepts "escape" string constants, which are an extension to the SQL standard.
                x @ 'e' | x @ 'E' if self.dialect.supports_string_escape_constant() => {
                    let starting_loc = chars.location();
                    chars.next(); // consume, to check the next char
                    match chars.peek() {
                        Some('\'') => {
                            let s =
                                self.tokenize_escaped_single_quoted_string(starting_loc, chars)?;
                            Ok(Some(Token::EscapedStringLiteral(s)))
                        }
                        _ => {
                            // regular identifier starting with an "E" or "e"
                            let s = self.tokenize_word(x, chars);
                            Ok(Some(Token::make_word_owned(s, None)))
                        }
                    }
                }
                // Unicode string literals like U&'first \000A second' are supported in some dialects, including PostgreSQL
                x @ 'u' | x @ 'U' if self.dialect.supports_unicode_string_literal() => {
                    chars.next(); // consume, to check the next char
                    if chars.peek() == Some(&'&') {
                        // we cannot advance the iterator here, as we need to consume the '&' later if the 'u' was an identifier
                        let mut chars_clone = chars.peekable.clone();
                        chars_clone.next(); // consume the '&' in the clone
                        if chars_clone.peek() == Some(&'\'') {
                            chars.next(); // consume the '&' in the original iterator
                            let s = unescape_unicode_single_quoted_string(chars)?;
                            return Ok(Some(Token::UnicodeStringLiteral(s)));
                        }
                    }
                    // regular identifier starting with an "U" or "u"
                    let s = self.tokenize_word(x, chars);
                    Ok(Some(Token::make_word_owned(s, None)))
                }
                // The spec only allows an uppercase 'X' to introduce a hex
                // string, but PostgreSQL, at least, allows a lowercase 'x' too.
                x @ 'x' | x @ 'X' => {
                    chars.next(); // consume, to check the next char
                    match chars.peek() {
                        Some('\'') => {
                            // X'...' - a <binary string literal>
                            let s = self.tokenize_single_quoted_string(chars, '\'', true)?;
                            Ok(Some(if dialect_of!(self is MySqlDialect) {
                                Token::MySqlHexStringLiteral(s, MySqlHexLiteralKind::HexQuoted)
                            } else {
                                Token::HexStringLiteral(s)
                            }))
                        }
                        _ => {
                            // regular identifier starting with an "X"
                            let s = self.tokenize_word(x, chars);
                            Ok(Some(Token::make_word_owned(s, None)))
                        }
                    }
                }
                // single quoted string
                '\'' => {
                    if self.dialect.supports_triple_quoted_string() {
                        return self
                            .tokenize_single_or_triple_quoted_string::<fn(String) -> Token>(
                                chars,
                                '\'',
                                self.dialect.supports_string_literal_backslash_escape(),
                                Token::SingleQuotedString,
                                Token::TripleSingleQuotedString,
                            );
                    }
                    let s = self.tokenize_single_quoted_string(
                        chars,
                        '\'',
                        self.dialect.supports_string_literal_backslash_escape(),
                    )?;

                    Ok(Some(Token::SingleQuotedString(s)))
                }
                // double quoted string
                '\"' if !self.dialect.is_delimited_identifier_start(ch)
                    && !self.dialect.is_identifier_start(ch) =>
                {
                    if self.dialect.supports_triple_quoted_string() {
                        return self
                            .tokenize_single_or_triple_quoted_string::<fn(String) -> Token>(
                                chars,
                                '"',
                                self.dialect.supports_string_literal_backslash_escape(),
                                Token::DoubleQuotedString,
                                Token::TripleDoubleQuotedString,
                            );
                    }
                    let s = self.tokenize_single_quoted_string(
                        chars,
                        '"',
                        self.dialect.supports_string_literal_backslash_escape(),
                    )?;

                    Ok(Some(Token::DoubleQuotedString(s)))
                }
                // delimited (quoted) identifier
                quote_start if self.dialect.is_delimited_identifier_start(ch) => {
                    let word = self.tokenize_quoted_identifier(quote_start, chars)?;
                    Ok(Some(Token::make_word_owned(word, Some(quote_start))))
                }
                // Potentially nested delimited (quoted) identifier
                quote_start
                    if self
                        .dialect
                        .is_nested_delimited_identifier_start(quote_start)
                        && self
                            .dialect
                            .peek_nested_delimited_identifier_quotes(chars.peekable.clone())
                            .is_some() =>
                {
                    let Some((quote_start, nested_quote_start)) = self
                        .dialect
                        .peek_nested_delimited_identifier_quotes(chars.peekable.clone())
                    else {
                        return self.tokenizer_error(
                            chars.location(),
                            format!("Expected nested delimiter '{quote_start}' before EOF."),
                        );
                    };

                    let Some(nested_quote_start) = nested_quote_start else {
                        let word = self.tokenize_quoted_identifier(quote_start, chars)?;
                        return Ok(Some(Token::make_word_owned(word, Some(quote_start))));
                    };

                    let mut word = vec![];
                    let quote_end = Word::matching_end_quote(quote_start);
                    let nested_quote_end = Word::matching_end_quote(nested_quote_start);
                    let error_loc = chars.location();

                    chars.next(); // skip the first delimiter
                    peeking_take_while(chars, |ch| ch.is_whitespace());
                    if chars.peek() != Some(&nested_quote_start) {
                        return self.tokenizer_error(
                            error_loc,
                            format!("Expected nested delimiter '{nested_quote_start}' before EOF."),
                        );
                    }
                    word.push(nested_quote_start.into());
                    word.push(self.tokenize_quoted_identifier(nested_quote_end, chars)?);
                    word.push(nested_quote_end.into());
                    peeking_take_while(chars, |ch| ch.is_whitespace());
                    if chars.peek() != Some(&quote_end) {
                        return self.tokenizer_error(
                            error_loc,
                            format!("Expected close delimiter '{quote_end}' before EOF."),
                        );
                    }
                    chars.next(); // skip close delimiter

                    Ok(Some(Token::make_word_owned(
                        word.concat(),
                        Some(quote_start),
                    )))
                }
                // numbers and period
                '0'..='9' | '.' => {
                    // special case where if ._ is encountered after a word then that word
                    // is a table and the _ is the start of the col name.
                    // if the prev token is not a word, then this is not a valid sql
                    // word or number.
                    if ch == '.' && chars.peek_nth(1) == Some('_') {
                        if let Some(Token::Word(_)) = prev_token {
                            chars.next();
                            return Ok(Some(Token::Period));
                        }

                        return self.tokenizer_error(
                            chars.location(),
                            "Unexpected character '_'".to_string(),
                        );
                    }

                    // Some dialects support underscore as number separator
                    // There can only be one at a time and it must be followed by another digit
                    let is_number_separator = |ch: char, next_char: Option<char>| {
                        self.dialect.supports_numeric_literal_underscores()
                            && ch == '_'
                            && next_char.is_some_and(|next_ch| next_ch.is_ascii_hexdigit())
                    };

                    let mut s = admitted_numeric_digits(chars, |ch, next_ch| {
                        ch.is_ascii_digit() || is_number_separator(ch, next_ch)
                    })?;

                    // match binary literal that starts with 0x
                    if s.value == "0" && chars.peek() == Some(&'x') {
                        if let Some(raw) = &chars.raw {
                            let mut candidate = chars.ascii_lookahead();
                            candidate.next();
                            let hex_count = candidate.take_while(char::is_ascii_hexdigit).count();
                            let identifier_tail = raw
                                .unit_at(raw.offset + 1 + hex_count)
                                .map_err(|message| TokenizerError {
                                    source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                                    message,
                                    location: chars.location(),
                                })?
                                .is_some_and(|unit| unit.identifier);
                            if hex_count == 0 || identifier_tail {
                                return self.tokenize_identifier_or_keyword([], chars);
                            }
                        }
                        chars.next();
                        let mut s2 = admitted_numeric_digits(chars, |ch, next_ch| {
                            ch.is_ascii_hexdigit() || is_number_separator(ch, next_ch)
                        })?;
                        // MySQL pads an odd leading nibble for unquoted 0x
                        // literals. Quoted X'...' literals take the distinct
                        // branch above and remain even-digit-only.
                        if !s2.value.is_empty() && !s2.value.len().is_multiple_of(2) {
                            s2.reserve(chars, 1)?;
                            s2.value.insert(0, '0');
                        }
                        let value = s2.into_string(chars)?;
                        return Ok(Some(if dialect_of!(self is MySqlDialect) {
                            Token::MySqlHexStringLiteral(value, MySqlHexLiteralKind::HexNumber)
                        } else {
                            Token::HexStringLiteral(value)
                        }));
                    }

                    // MySQL's unquoted 0b form has the same binary-string
                    // semantics as b'...'. Recognize it only when at least one
                    // bit follows and the token ends at an identifier boundary;
                    // otherwise numeric-prefix identifier handling below owns
                    // the complete token (for example 0b102).
                    if s.value == "0"
                        && chars.peek() == Some(&'b')
                        && dialect_of!(self is MySqlDialect)
                    {
                        let mut candidate = chars.ascii_lookahead().peekable();
                        candidate.next();
                        let mut bit_count = 0_usize;
                        while candidate.peek().is_some_and(|ch| matches!(ch, '0' | '1')) {
                            candidate.next();
                            bit_count += 1;
                        }
                        let identifier_tail = if let Some(raw) = &chars.raw {
                            raw.unit_at(raw.offset + 1 + bit_count)
                                .map_err(|message| TokenizerError {
                                    source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                                    message,
                                    location: chars.location(),
                                })?
                                .is_some_and(|unit| unit.identifier)
                        } else {
                            candidate
                                .peek()
                                .is_some_and(|ch| self.dialect.is_identifier_part(*ch))
                        };
                        if bit_count != 0 && !identifier_tail {
                            chars.next();
                            let bits =
                                admitted_numeric_digits(chars, |ch, _| matches!(ch, '0' | '1'))?;
                            let capacity =
                                bit_count.div_ceil(8).checked_mul(2).ok_or_else(|| {
                                    TokenizerError {
                                        source_kind:
                                            crate::tokenizer::MySqlSourceErrorKind::Message,
                                        message: "binary literal capacity overflow".into(),
                                        location: chars.location(),
                                    }
                                })?;
                            let admission = chars
                                .raw
                                .as_ref()
                                .map(|raw| raw.admit(capacity))
                                .transpose()
                                .map_err(|message| TokenizerError {
                                    source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                                    message,
                                    location: chars.location(),
                                })?;
                            let value = mysql_bit_literal_hex(&bits.value)
                                .expect("validated binary digits");
                            return Ok(Some(Token::MySqlHexStringLiteral(
                                AdmittedTokenText { value, admission }.into_string(chars)?,
                                MySqlHexLiteralKind::BitNumber,
                            )));
                        }
                    }

                    // match one period
                    if let Some('.') = chars.peek() {
                        s.push(chars, '.')?;
                        chars.next();
                    }

                    // If the dialect supports identifiers that start with a numeric prefix
                    // and we have now consumed a dot, check if the previous token was a Word.
                    // If so, what follows is definitely not part of a decimal number and
                    // we should yield the dot as a dedicated token so compound identifiers
                    // starting with digits can be parsed correctly.
                    if s.value == "." && self.dialect.supports_numeric_prefix() {
                        if let Some(Token::Word(_)) = prev_token {
                            return Ok(Some(Token::Period));
                        }
                    }

                    // Consume fractional digits.
                    let fraction = admitted_numeric_digits(chars, |ch, next_ch| {
                        ch.is_ascii_digit() || is_number_separator(ch, next_ch)
                    })?;
                    s.append(chars, &fraction.value)?;

                    // No fraction -> Token::Period
                    if s.value == "." {
                        return Ok(Some(Token::Period));
                    }

                    // Parse exponent as number
                    let mut exponent_part = AdmittedTokenText::new();
                    if chars.peek() == Some(&'e') || chars.peek() == Some(&'E') {
                        let mut char_clone = chars.ascii_lookahead().peekable();
                        exponent_part.push(chars, char_clone.next().unwrap())?;

                        // Optional sign
                        match char_clone.peek() {
                            Some(&c) if matches!(c, '+' | '-') => {
                                exponent_part.push(chars, c)?;
                                char_clone.next();
                            }
                            _ => (),
                        }

                        match char_clone.peek() {
                            // Definitely an exponent, get original iterator up to speed and use it
                            Some(&c) if c.is_ascii_digit() => {
                                for _ in 0..exponent_part.value.len() {
                                    chars.next();
                                }
                                let digits =
                                    admitted_numeric_digits(chars, |ch, _| ch.is_ascii_digit())?;
                                exponent_part.append(chars, &digits.value)?;
                                s.append(chars, &exponent_part.value)?;
                            }
                            // Not an exponent, discard the work done
                            _ => {
                                if chars.raw.is_some() {
                                    if s.value.contains('.') {
                                        return self.tokenizer_error(
                                            chars.location(),
                                            "incomplete MySQL decimal exponent",
                                        );
                                    }
                                    // MY_LEX_NUMBER_IDENT retains a failed
                                    // exponent as the original full identifier.
                                    exponent_part = AdmittedTokenText::new();
                                }
                            }
                        }
                    }

                    // If the dialect supports identifiers that start with a numeric prefix,
                    // we need to check if the value is in fact an identifier and must thus
                    // be tokenized as a word.
                    if self.dialect.supports_numeric_prefix()
                        && !(chars.raw.is_some() && s.value.contains('.'))
                    {
                        if exponent_part.value.is_empty()
                            && chars.raw.as_ref().is_some_and(|raw| {
                                raw.unit_at(raw.offset)
                                    .ok()
                                    .flatten()
                                    .is_some_and(|unit| unit.identifier)
                            })
                        {
                            return self.tokenize_identifier_or_keyword([], chars);
                        }
                        if exponent_part.value.is_empty() {
                            // If it is not a number with an exponent, it may be
                            // an identifier starting with digits.
                            let word = admitted_numeric_digits(chars, |ch, _| {
                                self.dialect.is_identifier_part(ch)
                            })?;

                            if !word.value.is_empty() {
                                s.append(chars, &word.value)?;
                                return Ok(Some(Token::make_word_owned(
                                    s.into_string(chars)?,
                                    None,
                                )));
                            }
                        } else if prev_token == Some(&Token::Period) {
                            // If the previous token was a period, thus not belonging to a number,
                            // the value we have is part of an identifier.
                            return Ok(Some(Token::make_word_owned(s.into_string(chars)?, None)));
                        }
                    }

                    let long = if chars.peek() == Some(&'L') {
                        chars.next();
                        true
                    } else {
                        false
                    };
                    Ok(Some(Token::Number(s.into_string(chars)?, long)))
                }
                // punctuation
                '(' => self.consume_and_return(chars, Token::LParen),
                ')' => self.consume_and_return(chars, Token::RParen),
                ',' => self.consume_and_return(chars, Token::Comma),
                // operators
                '-' => {
                    chars.next(); // consume the '-'

                    match chars.peek() {
                        Some('-') => {
                            let mut is_comment = true;
                            if self.dialect.requires_single_line_comment_whitespace() {
                                is_comment = chars
                                    .peekable
                                    .clone()
                                    .nth(1)
                                    .is_some_and(char::is_whitespace);
                            }

                            if is_comment {
                                chars.next(); // consume second '-'
                                let comment = self.tokenize_single_line_comment(chars);
                                return Ok(Some(Token::Whitespace(
                                    Whitespace::SingleLineComment {
                                        prefix: "--".to_owned(),
                                        comment,
                                    },
                                )));
                            }

                            self.start_binop(chars, "-", Token::Minus)
                        }
                        Some('>') => {
                            chars.next();
                            match chars.peek() {
                                Some('>') => self.consume_for_binop(chars, "->>", Token::LongArrow),
                                _ => self.start_binop(chars, "->", Token::Arrow),
                            }
                        }
                        // a regular '-' operator
                        _ => self.start_binop(chars, "-", Token::Minus),
                    }
                }
                '/' => {
                    chars.next(); // consume the '/'
                    match chars.peek() {
                        Some('*') => {
                            chars.next(); // consume the '*', starting a multi-line comment
                            self.tokenize_multiline_comment(chars)
                        }
                        Some('/') if dialect_of!(self is SnowflakeDialect) => {
                            chars.next(); // consume the second '/', starting a snowflake single-line comment
                            let comment = self.tokenize_single_line_comment(chars);
                            Ok(Some(Token::Whitespace(Whitespace::SingleLineComment {
                                prefix: "//".to_owned(),
                                comment,
                            })))
                        }
                        Some('/') if dialect_of!(self is DuckDbDialect | GenericDialect) => {
                            self.consume_and_return(chars, Token::DuckIntDiv)
                        }
                        // a regular '/' operator
                        _ => Ok(Some(Token::Div)),
                    }
                }
                '+' => self.consume_and_return(chars, Token::Plus),
                '*' => self.consume_and_return(chars, Token::Mul),
                '%' => {
                    chars.next(); // advance past '%'
                    match chars.peek() {
                        Some(s) if s.is_whitespace() => Ok(Some(Token::Mod)),
                        Some(sch) if self.dialect.is_identifier_start('%') => {
                            self.tokenize_identifier_or_keyword([ch, *sch], chars)
                        }
                        _ => self.start_binop(chars, "%", Token::Mod),
                    }
                }
                '|' => {
                    chars.next(); // consume the '|'
                    match chars.peek() {
                        Some('/') => self.consume_for_binop(chars, "|/", Token::PGSquareRoot),
                        Some('|') => {
                            chars.next(); // consume the second '|'
                            match chars.peek() {
                                Some('/') => {
                                    self.consume_for_binop(chars, "||/", Token::PGCubeRoot)
                                }
                                _ => self.start_binop(chars, "||", Token::StringConcat),
                            }
                        }
                        Some('&') if self.dialect.supports_geometric_types() => {
                            chars.next(); // consume
                            match chars.peek() {
                                Some('>') => self.consume_for_binop(
                                    chars,
                                    "|&>",
                                    Token::VerticalBarAmpersandRightAngleBracket,
                                ),
                                _ => self.start_binop_opt(chars, "|&", None),
                            }
                        }
                        Some('>') if self.dialect.supports_geometric_types() => {
                            chars.next(); // consume
                            match chars.peek() {
                                Some('>') => self.consume_for_binop(
                                    chars,
                                    "|>>",
                                    Token::VerticalBarShiftRight,
                                ),
                                _ => self.start_binop_opt(chars, "|>", None),
                            }
                        }
                        Some('>') if self.dialect.supports_pipe_operator() => {
                            self.consume_for_binop(chars, "|>", Token::VerticalBarRightAngleBracket)
                        }
                        // Bitshift '|' operator
                        _ => self.start_binop(chars, "|", Token::Pipe),
                    }
                }
                '=' => {
                    chars.next(); // consume
                    match chars.peek() {
                        Some('>') => self.consume_and_return(chars, Token::RArrow),
                        Some('=') => self.consume_and_return(chars, Token::DoubleEq),
                        _ => Ok(Some(Token::Eq)),
                    }
                }
                '!' => {
                    chars.next(); // consume
                    match chars.peek() {
                        Some('=') => self.consume_and_return(chars, Token::Neq),
                        Some('!') => self.consume_and_return(chars, Token::DoubleExclamationMark),
                        Some('~') => {
                            chars.next();
                            match chars.peek() {
                                Some('*') => self
                                    .consume_and_return(chars, Token::ExclamationMarkTildeAsterisk),
                                Some('~') => {
                                    chars.next();
                                    match chars.peek() {
                                        Some('*') => self.consume_and_return(
                                            chars,
                                            Token::ExclamationMarkDoubleTildeAsterisk,
                                        ),
                                        _ => Ok(Some(Token::ExclamationMarkDoubleTilde)),
                                    }
                                }
                                _ => Ok(Some(Token::ExclamationMarkTilde)),
                            }
                        }
                        _ => Ok(Some(Token::ExclamationMark)),
                    }
                }
                '<' => {
                    chars.next(); // consume
                    match chars.peek() {
                        Some('=') => {
                            chars.next();
                            match chars.peek() {
                                Some('>') => self.consume_for_binop(chars, "<=>", Token::Spaceship),
                                // `<=+` and `<=-` are not valid combined operators; treat `<=` as
                                // the operator and leave `+`/`-` to be tokenized separately.
                                Some('+') | Some('-') => Ok(Some(Token::LtEq)),
                                _ => self.start_binop(chars, "<=", Token::LtEq),
                            }
                        }
                        Some('|') if self.dialect.supports_geometric_types() => {
                            self.consume_for_binop(chars, "<<|", Token::ShiftLeftVerticalBar)
                        }
                        Some('>') => self.consume_for_binop(chars, "<>", Token::Neq),
                        Some('<') if self.dialect.supports_geometric_types() => {
                            chars.next(); // consume
                            match chars.peek() {
                                Some('|') => self.consume_for_binop(
                                    chars,
                                    "<<|",
                                    Token::ShiftLeftVerticalBar,
                                ),
                                _ => self.start_binop(chars, "<<", Token::ShiftLeft),
                            }
                        }
                        Some('<') => self.consume_for_binop(chars, "<<", Token::ShiftLeft),
                        // `<+` is not a valid combined operator; treat `<` as the operator
                        // and leave `+` to be tokenized separately.
                        Some('+') => Ok(Some(Token::Lt)),
                        Some('-') if self.dialect.supports_geometric_types() => {
                            if chars.peekable.clone().nth(1) == Some('>') {
                                chars.next(); // consume `-`
                                self.consume_for_binop(chars, "<->", Token::TwoWayArrow)
                            } else {
                                Ok(Some(Token::Lt))
                            }
                        }
                        Some('^') if self.dialect.supports_geometric_types() => {
                            self.consume_for_binop(chars, "<^", Token::LeftAngleBracketCaret)
                        }
                        Some('@') => self.consume_for_binop(chars, "<@", Token::ArrowAt),
                        _ => self.start_binop(chars, "<", Token::Lt),
                    }
                }
                '>' => {
                    chars.next(); // consume
                    match chars.peek() {
                        Some('=') => self.consume_for_binop(chars, ">=", Token::GtEq),
                        Some('>') => self.consume_for_binop(chars, ">>", Token::ShiftRight),
                        Some('^') if self.dialect.supports_geometric_types() => {
                            self.consume_for_binop(chars, ">^", Token::RightAngleBracketCaret)
                        }
                        _ => self.start_binop(chars, ">", Token::Gt),
                    }
                }
                ':' => {
                    chars.next();
                    match chars.peek() {
                        Some(':') => self.consume_and_return(chars, Token::DoubleColon),
                        Some('=') => self.consume_and_return(chars, Token::Assignment),
                        _ => Ok(Some(Token::Colon)),
                    }
                }
                ';' => self.consume_and_return(chars, Token::SemiColon),
                '\\' => self.consume_and_return(chars, Token::Backslash),
                '[' => self.consume_and_return(chars, Token::LBracket),
                ']' => self.consume_and_return(chars, Token::RBracket),
                '&' => {
                    chars.next(); // consume the '&'
                    match chars.peek() {
                        Some('>') if self.dialect.supports_geometric_types() => {
                            chars.next();
                            self.consume_and_return(chars, Token::AmpersandRightAngleBracket)
                        }
                        Some('<') if self.dialect.supports_geometric_types() => {
                            chars.next(); // consume
                            match chars.peek() {
                                Some('|') => self.consume_and_return(
                                    chars,
                                    Token::AmpersandLeftAngleBracketVerticalBar,
                                ),
                                _ => {
                                    self.start_binop(chars, "&<", Token::AmpersandLeftAngleBracket)
                                }
                            }
                        }
                        Some('&') => {
                            chars.next(); // consume the second '&'
                            self.start_binop(chars, "&&", Token::Overlap)
                        }
                        // Bitshift '&' operator
                        _ => self.start_binop(chars, "&", Token::Ampersand),
                    }
                }
                '^' => {
                    chars.next(); // consume the '^'
                    match chars.peek() {
                        Some('@') => self.consume_and_return(chars, Token::CaretAt),
                        _ => Ok(Some(Token::Caret)),
                    }
                }
                '{' => self.consume_and_return(chars, Token::LBrace),
                '}' => self.consume_and_return(chars, Token::RBrace),
                '#' if dialect_of!(self is SnowflakeDialect | BigQueryDialect | MySqlDialect | HiveDialect) =>
                {
                    chars.next(); // consume the '#', starting a snowflake single-line comment
                    let comment = self.tokenize_single_line_comment(chars);
                    Ok(Some(Token::Whitespace(Whitespace::SingleLineComment {
                        prefix: "#".to_owned(),
                        comment,
                    })))
                }
                '~' => {
                    chars.next(); // consume
                    match chars.peek() {
                        Some('*') => self.consume_for_binop(chars, "~*", Token::TildeAsterisk),
                        Some('=') if self.dialect.supports_geometric_types() => {
                            self.consume_for_binop(chars, "~=", Token::TildeEqual)
                        }
                        Some('~') => {
                            chars.next();
                            match chars.peek() {
                                Some('*') => {
                                    self.consume_for_binop(chars, "~~*", Token::DoubleTildeAsterisk)
                                }
                                _ => self.start_binop(chars, "~~", Token::DoubleTilde),
                            }
                        }
                        _ => self.start_binop(chars, "~", Token::Tilde),
                    }
                }
                '#' => {
                    chars.next();
                    match chars.peek() {
                        Some('-') => self.consume_for_binop(chars, "#-", Token::HashMinus),
                        Some('>') => {
                            chars.next();
                            match chars.peek() {
                                Some('>') => {
                                    self.consume_for_binop(chars, "#>>", Token::HashLongArrow)
                                }
                                _ => self.start_binop(chars, "#>", Token::HashArrow),
                            }
                        }
                        Some(' ') => Ok(Some(Token::Sharp)),
                        Some('#') if self.dialect.supports_geometric_types() => {
                            self.consume_for_binop(chars, "##", Token::DoubleSharp)
                        }
                        Some(sch) if self.dialect.is_identifier_start('#') => {
                            self.tokenize_identifier_or_keyword([ch, *sch], chars)
                        }
                        _ => self.start_binop(chars, "#", Token::Sharp),
                    }
                }
                '@' => {
                    chars.next();
                    match chars.peek() {
                        Some('@') if self.dialect.supports_geometric_types() => {
                            self.consume_and_return(chars, Token::AtAt)
                        }
                        Some('-') if self.dialect.supports_geometric_types() => {
                            chars.next();
                            match chars.peek() {
                                Some('@') => self.consume_and_return(chars, Token::AtDashAt),
                                _ => self.start_binop_opt(chars, "@-", None),
                            }
                        }
                        Some('>') => self.consume_and_return(chars, Token::AtArrow),
                        Some('?') => self.consume_and_return(chars, Token::AtQuestion),
                        Some('@') => {
                            chars.next();
                            match chars.peek() {
                                Some(' ') => Ok(Some(Token::AtAt)),
                                Some(tch) if self.dialect.is_identifier_start('@') => {
                                    self.tokenize_identifier_or_keyword([ch, '@', *tch], chars)
                                }
                                _ => Ok(Some(Token::AtAt)),
                            }
                        }
                        Some(' ') => Ok(Some(Token::AtSign)),
                        // We break on quotes here, because no dialect allows identifiers starting
                        // with @ and containing quotation marks (e.g. `@'foo'`) unless they are
                        // quoted, which is tokenized as a quoted string, not here (e.g.
                        // `"@'foo'"`). Further, at least two dialects parse `@` followed by a
                        // quoted string as two separate tokens, which this allows. For example,
                        // Postgres parses `@'1'` as the absolute value of '1' which is implicitly
                        // cast to a numeric type. And when parsing MySQL-style grantees (e.g.
                        // `GRANT ALL ON *.* to 'root'@'localhost'`), we also want separate tokens
                        // for the user, the `@`, and the host.
                        Some('\'') => Ok(Some(Token::AtSign)),
                        Some('\"') => Ok(Some(Token::AtSign)),
                        Some('`') => Ok(Some(Token::AtSign)),
                        Some(sch) if self.dialect.is_identifier_start('@') => {
                            self.tokenize_identifier_or_keyword([ch, *sch], chars)
                        }
                        _ => Ok(Some(Token::AtSign)),
                    }
                }
                // Postgres uses ? for jsonb operators, not prepared statements
                '?' if self.dialect.supports_geometric_types() => {
                    chars.next(); // consume
                    match chars.peek() {
                        Some('|') => {
                            chars.next();
                            match chars.peek() {
                                Some('|') => self.consume_and_return(
                                    chars,
                                    Token::QuestionMarkDoubleVerticalBar,
                                ),
                                _ => Ok(Some(Token::QuestionPipe)),
                            }
                        }

                        Some('&') => self.consume_and_return(chars, Token::QuestionAnd),
                        Some('-') => {
                            chars.next(); // consume
                            match chars.peek() {
                                Some('|') => self
                                    .consume_and_return(chars, Token::QuestionMarkDashVerticalBar),
                                _ => Ok(Some(Token::QuestionMarkDash)),
                            }
                        }
                        Some('#') => self.consume_and_return(chars, Token::QuestionMarkSharp),
                        _ => Ok(Some(Token::Question)),
                    }
                }
                '?' => {
                    chars.next();
                    let s = peeking_take_while(chars, |ch| ch.is_numeric());
                    Ok(Some(Token::Placeholder(format!("?{s}"))))
                }

                // identifier or keyword
                ch if self.dialect.is_identifier_start(ch) => {
                    self.tokenize_identifier_or_keyword([ch], chars)
                }
                '$' => Ok(Some(self.tokenize_dollar_preceded_value(chars)?)),

                // whitespace check (including unicode chars) should be last as it covers some of the chars above
                ch if ch.is_whitespace() => {
                    self.consume_and_return(chars, Token::Whitespace(Whitespace::Space))
                }
                other => self.consume_and_return(chars, Token::Char(other)),
            },
            None => Ok(None),
        }
    }

    /// Consume the next character, then parse a custom binary operator. The next character should be included in the prefix
    fn consume_for_binop(
        &self,
        chars: &mut State,
        prefix: &str,
        default: Token,
    ) -> Result<Option<Token>, TokenizerError> {
        chars.next(); // consume the first char
        self.start_binop_opt(chars, prefix, Some(default))
    }

    /// parse a custom binary operator
    fn start_binop(
        &self,
        chars: &mut State,
        prefix: &str,
        default: Token,
    ) -> Result<Option<Token>, TokenizerError> {
        self.start_binop_opt(chars, prefix, Some(default))
    }

    /// parse a custom binary operator
    fn start_binop_opt(
        &self,
        chars: &mut State,
        prefix: &str,
        default: Option<Token>,
    ) -> Result<Option<Token>, TokenizerError> {
        let mut custom = None;
        while let Some(&ch) = chars.peek() {
            if !self.dialect.is_custom_operator_part(ch) {
                break;
            }

            custom.get_or_insert_with(|| prefix.to_string()).push(ch);
            chars.next();
        }
        match (custom, default) {
            (Some(custom), _) => Ok(Token::CustomBinaryOperator(custom).into()),
            (None, Some(tok)) => Ok(Some(tok)),
            (None, None) => self.tokenizer_error(
                chars.location(),
                format!("Expected a valid binary operator after '{prefix}'"),
            ),
        }
    }

    /// Tokenize dollar preceded value (i.e: a string/placeholder)
    fn tokenize_dollar_preceded_value(&self, chars: &mut State) -> Result<Token, TokenizerError> {
        let mut s = String::new();
        let mut value = String::new();

        chars.next();

        // If the dialect does not support dollar-quoted strings, then `$$` is rather a placeholder.
        if matches!(chars.peek(), Some('$')) && !self.dialect.supports_dollar_placeholder() {
            chars.next();

            let mut is_terminated = false;
            let mut prev: Option<char> = None;

            while let Some(&ch) = chars.peek() {
                if prev == Some('$') {
                    if ch == '$' {
                        chars.next();
                        is_terminated = true;
                        break;
                    } else {
                        s.push('$');
                        s.push(ch);
                    }
                } else if ch != '$' {
                    s.push(ch);
                }

                prev = Some(ch);
                chars.next();
            }

            return if chars.peek().is_none() && !is_terminated {
                self.tokenizer_error(chars.location(), "Unterminated dollar-quoted string")
            } else {
                Ok(Token::DollarQuotedString(DollarQuotedString {
                    value: s,
                    tag: None,
                }))
            };
        } else {
            value.push_str(&peeking_take_while(chars, |ch| {
                ch.is_alphanumeric()
                    || ch == '_'
                    // Allow $ as a placeholder character if the dialect supports it
                    || matches!(ch, '$' if self.dialect.supports_dollar_placeholder())
            }));

            // If the dialect supports a dollar sign as a money prefix (e.g. SQL Server),
            // and the value so far is all digits, check for a decimal part, e.g. `$123.45`
            if matches!(chars.peek(), Some('.'))
                && self.dialect.supports_dollar_as_money_prefix()
                && !value.is_empty()
                && value.chars().all(|c| c.is_ascii_digit())
            {
                value.push('.');
                chars.next();
                value.push_str(&peeking_take_while(chars, |ch| ch.is_ascii_digit()));
                return Ok(Token::Placeholder(format!("${value}")));
            }

            // If the dialect does not support dollar-quoted strings, don't look for the end delimiter.
            if matches!(chars.peek(), Some('$')) && !self.dialect.supports_dollar_placeholder() {
                chars.next();

                let mut temp = String::new();
                let end_delimiter = format!("${value}$");

                loop {
                    match chars.next() {
                        Some(ch) => {
                            temp.push(ch);

                            if temp.ends_with(&end_delimiter) {
                                if let Some(temp) = temp.strip_suffix(&end_delimiter) {
                                    s.push_str(temp);
                                }
                                break;
                            }
                        }
                        None => {
                            if temp.ends_with(&end_delimiter) {
                                if let Some(temp) = temp.strip_suffix(&end_delimiter) {
                                    s.push_str(temp);
                                }
                                break;
                            }

                            return self.tokenizer_error(
                                chars.location(),
                                "Unterminated dollar-quoted, expected $",
                            );
                        }
                    }
                }
            } else {
                return Ok(Token::Placeholder(format!("${value}")));
            }
        }

        Ok(Token::DollarQuotedString(DollarQuotedString {
            value: s,
            tag: if value.is_empty() { None } else { Some(value) },
        }))
    }

    fn tokenizer_error<R>(
        &self,
        loc: Location,
        message: impl Into<String>,
    ) -> Result<R, TokenizerError> {
        Err(TokenizerError {
            source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
            message: message.into(),
            location: loc,
        })
    }

    // Consume characters until newline
    fn tokenize_single_line_comment(&self, chars: &mut State) -> String {
        let mut comment = peeking_take_while(chars, |ch| match ch {
            '\n' => false,                                           // Always stop at \n
            '\r' if dialect_of!(self is PostgreSqlDialect) => false, // Stop at \r for Postgres
            _ => true, // Keep consuming for other characters
        });

        if let Some(ch) = chars.next() {
            assert!(ch == '\n' || ch == '\r');
            comment.push(ch);
        }

        comment
    }

    /// Tokenize an identifier or keyword, after the first char is already consumed.
    fn tokenize_word(&self, first_chars: impl Into<String>, chars: &mut State) -> String {
        let mut s = first_chars.into();
        s.push_str(&peeking_take_while(chars, |ch| {
            self.dialect.is_identifier_part(ch)
        }));
        s
    }

    /// Read a quoted identifier
    fn tokenize_quoted_identifier(
        &self,
        quote_start: char,
        chars: &mut State,
    ) -> Result<String, TokenizerError> {
        let error_loc = chars.location();
        chars.next(); // consume the opening quote
        let quote_end = Word::matching_end_quote(quote_start);
        let (s, last_char) = self.parse_quoted_ident(chars, quote_end);

        if last_char == Some(quote_end) {
            Ok(s)
        } else {
            self.tokenizer_error(
                error_loc,
                format!("Expected close delimiter '{quote_end}' before EOF."),
            )
        }
    }

    /// Read a single quoted string, starting with the opening quote.
    fn tokenize_escaped_single_quoted_string(
        &self,
        starting_loc: Location,
        chars: &mut State,
    ) -> Result<String, TokenizerError> {
        if let Some(s) = unescape_single_quoted_string(chars) {
            return Ok(s);
        }

        self.tokenizer_error(starting_loc, "Unterminated encoded string literal")
    }

    /// Reads a string literal quoted by a single or triple quote characters.
    /// Examples: `'abc'`, `'''abc'''`, `"""abc"""`.
    fn tokenize_single_or_triple_quoted_string<F>(
        &self,
        chars: &mut State,
        quote_style: char,
        backslash_escape: bool,
        single_quote_token: F,
        triple_quote_token: F,
    ) -> Result<Option<Token>, TokenizerError>
    where
        F: Fn(String) -> Token,
    {
        let error_loc = chars.location();

        let mut num_opening_quotes = 0u8;
        for _ in 0..3 {
            if Some(&quote_style) == chars.peek() {
                chars.next(); // Consume quote.
                num_opening_quotes += 1;
            } else {
                break;
            }
        }

        let (token_fn, num_quote_chars) = match num_opening_quotes {
            1 => (single_quote_token, NumStringQuoteChars::One),
            2 => {
                // If we matched double quotes, then this is an empty string.
                return Ok(Some(single_quote_token("".into())));
            }
            3 => {
                let Some(num_quote_chars) = NonZeroU8::new(3) else {
                    return self.tokenizer_error(error_loc, "invalid number of opening quotes");
                };
                (
                    triple_quote_token,
                    NumStringQuoteChars::Many(num_quote_chars),
                )
            }
            _ => {
                return self.tokenizer_error(error_loc, "invalid string literal opening");
            }
        };

        let settings = TokenizeQuotedStringSettings {
            quote_style,
            num_quote_chars,
            num_opening_quotes_to_consume: 0,
            backslash_escape,
        };

        self.tokenize_quoted_string(chars, settings)
            .map(token_fn)
            .map(Some)
    }

    /// Reads a string literal quoted by a single quote character.
    fn tokenize_single_quoted_string(
        &self,
        chars: &mut State,
        quote_style: char,
        backslash_escape: bool,
    ) -> Result<String, TokenizerError> {
        self.tokenize_quoted_string(
            chars,
            TokenizeQuotedStringSettings {
                quote_style,
                num_quote_chars: NumStringQuoteChars::One,
                num_opening_quotes_to_consume: 1,
                backslash_escape,
            },
        )
    }

    /// Reads a quote delimited string expecting `chars.next()` to deliver a quote.
    ///
    /// See <https://docs.oracle.com/en/database/oracle/oracle-database/21/sqlrf/Literals.html#GUID-1824CBAA-6E16-4921-B2A6-112FB02248DA>
    fn tokenize_quote_delimited_string(
        &self,
        chars: &mut State,
        // the prefix that introduced the possible literal or word,
        // e.g. "Q" or "nq"
        literal_prefix: &[char],
    ) -> Result<QuoteDelimitedString, TokenizerError> {
        let literal_start_loc = chars.location();
        chars.next();

        let start_quote_loc = chars.location();
        let (start_quote, end_quote) = match chars.next() {
            None | Some(' ') | Some('\t') | Some('\r') | Some('\n') => {
                return self.tokenizer_error(
                    start_quote_loc,
                    format!(
                        "Invalid space, tab, newline, or EOF after '{}''",
                        String::from_iter(literal_prefix)
                    ),
                );
            }
            Some(c) => (
                c,
                match c {
                    '[' => ']',
                    '{' => '}',
                    '<' => '>',
                    '(' => ')',
                    c => c,
                },
            ),
        };

        // read the string literal until the "quote character" following a by literal quote
        let mut value = String::new();
        while let Some(ch) = chars.next() {
            if ch == end_quote {
                if let Some('\'') = chars.peek() {
                    chars.next(); // ~ consume the quote
                    return Ok(QuoteDelimitedString {
                        start_quote,
                        value,
                        end_quote,
                    });
                }
            }
            value.push(ch);
        }

        self.tokenizer_error(literal_start_loc, "Unterminated string literal")
    }

    /// Read a quoted string.
    fn tokenize_quoted_string(
        &self,
        chars: &mut State,
        settings: TokenizeQuotedStringSettings,
    ) -> Result<String, TokenizerError> {
        let mut s = String::new();
        let error_loc = chars.location();

        // Consume any opening quotes.
        for _ in 0..settings.num_opening_quotes_to_consume {
            if Some(settings.quote_style) != chars.next() {
                return self.tokenizer_error(error_loc, "invalid string literal opening");
            }
        }

        let mut num_consecutive_quotes = 0;
        while let Some(&ch) = chars.peek() {
            let pending_final_quote = match settings.num_quote_chars {
                NumStringQuoteChars::One => Some(NumStringQuoteChars::One),
                n @ NumStringQuoteChars::Many(count)
                    if num_consecutive_quotes + 1 == count.get() =>
                {
                    Some(n)
                }
                NumStringQuoteChars::Many(_) => None,
            };

            match ch {
                char if char == settings.quote_style && pending_final_quote.is_some() => {
                    chars.next(); // consume

                    if let Some(NumStringQuoteChars::Many(count)) = pending_final_quote {
                        // For an initial string like `"""abc"""`, at this point we have
                        // `abc""` in the buffer and have now matched the final `"`.
                        // However, the string to return is simply `abc`, so we strip off
                        // the trailing quotes before returning.
                        let mut buf = s.chars();
                        for _ in 1..count.get() {
                            buf.next_back();
                        }
                        return Ok(buf.as_str().to_string());
                    } else if chars
                        .peek()
                        .map(|c| *c == settings.quote_style)
                        .unwrap_or(false)
                    {
                        s.push(ch);
                        if !self.unescape {
                            // In no-escape mode, the given query has to be saved completely
                            s.push(ch);
                        }
                        chars.next();
                    } else {
                        return Ok(s);
                    }
                }
                '\\' if settings.backslash_escape => {
                    // consume backslash
                    chars.next();

                    num_consecutive_quotes = 0;

                    if let Some(next) = chars.peek() {
                        if !self.unescape
                            || (self.dialect.ignores_wildcard_escapes()
                                && (*next == '%' || *next == '_'))
                        {
                            // In no-escape mode, the given query has to be saved completely
                            // including backslashes. Similarly, with ignore_like_wildcard_escapes,
                            // the backslash is not stripped.
                            s.push(ch);
                            s.push(*next);
                            chars.next(); // consume next
                        } else {
                            let n = match next {
                                '0' => '\0',
                                'a' => '\u{7}',
                                'b' => '\u{8}',
                                'f' => '\u{c}',
                                'n' => '\n',
                                'r' => '\r',
                                't' => '\t',
                                'Z' => '\u{1a}',
                                _ => *next,
                            };
                            s.push(n);
                            chars.next(); // consume next
                        }
                    }
                }
                ch => {
                    chars.next(); // consume ch

                    if ch == settings.quote_style {
                        num_consecutive_quotes += 1;
                    } else {
                        num_consecutive_quotes = 0;
                    }

                    s.push(ch);
                }
            }
        }
        self.tokenizer_error(error_loc, "Unterminated string literal")
    }

    fn tokenize_multiline_comment(
        &self,
        chars: &mut State,
    ) -> Result<Option<Token>, TokenizerError> {
        let mut s = String::new();
        let mut nested = 1;
        let supports_nested_comments = self.dialect.supports_nested_comments();
        loop {
            match chars.next() {
                Some('/') if matches!(chars.peek(), Some('*')) && supports_nested_comments => {
                    chars.next(); // consume the '*'
                    s.push('/');
                    s.push('*');
                    nested += 1;
                }
                Some('*') if matches!(chars.peek(), Some('/')) => {
                    chars.next(); // consume the '/'
                    nested -= 1;
                    if nested == 0 {
                        break Ok(Some(Token::Whitespace(Whitespace::MultiLineComment(s))));
                    }
                    s.push('*');
                    s.push('/');
                }
                Some(ch) => {
                    s.push(ch);
                }
                None => {
                    break self.tokenizer_error(
                        chars.location(),
                        "Unexpected EOF while in a multi-line comment",
                    );
                }
            }
        }
    }

    fn parse_quoted_ident(&self, chars: &mut State, quote_end: char) -> (String, Option<char>) {
        let mut last_char = None;
        let mut s = String::new();
        while let Some(ch) = chars.next() {
            if ch == quote_end {
                if chars.peek() == Some(&quote_end) {
                    chars.next();
                    s.push(ch);
                    if !self.unescape {
                        // In no-escape mode, the given query has to be saved completely
                        s.push(ch);
                    }
                } else {
                    last_char = Some(quote_end);
                    break;
                }
            } else {
                s.push(ch);
            }
        }
        (s, last_char)
    }

    #[allow(clippy::unnecessary_wraps)]
    fn consume_and_return(
        &self,
        chars: &mut State,
        t: Token,
    ) -> Result<Option<Token>, TokenizerError> {
        chars.next();
        Ok(Some(t))
    }
}

/// Read from `chars` until `predicate` returns `false` or EOF is hit.
/// Numeric temporary of the existing FSM. Raw capacity is admitted before
/// growth; decoded-input callers retain the same String semantics.
struct AdmittedTokenText {
    value: String,
    admission: Option<SqlSourceAdmission>,
}

impl AdmittedTokenText {
    fn new() -> Self {
        Self {
            value: String::new(),
            admission: None,
        }
    }
    fn reserve(&mut self, chars: &State<'_>, additional: usize) -> Result<(), TokenizerError> {
        let required = self
            .value
            .len()
            .checked_add(additional)
            .ok_or_else(|| TokenizerError {
                source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                message: "numeric token capacity overflow".into(),
                location: chars.location(),
            })?;
        if required > self.value.capacity() {
            let capacity = self
                .value
                .capacity()
                .max(8)
                .checked_mul(2)
                .map(|capacity| capacity.max(required))
                .ok_or_else(|| TokenizerError {
                    source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                    message: "numeric token capacity overflow".into(),
                    location: chars.location(),
                })?;
            let admission = chars
                .raw
                .as_ref()
                .map(|raw| raw.admit(capacity))
                .transpose()
                .map_err(|message| TokenizerError {
                    source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                    message,
                    location: chars.location(),
                })?;
            self.value.reserve_exact(capacity - self.value.len());
            self.admission = admission;
        }
        Ok(())
    }
    fn push(&mut self, chars: &State<'_>, value: char) -> Result<(), TokenizerError> {
        self.reserve(chars, value.len_utf8())?;
        self.value.push(value);
        Ok(())
    }
    fn append(&mut self, chars: &State<'_>, value: &str) -> Result<(), TokenizerError> {
        self.reserve(chars, value.len())?;
        self.value.push_str(value);
        Ok(())
    }
    fn into_string(self, chars: &mut State<'_>) -> Result<String, TokenizerError> {
        let location = chars.location();
        if let (Some(raw), Some(admission)) = (&mut chars.raw, self.admission) {
            raw.retain_admission(admission)
                .map_err(|message| TokenizerError {
                    message,
                    location,
                    source_kind: MySqlSourceErrorKind::Message,
                })?;
        }
        Ok(self.value)
    }
}

fn admitted_numeric_digits(
    chars: &mut State<'_>,
    mut predicate: impl FnMut(char, Option<char>) -> bool,
) -> Result<AdmittedTokenText, TokenizerError> {
    let mut text = AdmittedTokenText::new();
    while let Some(&value) = chars.peek() {
        // Only the existing underscore-separator rule requires a following
        // character. A rejected delimiter must not demand the next statement.
        if !predicate(value, None) && (value != '_' || !predicate(value, chars.peek_nth(1))) {
            break;
        }
        text.push(chars, value)?;
        chars.next();
    }
    Ok(text)
}

/// Read from `chars` until `predicate` returns `false` or EOF is hit.
/// Return the characters read as String, and keep the first non-matching
/// char available as `chars.next()`.
fn peeking_take_while(chars: &mut State, mut predicate: impl FnMut(char) -> bool) -> String {
    let mut s = String::new();
    while let Some(&ch) = chars.peek() {
        if predicate(ch) {
            chars.next(); // consume
            s.push(ch);
        } else {
            break;
        }
    }
    s
}

/// Packs MySQL bit-literal digits into the byte-oriented hexadecimal carrier
/// shared with MySQL hex literals. Bits are left padded to a full byte, as in
/// MySQL's `Item_bin_string::make_bin_str`.
fn mysql_bit_literal_hex(bits: &str) -> Option<String> {
    if !bits.bytes().all(|byte| matches!(byte, b'0' | b'1')) {
        return None;
    }
    let capacity = bits.len().div_ceil(8).checked_mul(2)?;
    let mut hex = String::with_capacity(capacity);
    let padding = (8 - bits.len() % 8) % 8;
    let mut value = 0_u8;
    for (ordinal, bit) in core::iter::repeat_n(b'0', padding)
        .chain(bits.bytes())
        .enumerate()
    {
        value = (value << 1) | (bit - b'0');
        if ordinal % 8 == 7 {
            hex.push(char::from_digit(u32::from(value >> 4), 16)?);
            hex.push(char::from_digit(u32::from(value & 15), 16)?);
            value = 0;
        }
    }
    Some(hex)
}

fn unescape_single_quoted_string(chars: &mut State<'_>) -> Option<String> {
    Unescape::new(chars).unescape()
}

struct Unescape<'a: 'b, 'b> {
    chars: &'b mut State<'a>,
}

impl<'a: 'b, 'b> Unescape<'a, 'b> {
    fn new(chars: &'b mut State<'a>) -> Self {
        Self { chars }
    }
    fn unescape(mut self) -> Option<String> {
        let mut unescaped = String::new();

        self.chars.next();

        while let Some(c) = self.chars.next() {
            if c == '\'' {
                // case: ''''
                if self.chars.peek().map(|c| *c == '\'').unwrap_or(false) {
                    self.chars.next();
                    unescaped.push('\'');
                    continue;
                }
                return Some(unescaped);
            }

            if c != '\\' {
                unescaped.push(c);
                continue;
            }

            let c = match self.chars.next()? {
                'b' => '\u{0008}',
                'f' => '\u{000C}',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                'u' => self.unescape_unicode_16()?,
                'U' => self.unescape_unicode_32()?,
                'x' => self.unescape_hex()?,
                c if c.is_digit(8) => self.unescape_octal(c)?,
                c => c,
            };

            unescaped.push(Self::check_null(c)?);
        }

        None
    }

    #[inline]
    fn check_null(c: char) -> Option<char> {
        if c == '\0' {
            None
        } else {
            Some(c)
        }
    }

    #[inline]
    fn byte_to_char<const RADIX: u32>(s: &str) -> Option<char> {
        // u32 is used here because Pg has an overflow operation rather than throwing an exception directly.
        match u32::from_str_radix(s, RADIX) {
            Err(_) => None,
            Ok(n) => {
                let n = n & 0xFF;
                if n <= 127 {
                    char::from_u32(n)
                } else {
                    None
                }
            }
        }
    }

    // Hexadecimal byte value. \xh, \xhh (h = 0–9, A–F)
    fn unescape_hex(&mut self) -> Option<char> {
        let mut s = String::new();

        for _ in 0..2 {
            match self.next_hex_digit() {
                Some(c) => s.push(c),
                None => break,
            }
        }

        if s.is_empty() {
            return Some('x');
        }

        Self::byte_to_char::<16>(&s)
    }

    #[inline]
    fn next_hex_digit(&mut self) -> Option<char> {
        match self.chars.peek() {
            Some(c) if c.is_ascii_hexdigit() => self.chars.next(),
            _ => None,
        }
    }

    // Octal byte value. \o, \oo, \ooo (o = 0–7)
    fn unescape_octal(&mut self, c: char) -> Option<char> {
        let mut s = String::new();

        s.push(c);
        for _ in 0..2 {
            match self.next_octal_digest() {
                Some(c) => s.push(c),
                None => break,
            }
        }

        Self::byte_to_char::<8>(&s)
    }

    #[inline]
    fn next_octal_digest(&mut self) -> Option<char> {
        match self.chars.peek() {
            Some(c) if c.is_digit(8) => self.chars.next(),
            _ => None,
        }
    }

    // 16-bit hexadecimal Unicode character value. \uxxxx (x = 0–9, A–F)
    fn unescape_unicode_16(&mut self) -> Option<char> {
        self.unescape_unicode::<4>()
    }

    // 32-bit hexadecimal Unicode character value. \Uxxxxxxxx (x = 0–9, A–F)
    fn unescape_unicode_32(&mut self) -> Option<char> {
        self.unescape_unicode::<8>()
    }

    fn unescape_unicode<const NUM: usize>(&mut self) -> Option<char> {
        let mut s = String::new();
        for _ in 0..NUM {
            s.push(self.chars.next()?);
        }
        match u32::from_str_radix(&s, 16) {
            Err(_) => None,
            Ok(n) => char::from_u32(n),
        }
    }
}

fn unescape_unicode_single_quoted_string(chars: &mut State<'_>) -> Result<String, TokenizerError> {
    let mut unescaped = String::new();
    chars.next(); // consume the opening quote
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    unescaped.push('\'');
                } else {
                    return Ok(unescaped);
                }
            }
            '\\' => match chars.peek() {
                Some('\\') => {
                    chars.next();
                    unescaped.push('\\');
                }
                Some('+') => {
                    chars.next();
                    unescaped.push(take_char_from_hex_digits(chars, 6)?);
                }
                _ => unescaped.push(take_char_from_hex_digits(chars, 4)?),
            },
            _ => {
                unescaped.push(c);
            }
        }
    }
    Err(TokenizerError {
        source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
        message: "Unterminated unicode encoded string literal".to_string(),
        location: chars.location(),
    })
}

fn take_char_from_hex_digits(
    chars: &mut State<'_>,
    max_digits: usize,
) -> Result<char, TokenizerError> {
    let mut result = 0u32;
    for _ in 0..max_digits {
        let next_char = chars.next().ok_or_else(|| TokenizerError {
            source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
            message: "Unexpected EOF while parsing hex digit in escaped unicode string."
                .to_string(),
            location: chars.location(),
        })?;
        let digit = next_char.to_digit(16).ok_or_else(|| TokenizerError {
            source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
            message: format!("Invalid hex digit in escaped unicode string: {next_char}"),
            location: chars.location(),
        })?;
        result = result * 16 + digit;
    }
    char::from_u32(result).ok_or_else(|| TokenizerError {
        source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
        message: format!("Invalid unicode character: {result:x}"),
        location: chars.location(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialect::{
        BigQueryDialect, ClickHouseDialect, HiveDialect, MsSqlDialect, MySqlDialect,
        PostgreSqlDialect, SQLiteDialect,
    };
    use crate::test_utils::{all_dialects, all_dialects_except, all_dialects_where};
    use core::fmt::Debug;

    #[test]
    fn tokenizer_error_impl() {
        let err = TokenizerError {
            source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
            message: "test".into(),
            location: Location { line: 1, column: 1 },
        };
        {
            use core::error::Error;
            assert!(err.source().is_none());
        }
        assert_eq!(err.to_string(), "test at Line: 1, Column: 1");
    }

    #[test]
    fn tokenize_select_1() {
        let sql = String::from("SELECT 1");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();

        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from("1"), false),
        ];

        compare(expected, tokens);
    }

    #[test]
    fn tokenize_select_float() {
        let sql = String::from("SELECT .1");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();

        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from(".1"), false),
        ];

        compare(expected, tokens);
    }

    #[test]
    fn tokenize_with_mapper() {
        let sql = String::from("SELECT ?");
        let dialect = GenericDialect {};
        let mut param_num = 1;

        let mut tokens = vec![];
        Tokenizer::new(&dialect, &sql)
            .tokenize_with_location_into_buf_with_mapper(&mut tokens, |mut token_span| {
                token_span.token = match token_span.token {
                    Token::Placeholder(n) => Token::Placeholder(if n == "?" {
                        let ret = format!("${}", param_num);
                        param_num += 1;
                        ret
                    } else {
                        n
                    }),
                    token => token,
                };
                token_span
            })
            .unwrap();
        let actual = tokens.into_iter().map(|t| t.token).collect();
        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::Placeholder("$1".to_string()),
        ];

        compare(expected, actual);
    }

    #[test]
    fn tokenize_clickhouse_double_equal() {
        let sql = String::from("SELECT foo=='1'");
        let dialect = ClickHouseDialect {};
        let mut tokenizer = Tokenizer::new(&dialect, &sql);
        let tokens = tokenizer.tokenize().unwrap();

        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::Word(Word {
                value: "foo".to_string(),
                quote_style: None,
                keyword: Keyword::NoKeyword,
            }),
            Token::DoubleEq,
            Token::SingleQuotedString("1".to_string()),
        ];

        compare(expected, tokens);
    }

    #[test]
    fn tokenize_numeric_literal_underscore() {
        let dialect = GenericDialect {};
        let sql = String::from("SELECT 10_000");
        let mut tokenizer = Tokenizer::new(&dialect, &sql);
        let tokens = tokenizer.tokenize().unwrap();
        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::Number("10".to_string(), false),
            Token::make_word("_000", None),
        ];
        compare(expected, tokens);

        all_dialects_where(|dialect| dialect.supports_numeric_literal_underscores()).tokenizes_to(
            "SELECT 10_000, _10_000, 10_00_, 10___0",
            vec![
                Token::make_keyword("SELECT"),
                Token::Whitespace(Whitespace::Space),
                Token::Number("10_000".to_string(), false),
                Token::Comma,
                Token::Whitespace(Whitespace::Space),
                Token::make_word("_10_000", None), // leading underscore tokenizes as a word (parsed as column identifier)
                Token::Comma,
                Token::Whitespace(Whitespace::Space),
                Token::Number("10_00".to_string(), false),
                Token::make_word("_", None), // trailing underscores tokenizes as a word (syntax error in some dialects)
                Token::Comma,
                Token::Whitespace(Whitespace::Space),
                Token::Number("10".to_string(), false),
                Token::make_word("___0", None), // multiple underscores tokenizes as a word (syntax error in some dialects)
            ],
        );
    }

    #[test]
    fn tokenize_select_exponent() {
        let sql = String::from("SELECT 1e10, 1e-10, 1e+10, 1ea, 1e-10a, 1e-10-10");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();

        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from("1e10"), false),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from("1e-10"), false),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from("1e+10"), false),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from("1"), false),
            Token::make_word("ea", None),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from("1e-10"), false),
            Token::make_word("a", None),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from("1e-10"), false),
            Token::Minus,
            Token::Number(String::from("10"), false),
        ];

        compare(expected, tokens);
    }

    #[test]
    fn tokenize_scalar_function() {
        let sql = String::from("SELECT sqrt(1)");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();

        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("sqrt", None),
            Token::LParen,
            Token::Number(String::from("1"), false),
            Token::RParen,
        ];

        compare(expected, tokens);
    }

    #[test]
    fn tokenize_string_string_concat() {
        let sql = String::from("SELECT 'a' || 'b'");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();

        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::SingleQuotedString(String::from("a")),
            Token::Whitespace(Whitespace::Space),
            Token::StringConcat,
            Token::Whitespace(Whitespace::Space),
            Token::SingleQuotedString(String::from("b")),
        ];

        compare(expected, tokens);
    }
    #[test]
    fn tokenize_bitwise_op() {
        let sql = String::from("SELECT one | two ^ three");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();

        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("one", None),
            Token::Whitespace(Whitespace::Space),
            Token::Pipe,
            Token::Whitespace(Whitespace::Space),
            Token::make_word("two", None),
            Token::Whitespace(Whitespace::Space),
            Token::Caret,
            Token::Whitespace(Whitespace::Space),
            Token::make_word("three", None),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_logical_xor() {
        let sql =
            String::from("SELECT true XOR true, false XOR false, true XOR false, false XOR true");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();

        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("true"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("XOR"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("true"),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("false"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("XOR"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("false"),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("true"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("XOR"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("false"),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("false"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("XOR"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("true"),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_simple_select() {
        let sql = String::from("SELECT * FROM customer WHERE id = 1 LIMIT 5");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();

        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::Mul,
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("FROM"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("customer", None),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("WHERE"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("id", None),
            Token::Whitespace(Whitespace::Space),
            Token::Eq,
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from("1"), false),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("LIMIT"),
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from("5"), false),
        ];

        compare(expected, tokens);
    }

    #[test]
    fn tokenize_explain_select() {
        let sql = String::from("EXPLAIN SELECT * FROM customer WHERE id = 1");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();

        let expected = vec![
            Token::make_keyword("EXPLAIN"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::Mul,
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("FROM"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("customer", None),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("WHERE"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("id", None),
            Token::Whitespace(Whitespace::Space),
            Token::Eq,
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from("1"), false),
        ];

        compare(expected, tokens);
    }

    #[test]
    fn tokenize_explain_analyze_select() {
        let sql = String::from("EXPLAIN ANALYZE SELECT * FROM customer WHERE id = 1");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();

        let expected = vec![
            Token::make_keyword("EXPLAIN"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("ANALYZE"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::Mul,
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("FROM"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("customer", None),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("WHERE"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("id", None),
            Token::Whitespace(Whitespace::Space),
            Token::Eq,
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from("1"), false),
        ];

        compare(expected, tokens);
    }

    #[test]
    fn tokenize_string_predicate() {
        let sql = String::from("SELECT * FROM customer WHERE salary != 'Not Provided'");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();

        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::Mul,
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("FROM"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("customer", None),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("WHERE"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("salary", None),
            Token::Whitespace(Whitespace::Space),
            Token::Neq,
            Token::Whitespace(Whitespace::Space),
            Token::SingleQuotedString(String::from("Not Provided")),
        ];

        compare(expected, tokens);
    }

    #[test]
    fn tokenize_invalid_string() {
        let sql = String::from("\n💝مصطفىh");

        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        // println!("tokens: {:#?}", tokens);
        let expected = vec![
            Token::Whitespace(Whitespace::Newline),
            Token::Char('💝'),
            Token::make_word("مصطفىh", None),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_newline_in_string_literal() {
        let sql = String::from("'foo\r\nbar\nbaz'");

        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        let expected = vec![Token::SingleQuotedString("foo\r\nbar\nbaz".to_string())];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_unterminated_string_literal() {
        let sql = String::from("select 'foo");

        let dialect = GenericDialect {};
        let mut tokenizer = Tokenizer::new(&dialect, &sql);
        assert_eq!(
            tokenizer.tokenize(),
            Err(TokenizerError {
                source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                message: "Unterminated string literal".to_string(),
                location: Location { line: 1, column: 8 },
            })
        );
    }

    #[test]
    fn tokenize_unterminated_string_literal_utf8() {
        let sql = String::from("SELECT \"なにか\" FROM Y WHERE \"なにか\" = 'test;");

        let dialect = GenericDialect {};
        let mut tokenizer = Tokenizer::new(&dialect, &sql);
        assert_eq!(
            tokenizer.tokenize(),
            Err(TokenizerError {
                source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                message: "Unterminated string literal".to_string(),
                location: Location {
                    line: 1,
                    column: 35
                }
            })
        );
    }

    #[test]
    fn tokenize_invalid_string_cols() {
        let sql = String::from("\n\nSELECT * FROM table\t💝مصطفىh");

        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        // println!("tokens: {:#?}", tokens);
        let expected = vec![
            Token::Whitespace(Whitespace::Newline),
            Token::Whitespace(Whitespace::Newline),
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::Mul,
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("FROM"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("table"),
            Token::Whitespace(Whitespace::Tab),
            Token::Char('💝'),
            Token::make_word("مصطفىh", None),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_dollar_quoted_string_tagged() {
        let test_cases = vec![
            (
                String::from(
                    "SELECT $tag$dollar '$' quoted strings have $tags like this$ or like this $$$tag$",
                ),
                vec![
                    Token::make_keyword("SELECT"),
                    Token::Whitespace(Whitespace::Space),
                    Token::DollarQuotedString(DollarQuotedString {
                        value: "dollar '$' quoted strings have $tags like this$ or like this $$"
                            .into(),
                        tag: Some("tag".into()),
                    }),
                ],
            ),
            (
                String::from("SELECT $abc$x$ab$abc$"),
                vec![
                    Token::make_keyword("SELECT"),
                    Token::Whitespace(Whitespace::Space),
                    Token::DollarQuotedString(DollarQuotedString {
                        value: "x$ab".into(),
                        tag: Some("abc".into()),
                    }),
                ],
            ),
            (
                String::from("SELECT $abc$$abc$"),
                vec![
                    Token::make_keyword("SELECT"),
                    Token::Whitespace(Whitespace::Space),
                    Token::DollarQuotedString(DollarQuotedString {
                        value: "".into(),
                        tag: Some("abc".into()),
                    }),
                ],
            ),
            (
                String::from("0$abc$$abc$1"),
                vec![
                    Token::Number("0".into(), false),
                    Token::DollarQuotedString(DollarQuotedString {
                        value: "".into(),
                        tag: Some("abc".into()),
                    }),
                    Token::Number("1".into(), false),
                ],
            ),
            (
                String::from("$function$abc$q$data$q$$function$"),
                vec![Token::DollarQuotedString(DollarQuotedString {
                    value: "abc$q$data$q$".into(),
                    tag: Some("function".into()),
                })],
            ),
        ];

        let dialect = GenericDialect {};
        for (sql, expected) in test_cases {
            let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
            compare(expected, tokens);
        }
    }

    #[test]
    fn tokenize_dollar_quoted_string_tagged_unterminated() {
        let sql = String::from(
            "SELECT $tag$dollar '$' quoted strings have $tags like this$ or like this $$$different tag$",
        );
        let dialect = GenericDialect {};
        assert_eq!(
            Tokenizer::new(&dialect, &sql).tokenize(),
            Err(TokenizerError {
                source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                message: "Unterminated dollar-quoted, expected $".into(),
                location: Location {
                    line: 1,
                    column: 91
                }
            })
        );
    }

    #[test]
    fn tokenize_dollar_quoted_string_tagged_unterminated_mirror() {
        let sql = String::from("SELECT $abc$abc$");
        let dialect = GenericDialect {};
        assert_eq!(
            Tokenizer::new(&dialect, &sql).tokenize(),
            Err(TokenizerError {
                source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                message: "Unterminated dollar-quoted, expected $".into(),
                location: Location {
                    line: 1,
                    column: 17
                }
            })
        );
    }

    #[test]
    fn tokenize_dollar_placeholder() {
        let sql = String::from("SELECT $$, $$ABC$$, $ABC$, $ABC");
        let dialect = SQLiteDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::make_keyword("SELECT"),
                Token::Whitespace(Whitespace::Space),
                Token::Placeholder("$$".into()),
                Token::Comma,
                Token::Whitespace(Whitespace::Space),
                Token::Placeholder("$$ABC$$".into()),
                Token::Comma,
                Token::Whitespace(Whitespace::Space),
                Token::Placeholder("$ABC$".into()),
                Token::Comma,
                Token::Whitespace(Whitespace::Space),
                Token::Placeholder("$ABC".into()),
            ]
        );
    }

    #[test]
    fn tokenize_nested_dollar_quoted_strings() {
        let sql = String::from("SELECT $tag$dollar $nested$ string$tag$");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::DollarQuotedString(DollarQuotedString {
                value: "dollar $nested$ string".into(),
                tag: Some("tag".into()),
            }),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_dollar_quoted_string_untagged_empty() {
        let sql = String::from("SELECT $$$$");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::DollarQuotedString(DollarQuotedString {
                value: "".into(),
                tag: None,
            }),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_dollar_quoted_string_untagged() {
        let sql =
            String::from("SELECT $$within dollar '$' quoted strings have $tags like this$ $$");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::DollarQuotedString(DollarQuotedString {
                value: "within dollar '$' quoted strings have $tags like this$ ".into(),
                tag: None,
            }),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_dollar_quoted_string_untagged_unterminated() {
        let sql = String::from(
            "SELECT $$dollar '$' quoted strings have $tags like this$ or like this $different tag$",
        );
        let dialect = GenericDialect {};
        assert_eq!(
            Tokenizer::new(&dialect, &sql).tokenize(),
            Err(TokenizerError {
                source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                message: "Unterminated dollar-quoted string".into(),
                location: Location {
                    line: 1,
                    column: 86
                }
            })
        );
    }

    #[test]
    fn tokenize_right_arrow() {
        let sql = String::from("FUNCTION(key=>value)");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        let expected = vec![
            Token::make_word("FUNCTION", None),
            Token::LParen,
            Token::make_word("key", None),
            Token::RArrow,
            Token::make_word("value", None),
            Token::RParen,
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_is_null() {
        let sql = String::from("a IS NULL");
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();

        let expected = vec![
            Token::make_word("a", None),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("IS"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("NULL"),
        ];

        compare(expected, tokens);
    }

    #[test]
    fn tokenize_comment() {
        let test_cases = vec![
            (
                String::from("0--this is a comment\n1"),
                vec![
                    Token::Number("0".to_string(), false),
                    Token::Whitespace(Whitespace::SingleLineComment {
                        prefix: "--".to_string(),
                        comment: "this is a comment\n".to_string(),
                    }),
                    Token::Number("1".to_string(), false),
                ],
            ),
            (
                String::from("0--this is a comment\r1"),
                vec![
                    Token::Number("0".to_string(), false),
                    Token::Whitespace(Whitespace::SingleLineComment {
                        prefix: "--".to_string(),
                        comment: "this is a comment\r1".to_string(),
                    }),
                ],
            ),
            (
                String::from("0--this is a comment\r\n1"),
                vec![
                    Token::Number("0".to_string(), false),
                    Token::Whitespace(Whitespace::SingleLineComment {
                        prefix: "--".to_string(),
                        comment: "this is a comment\r\n".to_string(),
                    }),
                    Token::Number("1".to_string(), false),
                ],
            ),
        ];

        let dialect = GenericDialect {};

        for (sql, expected) in test_cases {
            let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
            compare(expected, tokens);
        }
    }

    #[test]
    fn tokenize_comment_postgres() {
        let sql = String::from("1--\r0");

        let dialect = PostgreSqlDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        let expected = vec![
            Token::Number("1".to_string(), false),
            Token::Whitespace(Whitespace::SingleLineComment {
                prefix: "--".to_string(),
                comment: "\r".to_string(),
            }),
            Token::Number("0".to_string(), false),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_comment_at_eof() {
        let sql = String::from("--this is a comment");

        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        let expected = vec![Token::Whitespace(Whitespace::SingleLineComment {
            prefix: "--".to_string(),
            comment: "this is a comment".to_string(),
        })];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_multiline_comment() {
        let sql = String::from("0/*multi-line\n* /comment*/1");

        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        let expected = vec![
            Token::Number("0".to_string(), false),
            Token::Whitespace(Whitespace::MultiLineComment(
                "multi-line\n* /comment".to_string(),
            )),
            Token::Number("1".to_string(), false),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_nested_multiline_comment() {
        all_dialects_where(|d| d.supports_nested_comments()).tokenizes_to(
            "0/*multi-line\n* \n/* comment \n /*comment*/*/ */ /comment*/1",
            vec![
                Token::Number("0".to_string(), false),
                Token::Whitespace(Whitespace::MultiLineComment(
                    "multi-line\n* \n/* comment \n /*comment*/*/ ".into(),
                )),
                Token::Whitespace(Whitespace::Space),
                Token::Div,
                Token::Word(Word {
                    value: "comment".to_string(),
                    quote_style: None,
                    keyword: Keyword::COMMENT,
                }),
                Token::Mul,
                Token::Div,
                Token::Number("1".to_string(), false),
            ],
        );

        all_dialects_where(|d| d.supports_nested_comments()).tokenizes_to(
            "0/*multi-line\n* \n/* comment \n /*comment/**/ */ /comment*/*/1",
            vec![
                Token::Number("0".to_string(), false),
                Token::Whitespace(Whitespace::MultiLineComment(
                    "multi-line\n* \n/* comment \n /*comment/**/ */ /comment*/".into(),
                )),
                Token::Number("1".to_string(), false),
            ],
        );

        all_dialects_where(|d| d.supports_nested_comments()).tokenizes_to(
            "SELECT 1/* a /* b */ c */0",
            vec![
                Token::make_keyword("SELECT"),
                Token::Whitespace(Whitespace::Space),
                Token::Number("1".to_string(), false),
                Token::Whitespace(Whitespace::MultiLineComment(" a /* b */ c ".to_string())),
                Token::Number("0".to_string(), false),
            ],
        );
    }

    #[test]
    fn tokenize_nested_multiline_comment_empty() {
        all_dialects_where(|d| d.supports_nested_comments()).tokenizes_to(
            "select 1/*/**/*/0",
            vec![
                Token::make_keyword("select"),
                Token::Whitespace(Whitespace::Space),
                Token::Number("1".to_string(), false),
                Token::Whitespace(Whitespace::MultiLineComment("/**/".to_string())),
                Token::Number("0".to_string(), false),
            ],
        );
    }

    #[test]
    fn tokenize_nested_comments_if_not_supported() {
        all_dialects_except(|d| d.supports_nested_comments()).tokenizes_to(
            "SELECT 1/*/* nested comment */*/0",
            vec![
                Token::make_keyword("SELECT"),
                Token::Whitespace(Whitespace::Space),
                Token::Number("1".to_string(), false),
                Token::Whitespace(Whitespace::MultiLineComment(
                    "/* nested comment ".to_string(),
                )),
                Token::Mul,
                Token::Div,
                Token::Number("0".to_string(), false),
            ],
        );
    }

    #[test]
    fn tokenize_multiline_comment_with_even_asterisks() {
        let sql = String::from("\n/** Comment **/\n");

        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        let expected = vec![
            Token::Whitespace(Whitespace::Newline),
            Token::Whitespace(Whitespace::MultiLineComment("* Comment *".to_string())),
            Token::Whitespace(Whitespace::Newline),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_unicode_whitespace() {
        let sql = String::from(" \u{2003}\n");

        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        let expected = vec![
            Token::Whitespace(Whitespace::Space),
            Token::Whitespace(Whitespace::Space),
            Token::Whitespace(Whitespace::Newline),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_mismatched_quotes() {
        let sql = String::from("\"foo");

        let dialect = GenericDialect {};
        let mut tokenizer = Tokenizer::new(&dialect, &sql);
        assert_eq!(
            tokenizer.tokenize(),
            Err(TokenizerError {
                source_kind: crate::tokenizer::MySqlSourceErrorKind::Message,
                message: "Expected close delimiter '\"' before EOF.".to_string(),
                location: Location { line: 1, column: 1 },
            })
        );
    }

    #[test]
    fn tokenize_newlines() {
        let sql = String::from("line1\nline2\rline3\r\nline4\r");

        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        let expected = vec![
            Token::make_word("line1", None),
            Token::Whitespace(Whitespace::Newline),
            Token::make_word("line2", None),
            Token::Whitespace(Whitespace::Newline),
            Token::make_word("line3", None),
            Token::Whitespace(Whitespace::Newline),
            Token::make_word("line4", None),
            Token::Whitespace(Whitespace::Newline),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_mssql_top() {
        let sql = "SELECT TOP 5 [bar] FROM foo";
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();
        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("TOP"),
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from("5"), false),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("bar", Some('[')),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("FROM"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("foo", None),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_pg_regex_match() {
        let sql = "SELECT col ~ '^a', col ~* '^a', col !~ '^a', col !~* '^a'";
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();
        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("col", None),
            Token::Whitespace(Whitespace::Space),
            Token::Tilde,
            Token::Whitespace(Whitespace::Space),
            Token::SingleQuotedString("^a".into()),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::make_word("col", None),
            Token::Whitespace(Whitespace::Space),
            Token::TildeAsterisk,
            Token::Whitespace(Whitespace::Space),
            Token::SingleQuotedString("^a".into()),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::make_word("col", None),
            Token::Whitespace(Whitespace::Space),
            Token::ExclamationMarkTilde,
            Token::Whitespace(Whitespace::Space),
            Token::SingleQuotedString("^a".into()),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::make_word("col", None),
            Token::Whitespace(Whitespace::Space),
            Token::ExclamationMarkTildeAsterisk,
            Token::Whitespace(Whitespace::Space),
            Token::SingleQuotedString("^a".into()),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_pg_like_match() {
        let sql = "SELECT col ~~ '_a%', col ~~* '_a%', col !~~ '_a%', col !~~* '_a%'";
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();
        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("col", None),
            Token::Whitespace(Whitespace::Space),
            Token::DoubleTilde,
            Token::Whitespace(Whitespace::Space),
            Token::SingleQuotedString("_a%".into()),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::make_word("col", None),
            Token::Whitespace(Whitespace::Space),
            Token::DoubleTildeAsterisk,
            Token::Whitespace(Whitespace::Space),
            Token::SingleQuotedString("_a%".into()),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::make_word("col", None),
            Token::Whitespace(Whitespace::Space),
            Token::ExclamationMarkDoubleTilde,
            Token::Whitespace(Whitespace::Space),
            Token::SingleQuotedString("_a%".into()),
            Token::Comma,
            Token::Whitespace(Whitespace::Space),
            Token::make_word("col", None),
            Token::Whitespace(Whitespace::Space),
            Token::ExclamationMarkDoubleTildeAsterisk,
            Token::Whitespace(Whitespace::Space),
            Token::SingleQuotedString("_a%".into()),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_quoted_identifier() {
        let sql = r#" "a "" b" "a """ "c """"" "#;
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();
        let expected = vec![
            Token::Whitespace(Whitespace::Space),
            Token::make_word(r#"a " b"#, Some('"')),
            Token::Whitespace(Whitespace::Space),
            Token::make_word(r#"a ""#, Some('"')),
            Token::Whitespace(Whitespace::Space),
            Token::make_word(r#"c """#, Some('"')),
            Token::Whitespace(Whitespace::Space),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_snowflake_div() {
        let sql = r#"field/1000"#;
        let dialect = SnowflakeDialect {};
        let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();
        let expected = vec![
            Token::make_word(r#"field"#, None),
            Token::Div,
            Token::Number("1000".to_string(), false),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_quoted_identifier_with_no_escape() {
        let sql = r#" "a "" b" "a """ "c """"" "#;
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .with_unescape(false)
            .tokenize()
            .unwrap();
        let expected = vec![
            Token::Whitespace(Whitespace::Space),
            Token::make_word(r#"a "" b"#, Some('"')),
            Token::Whitespace(Whitespace::Space),
            Token::make_word(r#"a """#, Some('"')),
            Token::Whitespace(Whitespace::Space),
            Token::make_word(r#"c """""#, Some('"')),
            Token::Whitespace(Whitespace::Space),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_with_location() {
        let sql = "SELECT a,\n b";
        let dialect = GenericDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .unwrap();
        let expected = vec![
            TokenWithSpan::at(Token::make_keyword("SELECT"), (1, 1).into(), (1, 7).into()),
            TokenWithSpan::at(
                Token::Whitespace(Whitespace::Space),
                (1, 7).into(),
                (1, 8).into(),
            ),
            TokenWithSpan::at(Token::make_word("a", None), (1, 8).into(), (1, 9).into()),
            TokenWithSpan::at(Token::Comma, (1, 9).into(), (1, 10).into()),
            TokenWithSpan::at(
                Token::Whitespace(Whitespace::Newline),
                (1, 10).into(),
                (2, 1).into(),
            ),
            TokenWithSpan::at(
                Token::Whitespace(Whitespace::Space),
                (2, 1).into(),
                (2, 2).into(),
            ),
            TokenWithSpan::at(Token::make_word("b", None), (2, 2).into(), (2, 3).into()),
        ];
        compare(expected, tokens);
    }

    fn compare<T: PartialEq + fmt::Debug>(expected: Vec<T>, actual: Vec<T>) {
        //println!("------------------------------");
        //println!("tokens   = {:?}", actual);
        //println!("expected = {:?}", expected);
        //println!("------------------------------");
        assert_eq!(expected, actual);
    }

    fn check_unescape(s: &str, expected: Option<&str>) {
        let s = format!("'{s}'");
        let mut state = State {
            raw: None,
            peekable: s.chars().peekable(),
            line: 0,
            col: 0,
            cancellation: None,
            characters_since_cancellation_check: 0,
            cancelled: false,
        };

        assert_eq!(
            unescape_single_quoted_string(&mut state),
            expected.map(|s| s.to_string())
        );
    }

    #[test]
    fn test_unescape() {
        check_unescape(r"\b", Some("\u{0008}"));
        check_unescape(r"\f", Some("\u{000C}"));
        check_unescape(r"\t", Some("\t"));
        check_unescape(r"\r\n", Some("\r\n"));
        check_unescape(r"\/", Some("/"));
        check_unescape(r"/", Some("/"));
        check_unescape(r"\\", Some("\\"));

        // 16 and 32-bit hexadecimal Unicode character value
        check_unescape(r"\u0001", Some("\u{0001}"));
        check_unescape(r"\u4c91", Some("\u{4c91}"));
        check_unescape(r"\u4c916", Some("\u{4c91}6"));
        check_unescape(r"\u4c", None);
        check_unescape(r"\u0000", None);
        check_unescape(r"\U0010FFFF", Some("\u{10FFFF}"));
        check_unescape(r"\U00110000", None);
        check_unescape(r"\U00000000", None);
        check_unescape(r"\u", None);
        check_unescape(r"\U", None);
        check_unescape(r"\U1010FFFF", None);

        // hexadecimal byte value
        check_unescape(r"\x4B", Some("\u{004b}"));
        check_unescape(r"\x4", Some("\u{0004}"));
        check_unescape(r"\x4L", Some("\u{0004}L"));
        check_unescape(r"\x", Some("x"));
        check_unescape(r"\xP", Some("xP"));
        check_unescape(r"\x0", None);
        check_unescape(r"\xCAD", None);
        check_unescape(r"\xA9", None);

        // octal byte value
        check_unescape(r"\1", Some("\u{0001}"));
        check_unescape(r"\12", Some("\u{000a}"));
        check_unescape(r"\123", Some("\u{0053}"));
        check_unescape(r"\1232", Some("\u{0053}2"));
        check_unescape(r"\4", Some("\u{0004}"));
        check_unescape(r"\45", Some("\u{0025}"));
        check_unescape(r"\450", Some("\u{0028}"));
        check_unescape(r"\603", None);
        check_unescape(r"\0", None);
        check_unescape(r"\080", None);

        // others
        check_unescape(r"\9", Some("9"));
        check_unescape(r"''", Some("'"));
        check_unescape(
            r"Hello\r\nRust/\u4c91 SQL Parser\U0010ABCD\1232",
            Some("Hello\r\nRust/\u{4c91} SQL Parser\u{10abcd}\u{0053}2"),
        );
        check_unescape(r"Hello\0", None);
        check_unescape(r"Hello\xCADRust", None);
    }

    #[test]
    fn tokenize_numeric_prefix_trait() {
        #[derive(Debug)]
        struct NumericPrefixDialect;

        impl Dialect for NumericPrefixDialect {
            fn is_identifier_start(&self, ch: char) -> bool {
                ch.is_ascii_lowercase()
                    || ch.is_ascii_uppercase()
                    || ch.is_ascii_digit()
                    || ch == '$'
            }

            fn is_identifier_part(&self, ch: char) -> bool {
                ch.is_ascii_lowercase()
                    || ch.is_ascii_uppercase()
                    || ch.is_ascii_digit()
                    || ch == '_'
                    || ch == '$'
                    || ch == '{'
                    || ch == '}'
            }

            fn supports_numeric_prefix(&self) -> bool {
                true
            }
        }

        tokenize_numeric_prefix_inner(&NumericPrefixDialect {});
        tokenize_numeric_prefix_inner(&HiveDialect {});
        tokenize_numeric_prefix_inner(&MySqlDialect {});
    }

    fn tokenize_numeric_prefix_inner(dialect: &dyn Dialect) {
        let sql = r#"SELECT * FROM 1"#;
        let tokens = Tokenizer::new(dialect, sql).tokenize().unwrap();
        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::Mul,
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("FROM"),
            Token::Whitespace(Whitespace::Space),
            Token::Number(String::from("1"), false),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_quoted_string_escape() {
        let dialect = SnowflakeDialect {};
        for (sql, expected, expected_unescaped) in [
            (r#"'%a\'%b'"#, r#"%a\'%b"#, r#"%a'%b"#),
            (r#"'a\'\'b\'c\'d'"#, r#"a\'\'b\'c\'d"#, r#"a''b'c'd"#),
            (r#"'\\'"#, r#"\\"#, r#"\"#),
            (
                r#"'\0\a\b\f\n\r\t\Z'"#,
                r#"\0\a\b\f\n\r\t\Z"#,
                "\0\u{7}\u{8}\u{c}\n\r\t\u{1a}",
            ),
            (r#"'\"'"#, r#"\""#, "\""),
            (r#"'\\a\\b\'c'"#, r#"\\a\\b\'c"#, r#"\a\b'c"#),
            (r#"'\'abcd'"#, r#"\'abcd"#, r#"'abcd"#),
            (r#"'''a''b'"#, r#"''a''b"#, r#"'a'b"#),
            (r#"'\q'"#, r#"\q"#, r#"q"#),
            (r#"'\%\_'"#, r#"\%\_"#, r#"%_"#),
            (r#"'\\%\\_'"#, r#"\\%\\_"#, r#"\%\_"#),
        ] {
            let tokens = Tokenizer::new(&dialect, sql)
                .with_unescape(false)
                .tokenize()
                .unwrap();
            let expected = vec![Token::SingleQuotedString(expected.to_string())];
            compare(expected, tokens);

            let tokens = Tokenizer::new(&dialect, sql)
                .with_unescape(true)
                .tokenize()
                .unwrap();
            let expected = vec![Token::SingleQuotedString(expected_unescaped.to_string())];
            compare(expected, tokens);
        }

        for sql in [r#"'\'"#, r#"'ab\'"#] {
            let mut tokenizer = Tokenizer::new(&dialect, sql);
            assert_eq!(
                "Unterminated string literal",
                tokenizer.tokenize().unwrap_err().message.as_str(),
            );
        }

        // Non-escape dialect
        for (sql, expected) in [(r#"'\'"#, r#"\"#), (r#"'ab\'"#, r#"ab\"#)] {
            let dialect = GenericDialect {};
            let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();

            let expected = vec![Token::SingleQuotedString(expected.to_string())];

            compare(expected, tokens);
        }

        // MySQL special case for LIKE escapes
        for (sql, expected) in [(r#"'\%'"#, r#"\%"#), (r#"'\_'"#, r#"\_"#)] {
            let dialect = MySqlDialect {};
            let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();

            let expected = vec![Token::SingleQuotedString(expected.to_string())];

            compare(expected, tokens);
        }
    }

    #[test]
    fn tokenize_triple_quoted_string() {
        fn check<F>(
            q: char, // The quote character to test
            r: char, // An alternate quote character.
            quote_token: F,
        ) where
            F: Fn(String) -> Token,
        {
            let dialect = BigQueryDialect {};

            for (sql, expected, expected_unescaped) in [
                // Empty string
                (format!(r#"{q}{q}{q}{q}{q}{q}"#), "".into(), "".into()),
                // Should not count escaped quote as end of string.
                (
                    format!(r#"{q}{q}{q}ab{q}{q}\{q}{q}cd{q}{q}{q}"#),
                    format!(r#"ab{q}{q}\{q}{q}cd"#),
                    format!(r#"ab{q}{q}{q}{q}cd"#),
                ),
                // Simple string
                (
                    format!(r#"{q}{q}{q}abc{q}{q}{q}"#),
                    "abc".into(),
                    "abc".into(),
                ),
                // Mix single-double quotes unescaped.
                (
                    format!(r#"{q}{q}{q}ab{r}{r}{r}c{r}def{r}{r}{r}{q}{q}{q}"#),
                    format!("ab{r}{r}{r}c{r}def{r}{r}{r}"),
                    format!("ab{r}{r}{r}c{r}def{r}{r}{r}"),
                ),
                // Escaped quote.
                (
                    format!(r#"{q}{q}{q}ab{q}{q}c{q}{q}\{q}de{q}{q}f{q}{q}{q}"#),
                    format!(r#"ab{q}{q}c{q}{q}\{q}de{q}{q}f"#),
                    format!(r#"ab{q}{q}c{q}{q}{q}de{q}{q}f"#),
                ),
                // backslash-escaped quote characters.
                (
                    format!(r#"{q}{q}{q}a\'\'b\'c\'d{q}{q}{q}"#),
                    r#"a\'\'b\'c\'d"#.into(),
                    r#"a''b'c'd"#.into(),
                ),
                // backslash-escaped characters
                (
                    format!(r#"{q}{q}{q}abc\0\n\rdef{q}{q}{q}"#),
                    r#"abc\0\n\rdef"#.into(),
                    "abc\0\n\rdef".into(),
                ),
            ] {
                let tokens = Tokenizer::new(&dialect, sql.as_str())
                    .with_unescape(false)
                    .tokenize()
                    .unwrap();
                let expected = vec![quote_token(expected.to_string())];
                compare(expected, tokens);

                let tokens = Tokenizer::new(&dialect, sql.as_str())
                    .with_unescape(true)
                    .tokenize()
                    .unwrap();
                let expected = vec![quote_token(expected_unescaped.to_string())];
                compare(expected, tokens);
            }

            for sql in [
                format!(r#"{q}{q}{q}{q}{q}\{q}"#),
                format!(r#"{q}{q}{q}abc{q}{q}\{q}"#),
                format!(r#"{q}{q}{q}{q}"#),
                format!(r#"{q}{q}{q}{r}{r}"#),
                format!(r#"{q}{q}{q}abc{q}"#),
                format!(r#"{q}{q}{q}abc{q}{q}"#),
                format!(r#"{q}{q}{q}abc"#),
            ] {
                let dialect = BigQueryDialect {};
                let mut tokenizer = Tokenizer::new(&dialect, sql.as_str());
                assert_eq!(
                    "Unterminated string literal",
                    tokenizer.tokenize().unwrap_err().message.as_str(),
                );
            }
        }

        check('"', '\'', Token::TripleDoubleQuotedString);

        check('\'', '"', Token::TripleSingleQuotedString);

        let dialect = BigQueryDialect {};

        let sql = r#"""''"#;
        let tokens = Tokenizer::new(&dialect, sql)
            .with_unescape(true)
            .tokenize()
            .unwrap();
        let expected = vec![
            Token::DoubleQuotedString("".to_string()),
            Token::SingleQuotedString("".to_string()),
        ];
        compare(expected, tokens);

        let sql = r#"''"""#;
        let tokens = Tokenizer::new(&dialect, sql)
            .with_unescape(true)
            .tokenize()
            .unwrap();
        let expected = vec![
            Token::SingleQuotedString("".to_string()),
            Token::DoubleQuotedString("".to_string()),
        ];
        compare(expected, tokens);

        // Non-triple quoted string dialect
        let dialect = SnowflakeDialect {};
        let sql = r#"''''''"#;
        let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();
        let expected = vec![Token::SingleQuotedString("''".to_string())];
        compare(expected, tokens);
    }

    #[test]
    fn test_mysql_users_grantees() {
        let dialect = MySqlDialect {};

        let sql = "CREATE USER `root`@`%`";
        let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();
        let expected = vec![
            Token::make_keyword("CREATE"),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("USER"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("root", Some('`')),
            Token::AtSign,
            Token::make_word("%", Some('`')),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn test_postgres_abs_without_space_and_string_literal() {
        let dialect = MySqlDialect {};

        let sql = "SELECT @'1'";
        let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();
        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::AtSign,
            Token::SingleQuotedString("1".to_string()),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn test_postgres_abs_without_space_and_quoted_column() {
        let dialect = MySqlDialect {};

        let sql = r#"SELECT @"bar" FROM foo"#;
        let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();
        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::AtSign,
            Token::DoubleQuotedString("bar".to_string()),
            Token::Whitespace(Whitespace::Space),
            Token::make_keyword("FROM"),
            Token::Whitespace(Whitespace::Space),
            Token::make_word("foo", None),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn test_national_strings_backslash_escape_not_supported() {
        all_dialects_where(|dialect| !dialect.supports_string_literal_backslash_escape())
            .tokenizes_to(
                "select n'''''\\'",
                vec![
                    Token::make_keyword("select"),
                    Token::Whitespace(Whitespace::Space),
                    Token::NationalStringLiteral("''\\".to_string()),
                ],
            );
    }

    #[test]
    fn test_national_strings_backslash_escape_supported() {
        all_dialects_where(|dialect| dialect.supports_string_literal_backslash_escape())
            .tokenizes_to(
                "select n'''''\\''",
                vec![
                    Token::make_keyword("select"),
                    Token::Whitespace(Whitespace::Space),
                    Token::NationalStringLiteral("'''".to_string()),
                ],
            );
    }

    #[test]
    fn test_string_escape_constant_not_supported() {
        all_dialects_where(|dialect| !dialect.supports_string_escape_constant()).tokenizes_to(
            "select e'...'",
            vec![
                Token::make_keyword("select"),
                Token::Whitespace(Whitespace::Space),
                Token::make_word("e", None),
                Token::SingleQuotedString("...".to_string()),
            ],
        );

        all_dialects_where(|dialect| !dialect.supports_string_escape_constant()).tokenizes_to(
            "select E'...'",
            vec![
                Token::make_keyword("select"),
                Token::Whitespace(Whitespace::Space),
                Token::make_word("E", None),
                Token::SingleQuotedString("...".to_string()),
            ],
        );
    }

    #[test]
    fn test_string_escape_constant_supported() {
        all_dialects_where(|dialect| dialect.supports_string_escape_constant()).tokenizes_to(
            "select e'\\''",
            vec![
                Token::make_keyword("select"),
                Token::Whitespace(Whitespace::Space),
                Token::EscapedStringLiteral("'".to_string()),
            ],
        );

        all_dialects_where(|dialect| dialect.supports_string_escape_constant()).tokenizes_to(
            "select E'\\''",
            vec![
                Token::make_keyword("select"),
                Token::Whitespace(Whitespace::Space),
                Token::EscapedStringLiteral("'".to_string()),
            ],
        );
    }

    #[test]
    fn test_whitespace_required_after_single_line_comment() {
        all_dialects_where(|dialect| dialect.requires_single_line_comment_whitespace())
            .tokenizes_to(
                "SELECT --'abc'",
                vec![
                    Token::make_keyword("SELECT"),
                    Token::Whitespace(Whitespace::Space),
                    Token::Minus,
                    Token::Minus,
                    Token::SingleQuotedString("abc".to_string()),
                ],
            );

        all_dialects_where(|dialect| dialect.requires_single_line_comment_whitespace())
            .tokenizes_to(
                "SELECT -- 'abc'",
                vec![
                    Token::make_keyword("SELECT"),
                    Token::Whitespace(Whitespace::Space),
                    Token::Whitespace(Whitespace::SingleLineComment {
                        prefix: "--".to_string(),
                        comment: " 'abc'".to_string(),
                    }),
                ],
            );

        all_dialects_where(|dialect| dialect.requires_single_line_comment_whitespace())
            .tokenizes_to(
                "SELECT --",
                vec![
                    Token::make_keyword("SELECT"),
                    Token::Whitespace(Whitespace::Space),
                    Token::Minus,
                    Token::Minus,
                ],
            );

        all_dialects_where(|d| d.requires_single_line_comment_whitespace()).tokenizes_to(
            "--\n-- Table structure for table...\n--\n",
            vec![
                Token::Whitespace(Whitespace::SingleLineComment {
                    prefix: "--".to_string(),
                    comment: "\n".to_string(),
                }),
                Token::Whitespace(Whitespace::SingleLineComment {
                    prefix: "--".to_string(),
                    comment: " Table structure for table...\n".to_string(),
                }),
                Token::Whitespace(Whitespace::SingleLineComment {
                    prefix: "--".to_string(),
                    comment: "\n".to_string(),
                }),
            ],
        );
    }

    #[test]
    fn test_whitespace_not_required_after_single_line_comment() {
        all_dialects_where(|dialect| !dialect.requires_single_line_comment_whitespace())
            .tokenizes_to(
                "SELECT --'abc'",
                vec![
                    Token::make_keyword("SELECT"),
                    Token::Whitespace(Whitespace::Space),
                    Token::Whitespace(Whitespace::SingleLineComment {
                        prefix: "--".to_string(),
                        comment: "'abc'".to_string(),
                    }),
                ],
            );

        all_dialects_where(|dialect| !dialect.requires_single_line_comment_whitespace())
            .tokenizes_to(
                "SELECT -- 'abc'",
                vec![
                    Token::make_keyword("SELECT"),
                    Token::Whitespace(Whitespace::Space),
                    Token::Whitespace(Whitespace::SingleLineComment {
                        prefix: "--".to_string(),
                        comment: " 'abc'".to_string(),
                    }),
                ],
            );

        all_dialects_where(|dialect| !dialect.requires_single_line_comment_whitespace())
            .tokenizes_to(
                "SELECT --",
                vec![
                    Token::make_keyword("SELECT"),
                    Token::Whitespace(Whitespace::Space),
                    Token::Whitespace(Whitespace::SingleLineComment {
                        prefix: "--".to_string(),
                        comment: "".to_string(),
                    }),
                ],
            );
    }

    #[test]
    fn test_tokenize_identifiers_numeric_prefix() {
        all_dialects_where(|dialect| dialect.supports_numeric_prefix())
            .tokenizes_to("123abc", vec![Token::make_word("123abc", None)]);

        all_dialects_where(|dialect| dialect.supports_numeric_prefix())
            .tokenizes_to("12e34", vec![Token::Number("12e34".to_string(), false)]);

        all_dialects_where(|dialect| dialect.supports_numeric_prefix()).tokenizes_to(
            "t.12e34",
            vec![
                Token::make_word("t", None),
                Token::Period,
                Token::make_word("12e34", None),
            ],
        );

        all_dialects_where(|dialect| dialect.supports_numeric_prefix()).tokenizes_to(
            "t.1two3",
            vec![
                Token::make_word("t", None),
                Token::Period,
                Token::make_word("1two3", None),
            ],
        );
    }

    #[test]
    fn tokenize_period_underscore() {
        let sql = String::from("SELECT table._col");
        // a dialect that supports underscores in numeric literals
        let dialect = PostgreSqlDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();

        let expected = vec![
            Token::make_keyword("SELECT"),
            Token::Whitespace(Whitespace::Space),
            Token::Word(Word {
                value: "table".to_string(),
                quote_style: None,
                keyword: Keyword::TABLE,
            }),
            Token::Period,
            Token::Word(Word {
                value: "_col".to_string(),
                quote_style: None,
                keyword: Keyword::NoKeyword,
            }),
        ];

        compare(expected, tokens);

        let sql = String::from("SELECT ._123");
        if let Ok(tokens) = Tokenizer::new(&dialect, &sql).tokenize() {
            panic!("Tokenizer should have failed on {sql}, but it succeeded with {tokens:?}");
        }

        let sql = String::from("SELECT ._abc");
        if let Ok(tokens) = Tokenizer::new(&dialect, &sql).tokenize() {
            panic!("Tokenizer should have failed on {sql}, but it succeeded with {tokens:?}");
        }
    }

    #[test]
    fn tokenize_question_mark() {
        let dialect = PostgreSqlDialect {};
        let sql = "SELECT x ? y";
        let tokens = Tokenizer::new(&dialect, sql).tokenize().unwrap();
        compare(
            tokens,
            vec![
                Token::make_keyword("SELECT"),
                Token::Whitespace(Whitespace::Space),
                Token::make_word("x", None),
                Token::Whitespace(Whitespace::Space),
                Token::Question,
                Token::Whitespace(Whitespace::Space),
                Token::make_word("y", None),
            ],
        );
    }

    #[test]
    fn tokenize_multiline_comment_with_comment_hint() {
        let sql = String::from("0/*! word */1");

        let dialect = MySqlDialect {};
        let tokens = Tokenizer::new(&dialect, &sql).tokenize().unwrap();
        let expected = vec![
            Token::Number("0".to_string(), false),
            Token::Whitespace(Whitespace::Space),
            Token::Word(Word {
                value: "word".to_string(),
                quote_style: None,
                keyword: Keyword::NoKeyword,
            }),
            Token::Whitespace(Whitespace::Space),
            Token::Number("1".to_string(), false),
        ];
        compare(expected, tokens);
    }

    #[test]
    fn tokenize_multiline_comment_with_comment_hint_and_version() {
        let sql_multi = String::from("0 /*!50110 KEY_BLOCK_SIZE = 1024*/ 1");
        let dialect = MySqlDialect {};
        let tokens = Tokenizer::new(&dialect, &sql_multi).tokenize().unwrap();
        let expected = vec![
            Token::Number("0".to_string(), false),
            Token::Whitespace(Whitespace::Space),
            Token::Whitespace(Whitespace::Space),
            Token::Word(Word {
                value: "KEY_BLOCK_SIZE".to_string(),
                quote_style: None,
                keyword: Keyword::KEY_BLOCK_SIZE,
            }),
            Token::Whitespace(Whitespace::Space),
            Token::Eq,
            Token::Whitespace(Whitespace::Space),
            Token::Number("1024".to_string(), false),
            Token::Whitespace(Whitespace::Space),
            Token::Number("1".to_string(), false),
        ];
        compare(expected, tokens);

        let tokens = Tokenizer::new(&dialect, "0 /*!50110 */ 1")
            .tokenize()
            .unwrap();
        compare(
            vec![
                Token::Number("0".to_string(), false),
                Token::Whitespace(Whitespace::Space),
                Token::Whitespace(Whitespace::Space),
                Token::Whitespace(Whitespace::Space),
                Token::Number("1".to_string(), false),
            ],
            tokens,
        );

        let tokens = Tokenizer::new(&dialect, "0 /*!*/ 1").tokenize().unwrap();
        compare(
            vec![
                Token::Number("0".to_string(), false),
                Token::Whitespace(Whitespace::Space),
                Token::Whitespace(Whitespace::Space),
                Token::Number("1".to_string(), false),
            ],
            tokens,
        );
        let tokens = Tokenizer::new(&dialect, "0 /*!   */ 1").tokenize().unwrap();
        compare(
            vec![
                Token::Number("0".to_string(), false),
                Token::Whitespace(Whitespace::Space),
                Token::Whitespace(Whitespace::Space),
                Token::Whitespace(Whitespace::Space),
                Token::Whitespace(Whitespace::Space),
                Token::Whitespace(Whitespace::Space),
                Token::Number("1".to_string(), false),
            ],
            tokens,
        );
    }

    #[test]
    fn tokenize_lt() {
        all_dialects().tokenizes_to(
            "select a <-50",
            vec![
                Token::make_keyword("select"),
                Token::Whitespace(Whitespace::Space),
                Token::make_word("a", None),
                Token::Whitespace(Whitespace::Space),
                Token::Lt,
                Token::Minus,
                Token::Number("50".to_string(), false),
            ],
        );
        all_dialects().tokenizes_to(
            "select a <+50",
            vec![
                Token::make_keyword("select"),
                Token::Whitespace(Whitespace::Space),
                Token::make_word("a", None),
                Token::Whitespace(Whitespace::Space),
                Token::Lt,
                Token::Plus,
                Token::Number("50".to_string(), false),
            ],
        );
        all_dialects().tokenizes_to(
            "select a <=-50",
            vec![
                Token::make_keyword("select"),
                Token::Whitespace(Whitespace::Space),
                Token::make_word("a", None),
                Token::Whitespace(Whitespace::Space),
                Token::LtEq,
                Token::Minus,
                Token::Number("50".to_string(), false),
            ],
        );
        all_dialects().tokenizes_to(
            "select a <=+50",
            vec![
                Token::make_keyword("select"),
                Token::Whitespace(Whitespace::Space),
                Token::make_word("a", None),
                Token::Whitespace(Whitespace::Space),
                Token::LtEq,
                Token::Plus,
                Token::Number("50".to_string(), false),
            ],
        );
        all_dialects_where(|d| d.supports_geometric_types()).tokenizes_to(
            "select a <->b",
            vec![
                Token::make_keyword("select"),
                Token::Whitespace(Whitespace::Space),
                Token::make_word("a", None),
                Token::Whitespace(Whitespace::Space),
                Token::TwoWayArrow,
                Token::make_word("b", None),
            ],
        );

        all_dialects().tokenizes_to(
            "select a <-b",
            vec![
                Token::make_keyword("select"),
                Token::Whitespace(Whitespace::Space),
                Token::make_word("a", None),
                Token::Whitespace(Whitespace::Space),
                Token::Lt,
                Token::Minus,
                Token::make_word("b", None),
            ],
        );
        all_dialects().tokenizes_to(
            "select a <+b",
            vec![
                Token::make_keyword("select"),
                Token::Whitespace(Whitespace::Space),
                Token::make_word("a", None),
                Token::Whitespace(Whitespace::Space),
                Token::Lt,
                Token::Plus,
                Token::make_word("b", None),
            ],
        );
    }
}
