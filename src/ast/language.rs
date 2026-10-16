//! PostgreSQL procedural language definitions and lifetime commands.
use super::{Ident, ObjectName};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::fmt;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "visitor")]
use sqlparser_derive::{Visit, VisitMut};
/// An explicit CREATE LANGUAGE definition. Handlerless forms use CREATE EXTENSION.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub struct CreateLanguage {
    /// Preserve existing OID, owner and ACL while replacing callbacks.
    pub or_replace: bool,
    /// Trusted languages permit USAGE grants to non-superusers.
    pub trusted: bool,
    /// Database-local language name.
    pub name: Ident,
    /// Zero-argument callback returning language_handler.
    pub handler: ObjectName,
    /// Optional one-internal-argument anonymous block callback.
    pub inline: Option<ObjectName>,
    /// Optional one-oid-argument definition validator.
    pub validator: Option<ObjectName>,
}
/// A language name change preserves its OID and routine dependencies.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub struct AlterLanguage {
    /// Current language name.
    pub name: Ident,
    /// New language name.
    pub new_name: Ident,
}
/// DROP LANGUAGE uses normal dependency RESTRICT/CASCADE semantics.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub struct DropLanguage {
    /// Missing languages produce NOTICE when true.
    pub if_exists: bool,
    /// Whole language identities selected for deletion.
    pub names: Vec<Ident>,
    /// Delete dependent functions when true.
    pub cascade: bool,
}
impl fmt::Display for CreateLanguage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "CREATE {}{}LANGUAGE {} HANDLER {}",
            if self.or_replace { "OR REPLACE " } else { "" },
            if self.trusted { "TRUSTED " } else { "" },
            self.name,
            self.handler
        )?;
        if let Some(v) = &self.inline {
            write!(f, " INLINE {v}")?;
        }
        if let Some(v) = &self.validator {
            write!(f, " VALIDATOR {v}")?;
        }
        Ok(())
    }
}
impl fmt::Display for AlterLanguage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "ALTER LANGUAGE {} RENAME TO {}",
            self.name, self.new_name
        )
    }
}
impl fmt::Display for DropLanguage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "DROP LANGUAGE {}{} {}",
            if self.if_exists { "IF EXISTS " } else { "" },
            super::display_comma_separated(&self.names),
            if self.cascade { "CASCADE" } else { "RESTRICT" }
        )
    }
}
