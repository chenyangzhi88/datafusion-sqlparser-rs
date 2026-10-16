//! Table partition syntax keeps the key expression and its operator class together.
use super::{display_comma_separated, Expr, Ident, ObjectName};
#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, vec::Vec};
use core::fmt;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "visitor")]
use sqlparser_derive::{Visit, VisitMut};

/// One PostgreSQL partition key with its optional operator class.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub struct PartitionKey {
    /// COLLATE remains an explicit annotation on this same expression.
    pub expression: Expr,
    /// Optional operator class, resolved against the key's type.
    pub operator_class: Option<ObjectName>,
}
impl fmt::Display for PartitionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.expression)?;
        if let Some(class) = &self.operator_class {
            write!(f, " {class}")?;
        }
        Ok(())
    }
}

/// A table partition clause in the syntax of its SQL dialect.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub enum TablePartitionBy {
    /// Expression-based partition clauses of other SQL dialects.
    Expression(Box<Expr>),
    /// PostgreSQL partition strategy and its ordered keys.
    PostgreSql {
        /// RANGE, LIST, or HASH.
        strategy: Ident,
        /// Key expressions and their optional operator classes.
        keys: Vec<PartitionKey>,
    },
}
impl fmt::Display for TablePartitionBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expression(expression) => write!(f, "{expression}"),
            Self::PostgreSql { strategy, keys } => {
                write!(f, "{strategy}({})", display_comma_separated(keys))
            }
        }
    }
}
