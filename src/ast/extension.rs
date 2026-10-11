//! PostgreSQL extension mutations retain structured object identities.
use super::{DataType, FunctionDesc, Ident, ObjectName};
use core::fmt;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "visitor")]
use sqlparser_derive::{Visit, VisitMut};
/// One installed extension mutation.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub struct AlterExtension {
    /// Installed extension name.
    pub name: Ident,
    /// Version, namespace, or membership mutation.
    pub operation: AlterExtensionOperation,
}
/// Actions supported by ALTER EXTENSION.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub enum AlterExtensionOperation {
    /// Follow update scripts to the requested or default version.
    Update {
        /// Optional target version.
        version: Option<Ident>,
    },
    /// Relocate the extension and its schema-addressable members.
    SetSchema(Ident),
    /// Attach or detach one existing whole SQL object.
    Member {
        /// True for ADD; false for DROP.
        add: bool,
        /// Structured object identity.
        object: ExtensionMember,
    },
}
/// An extension member's native SQL object identity.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub enum ExtensionMember {
    /// An object identified by a qualified name and a finite native kind.
    Named {
        /// Native object kind.
        kind: ExtensionMemberKind,
        /// Qualified object name.
        name: ObjectName,
    },
    /// A routine identity, including its input signature when specified.
    Routine {
        /// FUNCTION, PROCEDURE, ROUTINE, or AGGREGATE.
        kind: ExtensionRoutineKind,
        /// Name and input signature.
        routine: FunctionDesc,
    },
    /// A type or domain identity, including array modifiers.
    Type {
        /// True for DOMAIN.
        domain: bool,
        /// Type identity.
        data_type: DataType,
    },
    /// A cast's source and destination types.
    Cast {
        /// Source type.
        source: DataType,
        /// Destination type.
        target: DataType,
    },
    /// An operator's name and optional unary operands.
    Operator {
        /// Qualified operator name.
        name: ObjectName,
        /// NONE for a prefix operator.
        left: Option<DataType>,
        /// NONE for an absent right operand.
        right: Option<DataType>,
    },
    /// An operator class or family, scoped to its access method.
    OperatorClass {
        /// True for FAMILY.
        family: bool,
        /// Qualified name.
        name: ObjectName,
        /// Access method name.
        method: Ident,
    },
    /// A transform identified by its type and language.
    Transform {
        /// Transformed type.
        data_type: DataType,
        /// Language name.
        language: Ident,
    },
}
/// Native routine identity category.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub enum ExtensionRoutineKind {
    /// FUNCTION.
    Function,
    /// PROCEDURE.
    Procedure,
    /// ROUTINE.
    Routine,
    /// AGGREGATE.
    Aggregate,
}
/// Name-addressable extension member category.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub enum ExtensionMemberKind {
    /// ACCESS METHOD.
    AccessMethod,
    /// COLLATION.
    Collation,
    /// CONVERSION.
    Conversion,
    /// EVENT TRIGGER.
    EventTrigger,
    /// FOREIGN DATA WRAPPER.
    ForeignDataWrapper,
    /// FOREIGN TABLE.
    ForeignTable,
    /// LANGUAGE.
    Language,
    /// MATERIALIZED VIEW.
    MaterializedView,
    /// SCHEMA.
    Schema,
    /// SEQUENCE.
    Sequence,
    /// SERVER.
    Server,
    /// TABLE.
    Table,
    /// TEXT SEARCH CONFIGURATION.
    TextSearchConfiguration,
    /// TEXT SEARCH DICTIONARY.
    TextSearchDictionary,
    /// TEXT SEARCH PARSER.
    TextSearchParser,
    /// TEXT SEARCH TEMPLATE.
    TextSearchTemplate,
    /// VIEW.
    View,
    /// DATABASE; syntactically valid but membership is forbidden.
    Database,
    /// EXTENSION; syntactically valid but membership is forbidden.
    Extension,
    /// INDEX; syntactically valid but membership is forbidden.
    Index,
    /// PUBLICATION; syntactically valid but membership is forbidden.
    Publication,
    /// ROLE; syntactically valid but membership is forbidden.
    Role,
    /// STATISTICS; syntactically valid but membership is forbidden.
    Statistics,
    /// SUBSCRIPTION; syntactically valid but membership is forbidden.
    Subscription,
    /// TABLESPACE; syntactically valid but membership is forbidden.
    Tablespace,
}
impl fmt::Display for ExtensionRoutineKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Function => "FUNCTION",
            Self::Procedure => "PROCEDURE",
            Self::Routine => "ROUTINE",
            Self::Aggregate => "AGGREGATE",
        })
    }
}
impl fmt::Display for ExtensionMemberKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::AccessMethod => "ACCESS METHOD",
            Self::Collation => "COLLATION",
            Self::Conversion => "CONVERSION",
            Self::EventTrigger => "EVENT TRIGGER",
            Self::ForeignDataWrapper => "FOREIGN DATA WRAPPER",
            Self::ForeignTable => "FOREIGN TABLE",
            Self::Language => "LANGUAGE",
            Self::MaterializedView => "MATERIALIZED VIEW",
            Self::Schema => "SCHEMA",
            Self::Sequence => "SEQUENCE",
            Self::Server => "SERVER",
            Self::Table => "TABLE",
            Self::TextSearchConfiguration => "TEXT SEARCH CONFIGURATION",
            Self::TextSearchDictionary => "TEXT SEARCH DICTIONARY",
            Self::TextSearchParser => "TEXT SEARCH PARSER",
            Self::TextSearchTemplate => "TEXT SEARCH TEMPLATE",
            Self::View => "VIEW",
            Self::Database => "DATABASE",
            Self::Extension => "EXTENSION",
            Self::Index => "INDEX",
            Self::Publication => "PUBLICATION",
            Self::Role => "ROLE",
            Self::Statistics => "STATISTICS",
            Self::Subscription => "SUBSCRIPTION",
            Self::Tablespace => "TABLESPACE",
        })
    }
}
impl fmt::Display for ExtensionMember {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named { kind, name } => write!(f, "{kind} {name}"),
            Self::Routine { kind, routine } => write!(f, "{kind} {routine}"),
            Self::Type { domain, data_type } => {
                write!(f, "{} {data_type}", if *domain { "DOMAIN" } else { "TYPE" })
            }
            Self::Cast { source, target } => write!(f, "CAST ({source} AS {target})"),
            Self::Operator { name, left, right } => {
                write!(f, "OPERATOR {name} (")?;
                if let Some(t) = left {
                    write!(f, "{t}")?
                } else {
                    f.write_str("NONE")?
                };
                f.write_str(", ")?;
                if let Some(t) = right {
                    write!(f, "{t}")?
                } else {
                    f.write_str("NONE")?
                };
                f.write_str(")")
            }
            Self::OperatorClass {
                family,
                name,
                method,
            } => write!(
                f,
                "OPERATOR {} {name} USING {method}",
                if *family { "FAMILY" } else { "CLASS" }
            ),
            Self::Transform {
                data_type,
                language,
            } => write!(f, "TRANSFORM FOR {data_type} LANGUAGE {language}"),
        }
    }
}
impl fmt::Display for AlterExtension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ALTER EXTENSION {} ", self.name)?;
        match &self.operation {
            AlterExtensionOperation::Update { version } => {
                f.write_str("UPDATE")?;
                if let Some(v) = version {
                    write!(f, " TO {v}")?
                }
                Ok(())
            }
            AlterExtensionOperation::SetSchema(schema) => write!(f, "SET SCHEMA {schema}"),
            AlterExtensionOperation::Member { add, object } => {
                write!(f, "{} {object}", if *add { "ADD" } else { "DROP" })
            }
        }
    }
}
