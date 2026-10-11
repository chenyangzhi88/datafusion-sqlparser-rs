//! MySQL-specific keyword metadata.
//!
//! The registry is the exact output of MySQL 8.0.46's
//! `sql/gen_keyword_list.cc` for `sql/sql_yacc.yy`. Keep this separate from
//! the parser's cross-dialect keyword enum: INFORMATION_SCHEMA.KEYWORDS exposes
//! the server's complete vocabulary and reservation status, not merely tokens
//! used by this parser.

/// A keyword visible through MySQL `INFORMATION_SCHEMA.KEYWORDS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MySqlKeyword {
    /// Uppercase keyword spelling.
    pub word: &'static str,
    /// Whether MySQL 8.0.46 reserves this word.
    pub reserved: bool,
}

/// Complete MySQL 8.0.46 keyword registry, sorted by `word`.
pub const MYSQL_8_0_46_KEYWORDS: &[MySqlKeyword] = &[
    MySqlKeyword {
        word: "ACCESSIBLE",
        reserved: true,
    },
    MySqlKeyword {
        word: "ACCOUNT",
        reserved: false,
    },
    MySqlKeyword {
        word: "ACTION",
        reserved: false,
    },
    MySqlKeyword {
        word: "ACTIVE",
        reserved: false,
    },
    MySqlKeyword {
        word: "ADD",
        reserved: true,
    },
    MySqlKeyword {
        word: "ADMIN",
        reserved: false,
    },
    MySqlKeyword {
        word: "AFTER",
        reserved: false,
    },
    MySqlKeyword {
        word: "AGAINST",
        reserved: false,
    },
    MySqlKeyword {
        word: "AGGREGATE",
        reserved: false,
    },
    MySqlKeyword {
        word: "ALGORITHM",
        reserved: false,
    },
    MySqlKeyword {
        word: "ALL",
        reserved: true,
    },
    MySqlKeyword {
        word: "ALTER",
        reserved: true,
    },
    MySqlKeyword {
        word: "ALWAYS",
        reserved: false,
    },
    MySqlKeyword {
        word: "ANALYZE",
        reserved: true,
    },
    MySqlKeyword {
        word: "AND",
        reserved: true,
    },
    MySqlKeyword {
        word: "ANY",
        reserved: false,
    },
    MySqlKeyword {
        word: "ARRAY",
        reserved: false,
    },
    MySqlKeyword {
        word: "AS",
        reserved: true,
    },
    MySqlKeyword {
        word: "ASC",
        reserved: true,
    },
    MySqlKeyword {
        word: "ASCII",
        reserved: false,
    },
    MySqlKeyword {
        word: "ASENSITIVE",
        reserved: true,
    },
    MySqlKeyword {
        word: "ASSIGN_GTIDS_TO_ANONYMOUS_TRANSACTIONS",
        reserved: false,
    },
    MySqlKeyword {
        word: "AT",
        reserved: false,
    },
    MySqlKeyword {
        word: "ATTRIBUTE",
        reserved: false,
    },
    MySqlKeyword {
        word: "AUTHENTICATION",
        reserved: false,
    },
    MySqlKeyword {
        word: "AUTOEXTEND_SIZE",
        reserved: false,
    },
    MySqlKeyword {
        word: "AUTO_INCREMENT",
        reserved: false,
    },
    MySqlKeyword {
        word: "AVG",
        reserved: false,
    },
    MySqlKeyword {
        word: "AVG_ROW_LENGTH",
        reserved: false,
    },
    MySqlKeyword {
        word: "BACKUP",
        reserved: false,
    },
    MySqlKeyword {
        word: "BEFORE",
        reserved: true,
    },
    MySqlKeyword {
        word: "BEGIN",
        reserved: false,
    },
    MySqlKeyword {
        word: "BETWEEN",
        reserved: true,
    },
    MySqlKeyword {
        word: "BIGINT",
        reserved: true,
    },
    MySqlKeyword {
        word: "BINARY",
        reserved: true,
    },
    MySqlKeyword {
        word: "BINLOG",
        reserved: false,
    },
    MySqlKeyword {
        word: "BIT",
        reserved: false,
    },
    MySqlKeyword {
        word: "BLOB",
        reserved: true,
    },
    MySqlKeyword {
        word: "BLOCK",
        reserved: false,
    },
    MySqlKeyword {
        word: "BOOL",
        reserved: false,
    },
    MySqlKeyword {
        word: "BOOLEAN",
        reserved: false,
    },
    MySqlKeyword {
        word: "BOTH",
        reserved: true,
    },
    MySqlKeyword {
        word: "BTREE",
        reserved: false,
    },
    MySqlKeyword {
        word: "BUCKETS",
        reserved: false,
    },
    MySqlKeyword {
        word: "BULK",
        reserved: false,
    },
    MySqlKeyword {
        word: "BY",
        reserved: true,
    },
    MySqlKeyword {
        word: "BYTE",
        reserved: false,
    },
    MySqlKeyword {
        word: "CACHE",
        reserved: false,
    },
    MySqlKeyword {
        word: "CALL",
        reserved: true,
    },
    MySqlKeyword {
        word: "CASCADE",
        reserved: true,
    },
    MySqlKeyword {
        word: "CASCADED",
        reserved: false,
    },
    MySqlKeyword {
        word: "CASE",
        reserved: true,
    },
    MySqlKeyword {
        word: "CATALOG_NAME",
        reserved: false,
    },
    MySqlKeyword {
        word: "CHAIN",
        reserved: false,
    },
    MySqlKeyword {
        word: "CHALLENGE_RESPONSE",
        reserved: false,
    },
    MySqlKeyword {
        word: "CHANGE",
        reserved: true,
    },
    MySqlKeyword {
        word: "CHANGED",
        reserved: false,
    },
    MySqlKeyword {
        word: "CHANNEL",
        reserved: false,
    },
    MySqlKeyword {
        word: "CHAR",
        reserved: true,
    },
    MySqlKeyword {
        word: "CHARACTER",
        reserved: true,
    },
    MySqlKeyword {
        word: "CHARSET",
        reserved: false,
    },
    MySqlKeyword {
        word: "CHECK",
        reserved: true,
    },
    MySqlKeyword {
        word: "CHECKSUM",
        reserved: false,
    },
    MySqlKeyword {
        word: "CIPHER",
        reserved: false,
    },
    MySqlKeyword {
        word: "CLASS_ORIGIN",
        reserved: false,
    },
    MySqlKeyword {
        word: "CLIENT",
        reserved: false,
    },
    MySqlKeyword {
        word: "CLONE",
        reserved: false,
    },
    MySqlKeyword {
        word: "CLOSE",
        reserved: false,
    },
    MySqlKeyword {
        word: "COALESCE",
        reserved: false,
    },
    MySqlKeyword {
        word: "CODE",
        reserved: false,
    },
    MySqlKeyword {
        word: "COLLATE",
        reserved: true,
    },
    MySqlKeyword {
        word: "COLLATION",
        reserved: false,
    },
    MySqlKeyword {
        word: "COLUMN",
        reserved: true,
    },
    MySqlKeyword {
        word: "COLUMNS",
        reserved: false,
    },
    MySqlKeyword {
        word: "COLUMN_FORMAT",
        reserved: false,
    },
    MySqlKeyword {
        word: "COLUMN_NAME",
        reserved: false,
    },
    MySqlKeyword {
        word: "COMMENT",
        reserved: false,
    },
    MySqlKeyword {
        word: "COMMIT",
        reserved: false,
    },
    MySqlKeyword {
        word: "COMMITTED",
        reserved: false,
    },
    MySqlKeyword {
        word: "COMPACT",
        reserved: false,
    },
    MySqlKeyword {
        word: "COMPLETION",
        reserved: false,
    },
    MySqlKeyword {
        word: "COMPONENT",
        reserved: false,
    },
    MySqlKeyword {
        word: "COMPRESSED",
        reserved: false,
    },
    MySqlKeyword {
        word: "COMPRESSION",
        reserved: false,
    },
    MySqlKeyword {
        word: "CONCURRENT",
        reserved: false,
    },
    MySqlKeyword {
        word: "CONDITION",
        reserved: true,
    },
    MySqlKeyword {
        word: "CONNECTION",
        reserved: false,
    },
    MySqlKeyword {
        word: "CONSISTENT",
        reserved: false,
    },
    MySqlKeyword {
        word: "CONSTRAINT",
        reserved: true,
    },
    MySqlKeyword {
        word: "CONSTRAINT_CATALOG",
        reserved: false,
    },
    MySqlKeyword {
        word: "CONSTRAINT_NAME",
        reserved: false,
    },
    MySqlKeyword {
        word: "CONSTRAINT_SCHEMA",
        reserved: false,
    },
    MySqlKeyword {
        word: "CONTAINS",
        reserved: false,
    },
    MySqlKeyword {
        word: "CONTEXT",
        reserved: false,
    },
    MySqlKeyword {
        word: "CONTINUE",
        reserved: true,
    },
    MySqlKeyword {
        word: "CONVERT",
        reserved: true,
    },
    MySqlKeyword {
        word: "CPU",
        reserved: false,
    },
    MySqlKeyword {
        word: "CREATE",
        reserved: true,
    },
    MySqlKeyword {
        word: "CROSS",
        reserved: true,
    },
    MySqlKeyword {
        word: "CUBE",
        reserved: true,
    },
    MySqlKeyword {
        word: "CUME_DIST",
        reserved: true,
    },
    MySqlKeyword {
        word: "CURRENT",
        reserved: false,
    },
    MySqlKeyword {
        word: "CURRENT_DATE",
        reserved: true,
    },
    MySqlKeyword {
        word: "CURRENT_TIME",
        reserved: true,
    },
    MySqlKeyword {
        word: "CURRENT_TIMESTAMP",
        reserved: true,
    },
    MySqlKeyword {
        word: "CURRENT_USER",
        reserved: true,
    },
    MySqlKeyword {
        word: "CURSOR",
        reserved: true,
    },
    MySqlKeyword {
        word: "CURSOR_NAME",
        reserved: false,
    },
    MySqlKeyword {
        word: "DATA",
        reserved: false,
    },
    MySqlKeyword {
        word: "DATABASE",
        reserved: true,
    },
    MySqlKeyword {
        word: "DATABASES",
        reserved: true,
    },
    MySqlKeyword {
        word: "DATAFILE",
        reserved: false,
    },
    MySqlKeyword {
        word: "DATE",
        reserved: false,
    },
    MySqlKeyword {
        word: "DATETIME",
        reserved: false,
    },
    MySqlKeyword {
        word: "DAY",
        reserved: false,
    },
    MySqlKeyword {
        word: "DAY_HOUR",
        reserved: true,
    },
    MySqlKeyword {
        word: "DAY_MICROSECOND",
        reserved: true,
    },
    MySqlKeyword {
        word: "DAY_MINUTE",
        reserved: true,
    },
    MySqlKeyword {
        word: "DAY_SECOND",
        reserved: true,
    },
    MySqlKeyword {
        word: "DEALLOCATE",
        reserved: false,
    },
    MySqlKeyword {
        word: "DEC",
        reserved: true,
    },
    MySqlKeyword {
        word: "DECIMAL",
        reserved: true,
    },
    MySqlKeyword {
        word: "DECLARE",
        reserved: true,
    },
    MySqlKeyword {
        word: "DEFAULT",
        reserved: true,
    },
    MySqlKeyword {
        word: "DEFAULT_AUTH",
        reserved: false,
    },
    MySqlKeyword {
        word: "DEFINER",
        reserved: false,
    },
    MySqlKeyword {
        word: "DEFINITION",
        reserved: false,
    },
    MySqlKeyword {
        word: "DELAYED",
        reserved: true,
    },
    MySqlKeyword {
        word: "DELAY_KEY_WRITE",
        reserved: false,
    },
    MySqlKeyword {
        word: "DELETE",
        reserved: true,
    },
    MySqlKeyword {
        word: "DENSE_RANK",
        reserved: true,
    },
    MySqlKeyword {
        word: "DESC",
        reserved: true,
    },
    MySqlKeyword {
        word: "DESCRIBE",
        reserved: true,
    },
    MySqlKeyword {
        word: "DESCRIPTION",
        reserved: false,
    },
    MySqlKeyword {
        word: "DETERMINISTIC",
        reserved: true,
    },
    MySqlKeyword {
        word: "DIAGNOSTICS",
        reserved: false,
    },
    MySqlKeyword {
        word: "DIRECTORY",
        reserved: false,
    },
    MySqlKeyword {
        word: "DISABLE",
        reserved: false,
    },
    MySqlKeyword {
        word: "DISCARD",
        reserved: false,
    },
    MySqlKeyword {
        word: "DISK",
        reserved: false,
    },
    MySqlKeyword {
        word: "DISTINCT",
        reserved: true,
    },
    MySqlKeyword {
        word: "DISTINCTROW",
        reserved: true,
    },
    MySqlKeyword {
        word: "DIV",
        reserved: true,
    },
    MySqlKeyword {
        word: "DO",
        reserved: false,
    },
    MySqlKeyword {
        word: "DOUBLE",
        reserved: true,
    },
    MySqlKeyword {
        word: "DROP",
        reserved: true,
    },
    MySqlKeyword {
        word: "DUAL",
        reserved: true,
    },
    MySqlKeyword {
        word: "DUMPFILE",
        reserved: false,
    },
    MySqlKeyword {
        word: "DUPLICATE",
        reserved: false,
    },
    MySqlKeyword {
        word: "DYNAMIC",
        reserved: false,
    },
    MySqlKeyword {
        word: "EACH",
        reserved: true,
    },
    MySqlKeyword {
        word: "ELSE",
        reserved: true,
    },
    MySqlKeyword {
        word: "ELSEIF",
        reserved: true,
    },
    MySqlKeyword {
        word: "EMPTY",
        reserved: true,
    },
    MySqlKeyword {
        word: "ENABLE",
        reserved: false,
    },
    MySqlKeyword {
        word: "ENCLOSED",
        reserved: true,
    },
    MySqlKeyword {
        word: "ENCRYPTION",
        reserved: false,
    },
    MySqlKeyword {
        word: "END",
        reserved: false,
    },
    MySqlKeyword {
        word: "ENDS",
        reserved: false,
    },
    MySqlKeyword {
        word: "ENFORCED",
        reserved: false,
    },
    MySqlKeyword {
        word: "ENGINE",
        reserved: false,
    },
    MySqlKeyword {
        word: "ENGINES",
        reserved: false,
    },
    MySqlKeyword {
        word: "ENGINE_ATTRIBUTE",
        reserved: false,
    },
    MySqlKeyword {
        word: "ENUM",
        reserved: false,
    },
    MySqlKeyword {
        word: "ERROR",
        reserved: false,
    },
    MySqlKeyword {
        word: "ERRORS",
        reserved: false,
    },
    MySqlKeyword {
        word: "ESCAPE",
        reserved: false,
    },
    MySqlKeyword {
        word: "ESCAPED",
        reserved: true,
    },
    MySqlKeyword {
        word: "EVENT",
        reserved: false,
    },
    MySqlKeyword {
        word: "EVENTS",
        reserved: false,
    },
    MySqlKeyword {
        word: "EVERY",
        reserved: false,
    },
    MySqlKeyword {
        word: "EXCEPT",
        reserved: true,
    },
    MySqlKeyword {
        word: "EXCHANGE",
        reserved: false,
    },
    MySqlKeyword {
        word: "EXCLUDE",
        reserved: false,
    },
    MySqlKeyword {
        word: "EXECUTE",
        reserved: false,
    },
    MySqlKeyword {
        word: "EXISTS",
        reserved: true,
    },
    MySqlKeyword {
        word: "EXIT",
        reserved: true,
    },
    MySqlKeyword {
        word: "EXPANSION",
        reserved: false,
    },
    MySqlKeyword {
        word: "EXPIRE",
        reserved: false,
    },
    MySqlKeyword {
        word: "EXPLAIN",
        reserved: true,
    },
    MySqlKeyword {
        word: "EXPORT",
        reserved: false,
    },
    MySqlKeyword {
        word: "EXTENDED",
        reserved: false,
    },
    MySqlKeyword {
        word: "EXTENT_SIZE",
        reserved: false,
    },
    MySqlKeyword {
        word: "FACTOR",
        reserved: false,
    },
    MySqlKeyword {
        word: "FAILED_LOGIN_ATTEMPTS",
        reserved: false,
    },
    MySqlKeyword {
        word: "FALSE",
        reserved: true,
    },
    MySqlKeyword {
        word: "FAST",
        reserved: false,
    },
    MySqlKeyword {
        word: "FAULTS",
        reserved: false,
    },
    MySqlKeyword {
        word: "FETCH",
        reserved: true,
    },
    MySqlKeyword {
        word: "FIELDS",
        reserved: false,
    },
    MySqlKeyword {
        word: "FILE",
        reserved: false,
    },
    MySqlKeyword {
        word: "FILE_BLOCK_SIZE",
        reserved: false,
    },
    MySqlKeyword {
        word: "FILTER",
        reserved: false,
    },
    MySqlKeyword {
        word: "FINISH",
        reserved: false,
    },
    MySqlKeyword {
        word: "FIRST",
        reserved: false,
    },
    MySqlKeyword {
        word: "FIRST_VALUE",
        reserved: true,
    },
    MySqlKeyword {
        word: "FIXED",
        reserved: false,
    },
    MySqlKeyword {
        word: "FLOAT",
        reserved: true,
    },
    MySqlKeyword {
        word: "FLOAT4",
        reserved: true,
    },
    MySqlKeyword {
        word: "FLOAT8",
        reserved: true,
    },
    MySqlKeyword {
        word: "FLUSH",
        reserved: false,
    },
    MySqlKeyword {
        word: "FOLLOWING",
        reserved: false,
    },
    MySqlKeyword {
        word: "FOLLOWS",
        reserved: false,
    },
    MySqlKeyword {
        word: "FOR",
        reserved: true,
    },
    MySqlKeyword {
        word: "FORCE",
        reserved: true,
    },
    MySqlKeyword {
        word: "FOREIGN",
        reserved: true,
    },
    MySqlKeyword {
        word: "FORMAT",
        reserved: false,
    },
    MySqlKeyword {
        word: "FOUND",
        reserved: false,
    },
    MySqlKeyword {
        word: "FROM",
        reserved: true,
    },
    MySqlKeyword {
        word: "FULL",
        reserved: false,
    },
    MySqlKeyword {
        word: "FULLTEXT",
        reserved: true,
    },
    MySqlKeyword {
        word: "FUNCTION",
        reserved: true,
    },
    MySqlKeyword {
        word: "GENERAL",
        reserved: false,
    },
    MySqlKeyword {
        word: "GENERATE",
        reserved: false,
    },
    MySqlKeyword {
        word: "GENERATED",
        reserved: true,
    },
    MySqlKeyword {
        word: "GEOMCOLLECTION",
        reserved: false,
    },
    MySqlKeyword {
        word: "GEOMETRY",
        reserved: false,
    },
    MySqlKeyword {
        word: "GEOMETRYCOLLECTION",
        reserved: false,
    },
    MySqlKeyword {
        word: "GET",
        reserved: true,
    },
    MySqlKeyword {
        word: "GET_FORMAT",
        reserved: false,
    },
    MySqlKeyword {
        word: "GET_MASTER_PUBLIC_KEY",
        reserved: false,
    },
    MySqlKeyword {
        word: "GET_SOURCE_PUBLIC_KEY",
        reserved: false,
    },
    MySqlKeyword {
        word: "GLOBAL",
        reserved: false,
    },
    MySqlKeyword {
        word: "GRANT",
        reserved: true,
    },
    MySqlKeyword {
        word: "GRANTS",
        reserved: false,
    },
    MySqlKeyword {
        word: "GROUP",
        reserved: true,
    },
    MySqlKeyword {
        word: "GROUPING",
        reserved: true,
    },
    MySqlKeyword {
        word: "GROUPS",
        reserved: true,
    },
    MySqlKeyword {
        word: "GROUP_REPLICATION",
        reserved: false,
    },
    MySqlKeyword {
        word: "GTID_ONLY",
        reserved: false,
    },
    MySqlKeyword {
        word: "HANDLER",
        reserved: false,
    },
    MySqlKeyword {
        word: "HASH",
        reserved: false,
    },
    MySqlKeyword {
        word: "HAVING",
        reserved: true,
    },
    MySqlKeyword {
        word: "HELP",
        reserved: false,
    },
    MySqlKeyword {
        word: "HIGH_PRIORITY",
        reserved: true,
    },
    MySqlKeyword {
        word: "HISTOGRAM",
        reserved: false,
    },
    MySqlKeyword {
        word: "HISTORY",
        reserved: false,
    },
    MySqlKeyword {
        word: "HOST",
        reserved: false,
    },
    MySqlKeyword {
        word: "HOSTS",
        reserved: false,
    },
    MySqlKeyword {
        word: "HOUR",
        reserved: false,
    },
    MySqlKeyword {
        word: "HOUR_MICROSECOND",
        reserved: true,
    },
    MySqlKeyword {
        word: "HOUR_MINUTE",
        reserved: true,
    },
    MySqlKeyword {
        word: "HOUR_SECOND",
        reserved: true,
    },
    MySqlKeyword {
        word: "IDENTIFIED",
        reserved: false,
    },
    MySqlKeyword {
        word: "IF",
        reserved: true,
    },
    MySqlKeyword {
        word: "IGNORE",
        reserved: true,
    },
    MySqlKeyword {
        word: "IGNORE_SERVER_IDS",
        reserved: false,
    },
    MySqlKeyword {
        word: "IMPORT",
        reserved: false,
    },
    MySqlKeyword {
        word: "IN",
        reserved: true,
    },
    MySqlKeyword {
        word: "INACTIVE",
        reserved: false,
    },
    MySqlKeyword {
        word: "INDEX",
        reserved: true,
    },
    MySqlKeyword {
        word: "INDEXES",
        reserved: false,
    },
    MySqlKeyword {
        word: "INFILE",
        reserved: true,
    },
    MySqlKeyword {
        word: "INITIAL",
        reserved: false,
    },
    MySqlKeyword {
        word: "INITIAL_SIZE",
        reserved: false,
    },
    MySqlKeyword {
        word: "INITIATE",
        reserved: false,
    },
    MySqlKeyword {
        word: "INNER",
        reserved: true,
    },
    MySqlKeyword {
        word: "INOUT",
        reserved: true,
    },
    MySqlKeyword {
        word: "INSENSITIVE",
        reserved: true,
    },
    MySqlKeyword {
        word: "INSERT",
        reserved: true,
    },
    MySqlKeyword {
        word: "INSERT_METHOD",
        reserved: false,
    },
    MySqlKeyword {
        word: "INSTALL",
        reserved: false,
    },
    MySqlKeyword {
        word: "INSTANCE",
        reserved: false,
    },
    MySqlKeyword {
        word: "INT",
        reserved: true,
    },
    MySqlKeyword {
        word: "INT1",
        reserved: true,
    },
    MySqlKeyword {
        word: "INT2",
        reserved: true,
    },
    MySqlKeyword {
        word: "INT3",
        reserved: true,
    },
    MySqlKeyword {
        word: "INT4",
        reserved: true,
    },
    MySqlKeyword {
        word: "INT8",
        reserved: true,
    },
    MySqlKeyword {
        word: "INTEGER",
        reserved: true,
    },
    MySqlKeyword {
        word: "INTERSECT",
        reserved: true,
    },
    MySqlKeyword {
        word: "INTERVAL",
        reserved: true,
    },
    MySqlKeyword {
        word: "INTO",
        reserved: true,
    },
    MySqlKeyword {
        word: "INVISIBLE",
        reserved: false,
    },
    MySqlKeyword {
        word: "INVOKER",
        reserved: false,
    },
    MySqlKeyword {
        word: "IO",
        reserved: false,
    },
    MySqlKeyword {
        word: "IO_AFTER_GTIDS",
        reserved: true,
    },
    MySqlKeyword {
        word: "IO_BEFORE_GTIDS",
        reserved: true,
    },
    MySqlKeyword {
        word: "IO_THREAD",
        reserved: false,
    },
    MySqlKeyword {
        word: "IPC",
        reserved: false,
    },
    MySqlKeyword {
        word: "IS",
        reserved: true,
    },
    MySqlKeyword {
        word: "ISOLATION",
        reserved: false,
    },
    MySqlKeyword {
        word: "ISSUER",
        reserved: false,
    },
    MySqlKeyword {
        word: "ITERATE",
        reserved: true,
    },
    MySqlKeyword {
        word: "JOIN",
        reserved: true,
    },
    MySqlKeyword {
        word: "JSON",
        reserved: false,
    },
    MySqlKeyword {
        word: "JSON_TABLE",
        reserved: true,
    },
    MySqlKeyword {
        word: "JSON_VALUE",
        reserved: false,
    },
    MySqlKeyword {
        word: "KEY",
        reserved: true,
    },
    MySqlKeyword {
        word: "KEYRING",
        reserved: false,
    },
    MySqlKeyword {
        word: "KEYS",
        reserved: true,
    },
    MySqlKeyword {
        word: "KEY_BLOCK_SIZE",
        reserved: false,
    },
    MySqlKeyword {
        word: "KILL",
        reserved: true,
    },
    MySqlKeyword {
        word: "LAG",
        reserved: true,
    },
    MySqlKeyword {
        word: "LANGUAGE",
        reserved: false,
    },
    MySqlKeyword {
        word: "LAST",
        reserved: false,
    },
    MySqlKeyword {
        word: "LAST_VALUE",
        reserved: true,
    },
    MySqlKeyword {
        word: "LATERAL",
        reserved: true,
    },
    MySqlKeyword {
        word: "LEAD",
        reserved: true,
    },
    MySqlKeyword {
        word: "LEADING",
        reserved: true,
    },
    MySqlKeyword {
        word: "LEAVE",
        reserved: true,
    },
    MySqlKeyword {
        word: "LEAVES",
        reserved: false,
    },
    MySqlKeyword {
        word: "LEFT",
        reserved: true,
    },
    MySqlKeyword {
        word: "LESS",
        reserved: false,
    },
    MySqlKeyword {
        word: "LEVEL",
        reserved: false,
    },
    MySqlKeyword {
        word: "LIKE",
        reserved: true,
    },
    MySqlKeyword {
        word: "LIMIT",
        reserved: true,
    },
    MySqlKeyword {
        word: "LINEAR",
        reserved: true,
    },
    MySqlKeyword {
        word: "LINES",
        reserved: true,
    },
    MySqlKeyword {
        word: "LINESTRING",
        reserved: false,
    },
    MySqlKeyword {
        word: "LIST",
        reserved: false,
    },
    MySqlKeyword {
        word: "LOAD",
        reserved: true,
    },
    MySqlKeyword {
        word: "LOCAL",
        reserved: false,
    },
    MySqlKeyword {
        word: "LOCALTIME",
        reserved: true,
    },
    MySqlKeyword {
        word: "LOCALTIMESTAMP",
        reserved: true,
    },
    MySqlKeyword {
        word: "LOCK",
        reserved: true,
    },
    MySqlKeyword {
        word: "LOCKED",
        reserved: false,
    },
    MySqlKeyword {
        word: "LOCKS",
        reserved: false,
    },
    MySqlKeyword {
        word: "LOGFILE",
        reserved: false,
    },
    MySqlKeyword {
        word: "LOGS",
        reserved: false,
    },
    MySqlKeyword {
        word: "LONG",
        reserved: true,
    },
    MySqlKeyword {
        word: "LONGBLOB",
        reserved: true,
    },
    MySqlKeyword {
        word: "LONGTEXT",
        reserved: true,
    },
    MySqlKeyword {
        word: "LOOP",
        reserved: true,
    },
    MySqlKeyword {
        word: "LOW_PRIORITY",
        reserved: true,
    },
    MySqlKeyword {
        word: "MASTER",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_AUTO_POSITION",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_BIND",
        reserved: true,
    },
    MySqlKeyword {
        word: "MASTER_COMPRESSION_ALGORITHMS",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_CONNECT_RETRY",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_DELAY",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_HEARTBEAT_PERIOD",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_HOST",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_LOG_FILE",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_LOG_POS",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_PASSWORD",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_PORT",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_PUBLIC_KEY_PATH",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_RETRY_COUNT",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_SSL",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_SSL_CA",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_SSL_CAPATH",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_SSL_CERT",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_SSL_CIPHER",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_SSL_CRL",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_SSL_CRLPATH",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_SSL_KEY",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_SSL_VERIFY_SERVER_CERT",
        reserved: true,
    },
    MySqlKeyword {
        word: "MASTER_TLS_CIPHERSUITES",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_TLS_VERSION",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_USER",
        reserved: false,
    },
    MySqlKeyword {
        word: "MASTER_ZSTD_COMPRESSION_LEVEL",
        reserved: false,
    },
    MySqlKeyword {
        word: "MATCH",
        reserved: true,
    },
    MySqlKeyword {
        word: "MAXVALUE",
        reserved: true,
    },
    MySqlKeyword {
        word: "MAX_CONNECTIONS_PER_HOUR",
        reserved: false,
    },
    MySqlKeyword {
        word: "MAX_QUERIES_PER_HOUR",
        reserved: false,
    },
    MySqlKeyword {
        word: "MAX_ROWS",
        reserved: false,
    },
    MySqlKeyword {
        word: "MAX_SIZE",
        reserved: false,
    },
    MySqlKeyword {
        word: "MAX_UPDATES_PER_HOUR",
        reserved: false,
    },
    MySqlKeyword {
        word: "MAX_USER_CONNECTIONS",
        reserved: false,
    },
    MySqlKeyword {
        word: "MEDIUM",
        reserved: false,
    },
    MySqlKeyword {
        word: "MEDIUMBLOB",
        reserved: true,
    },
    MySqlKeyword {
        word: "MEDIUMINT",
        reserved: true,
    },
    MySqlKeyword {
        word: "MEDIUMTEXT",
        reserved: true,
    },
    MySqlKeyword {
        word: "MEMBER",
        reserved: false,
    },
    MySqlKeyword {
        word: "MEMORY",
        reserved: false,
    },
    MySqlKeyword {
        word: "MERGE",
        reserved: false,
    },
    MySqlKeyword {
        word: "MESSAGE_TEXT",
        reserved: false,
    },
    MySqlKeyword {
        word: "MICROSECOND",
        reserved: false,
    },
    MySqlKeyword {
        word: "MIDDLEINT",
        reserved: true,
    },
    MySqlKeyword {
        word: "MIGRATE",
        reserved: false,
    },
    MySqlKeyword {
        word: "MINUTE",
        reserved: false,
    },
    MySqlKeyword {
        word: "MINUTE_MICROSECOND",
        reserved: true,
    },
    MySqlKeyword {
        word: "MINUTE_SECOND",
        reserved: true,
    },
    MySqlKeyword {
        word: "MIN_ROWS",
        reserved: false,
    },
    MySqlKeyword {
        word: "MOD",
        reserved: true,
    },
    MySqlKeyword {
        word: "MODE",
        reserved: false,
    },
    MySqlKeyword {
        word: "MODIFIES",
        reserved: true,
    },
    MySqlKeyword {
        word: "MODIFY",
        reserved: false,
    },
    MySqlKeyword {
        word: "MONTH",
        reserved: false,
    },
    MySqlKeyword {
        word: "MULTILINESTRING",
        reserved: false,
    },
    MySqlKeyword {
        word: "MULTIPOINT",
        reserved: false,
    },
    MySqlKeyword {
        word: "MULTIPOLYGON",
        reserved: false,
    },
    MySqlKeyword {
        word: "MUTEX",
        reserved: false,
    },
    MySqlKeyword {
        word: "MYSQL_ERRNO",
        reserved: false,
    },
    MySqlKeyword {
        word: "NAME",
        reserved: false,
    },
    MySqlKeyword {
        word: "NAMES",
        reserved: false,
    },
    MySqlKeyword {
        word: "NATIONAL",
        reserved: false,
    },
    MySqlKeyword {
        word: "NATURAL",
        reserved: true,
    },
    MySqlKeyword {
        word: "NCHAR",
        reserved: false,
    },
    MySqlKeyword {
        word: "NDB",
        reserved: false,
    },
    MySqlKeyword {
        word: "NDBCLUSTER",
        reserved: false,
    },
    MySqlKeyword {
        word: "NESTED",
        reserved: false,
    },
    MySqlKeyword {
        word: "NETWORK_NAMESPACE",
        reserved: false,
    },
    MySqlKeyword {
        word: "NEVER",
        reserved: false,
    },
    MySqlKeyword {
        word: "NEW",
        reserved: false,
    },
    MySqlKeyword {
        word: "NEXT",
        reserved: false,
    },
    MySqlKeyword {
        word: "NO",
        reserved: false,
    },
    MySqlKeyword {
        word: "NODEGROUP",
        reserved: false,
    },
    MySqlKeyword {
        word: "NONE",
        reserved: false,
    },
    MySqlKeyword {
        word: "NOT",
        reserved: true,
    },
    MySqlKeyword {
        word: "NOWAIT",
        reserved: false,
    },
    MySqlKeyword {
        word: "NO_WAIT",
        reserved: false,
    },
    MySqlKeyword {
        word: "NO_WRITE_TO_BINLOG",
        reserved: true,
    },
    MySqlKeyword {
        word: "NTH_VALUE",
        reserved: true,
    },
    MySqlKeyword {
        word: "NTILE",
        reserved: true,
    },
    MySqlKeyword {
        word: "NULL",
        reserved: true,
    },
    MySqlKeyword {
        word: "NULLS",
        reserved: false,
    },
    MySqlKeyword {
        word: "NUMBER",
        reserved: false,
    },
    MySqlKeyword {
        word: "NUMERIC",
        reserved: true,
    },
    MySqlKeyword {
        word: "NVARCHAR",
        reserved: false,
    },
    MySqlKeyword {
        word: "OF",
        reserved: true,
    },
    MySqlKeyword {
        word: "OFF",
        reserved: false,
    },
    MySqlKeyword {
        word: "OFFSET",
        reserved: false,
    },
    MySqlKeyword {
        word: "OJ",
        reserved: false,
    },
    MySqlKeyword {
        word: "OLD",
        reserved: false,
    },
    MySqlKeyword {
        word: "ON",
        reserved: true,
    },
    MySqlKeyword {
        word: "ONE",
        reserved: false,
    },
    MySqlKeyword {
        word: "ONLY",
        reserved: false,
    },
    MySqlKeyword {
        word: "OPEN",
        reserved: false,
    },
    MySqlKeyword {
        word: "OPTIMIZE",
        reserved: true,
    },
    MySqlKeyword {
        word: "OPTIMIZER_COSTS",
        reserved: true,
    },
    MySqlKeyword {
        word: "OPTION",
        reserved: true,
    },
    MySqlKeyword {
        word: "OPTIONAL",
        reserved: false,
    },
    MySqlKeyword {
        word: "OPTIONALLY",
        reserved: true,
    },
    MySqlKeyword {
        word: "OPTIONS",
        reserved: false,
    },
    MySqlKeyword {
        word: "OR",
        reserved: true,
    },
    MySqlKeyword {
        word: "ORDER",
        reserved: true,
    },
    MySqlKeyword {
        word: "ORDINALITY",
        reserved: false,
    },
    MySqlKeyword {
        word: "ORGANIZATION",
        reserved: false,
    },
    MySqlKeyword {
        word: "OTHERS",
        reserved: false,
    },
    MySqlKeyword {
        word: "OUT",
        reserved: true,
    },
    MySqlKeyword {
        word: "OUTER",
        reserved: true,
    },
    MySqlKeyword {
        word: "OUTFILE",
        reserved: true,
    },
    MySqlKeyword {
        word: "OVER",
        reserved: true,
    },
    MySqlKeyword {
        word: "OWNER",
        reserved: false,
    },
    MySqlKeyword {
        word: "PACK_KEYS",
        reserved: false,
    },
    MySqlKeyword {
        word: "PAGE",
        reserved: false,
    },
    MySqlKeyword {
        word: "PARSER",
        reserved: false,
    },
    MySqlKeyword {
        word: "PARTIAL",
        reserved: false,
    },
    MySqlKeyword {
        word: "PARTITION",
        reserved: true,
    },
    MySqlKeyword {
        word: "PARTITIONING",
        reserved: false,
    },
    MySqlKeyword {
        word: "PARTITIONS",
        reserved: false,
    },
    MySqlKeyword {
        word: "PASSWORD",
        reserved: false,
    },
    MySqlKeyword {
        word: "PASSWORD_LOCK_TIME",
        reserved: false,
    },
    MySqlKeyword {
        word: "PATH",
        reserved: false,
    },
    MySqlKeyword {
        word: "PERCENT_RANK",
        reserved: true,
    },
    MySqlKeyword {
        word: "PERSIST",
        reserved: false,
    },
    MySqlKeyword {
        word: "PERSIST_ONLY",
        reserved: false,
    },
    MySqlKeyword {
        word: "PHASE",
        reserved: false,
    },
    MySqlKeyword {
        word: "PLUGIN",
        reserved: false,
    },
    MySqlKeyword {
        word: "PLUGINS",
        reserved: false,
    },
    MySqlKeyword {
        word: "PLUGIN_DIR",
        reserved: false,
    },
    MySqlKeyword {
        word: "POINT",
        reserved: false,
    },
    MySqlKeyword {
        word: "POLYGON",
        reserved: false,
    },
    MySqlKeyword {
        word: "PORT",
        reserved: false,
    },
    MySqlKeyword {
        word: "PRECEDES",
        reserved: false,
    },
    MySqlKeyword {
        word: "PRECEDING",
        reserved: false,
    },
    MySqlKeyword {
        word: "PRECISION",
        reserved: true,
    },
    MySqlKeyword {
        word: "PREPARE",
        reserved: false,
    },
    MySqlKeyword {
        word: "PRESERVE",
        reserved: false,
    },
    MySqlKeyword {
        word: "PREV",
        reserved: false,
    },
    MySqlKeyword {
        word: "PRIMARY",
        reserved: true,
    },
    MySqlKeyword {
        word: "PRIVILEGES",
        reserved: false,
    },
    MySqlKeyword {
        word: "PRIVILEGE_CHECKS_USER",
        reserved: false,
    },
    MySqlKeyword {
        word: "PROCEDURE",
        reserved: true,
    },
    MySqlKeyword {
        word: "PROCESS",
        reserved: false,
    },
    MySqlKeyword {
        word: "PROCESSLIST",
        reserved: false,
    },
    MySqlKeyword {
        word: "PROFILE",
        reserved: false,
    },
    MySqlKeyword {
        word: "PROFILES",
        reserved: false,
    },
    MySqlKeyword {
        word: "PROXY",
        reserved: false,
    },
    MySqlKeyword {
        word: "PURGE",
        reserved: true,
    },
    MySqlKeyword {
        word: "QUARTER",
        reserved: false,
    },
    MySqlKeyword {
        word: "QUERY",
        reserved: false,
    },
    MySqlKeyword {
        word: "QUICK",
        reserved: false,
    },
    MySqlKeyword {
        word: "RANDOM",
        reserved: false,
    },
    MySqlKeyword {
        word: "RANGE",
        reserved: true,
    },
    MySqlKeyword {
        word: "RANK",
        reserved: true,
    },
    MySqlKeyword {
        word: "READ",
        reserved: true,
    },
    MySqlKeyword {
        word: "READS",
        reserved: true,
    },
    MySqlKeyword {
        word: "READ_ONLY",
        reserved: false,
    },
    MySqlKeyword {
        word: "READ_WRITE",
        reserved: true,
    },
    MySqlKeyword {
        word: "REAL",
        reserved: true,
    },
    MySqlKeyword {
        word: "REBUILD",
        reserved: false,
    },
    MySqlKeyword {
        word: "RECOVER",
        reserved: false,
    },
    MySqlKeyword {
        word: "RECURSIVE",
        reserved: true,
    },
    MySqlKeyword {
        word: "REDO_BUFFER_SIZE",
        reserved: false,
    },
    MySqlKeyword {
        word: "REDUNDANT",
        reserved: false,
    },
    MySqlKeyword {
        word: "REFERENCE",
        reserved: false,
    },
    MySqlKeyword {
        word: "REFERENCES",
        reserved: true,
    },
    MySqlKeyword {
        word: "REGEXP",
        reserved: true,
    },
    MySqlKeyword {
        word: "REGISTRATION",
        reserved: false,
    },
    MySqlKeyword {
        word: "RELAY",
        reserved: false,
    },
    MySqlKeyword {
        word: "RELAYLOG",
        reserved: false,
    },
    MySqlKeyword {
        word: "RELAY_LOG_FILE",
        reserved: false,
    },
    MySqlKeyword {
        word: "RELAY_LOG_POS",
        reserved: false,
    },
    MySqlKeyword {
        word: "RELAY_THREAD",
        reserved: false,
    },
    MySqlKeyword {
        word: "RELEASE",
        reserved: true,
    },
    MySqlKeyword {
        word: "RELOAD",
        reserved: false,
    },
    MySqlKeyword {
        word: "REMOVE",
        reserved: false,
    },
    MySqlKeyword {
        word: "RENAME",
        reserved: true,
    },
    MySqlKeyword {
        word: "REORGANIZE",
        reserved: false,
    },
    MySqlKeyword {
        word: "REPAIR",
        reserved: false,
    },
    MySqlKeyword {
        word: "REPEAT",
        reserved: true,
    },
    MySqlKeyword {
        word: "REPEATABLE",
        reserved: false,
    },
    MySqlKeyword {
        word: "REPLACE",
        reserved: true,
    },
    MySqlKeyword {
        word: "REPLICA",
        reserved: false,
    },
    MySqlKeyword {
        word: "REPLICAS",
        reserved: false,
    },
    MySqlKeyword {
        word: "REPLICATE_DO_DB",
        reserved: false,
    },
    MySqlKeyword {
        word: "REPLICATE_DO_TABLE",
        reserved: false,
    },
    MySqlKeyword {
        word: "REPLICATE_IGNORE_DB",
        reserved: false,
    },
    MySqlKeyword {
        word: "REPLICATE_IGNORE_TABLE",
        reserved: false,
    },
    MySqlKeyword {
        word: "REPLICATE_REWRITE_DB",
        reserved: false,
    },
    MySqlKeyword {
        word: "REPLICATE_WILD_DO_TABLE",
        reserved: false,
    },
    MySqlKeyword {
        word: "REPLICATE_WILD_IGNORE_TABLE",
        reserved: false,
    },
    MySqlKeyword {
        word: "REPLICATION",
        reserved: false,
    },
    MySqlKeyword {
        word: "REQUIRE",
        reserved: true,
    },
    MySqlKeyword {
        word: "REQUIRE_ROW_FORMAT",
        reserved: false,
    },
    MySqlKeyword {
        word: "REQUIRE_TABLE_PRIMARY_KEY_CHECK",
        reserved: false,
    },
    MySqlKeyword {
        word: "RESET",
        reserved: false,
    },
    MySqlKeyword {
        word: "RESIGNAL",
        reserved: true,
    },
    MySqlKeyword {
        word: "RESOURCE",
        reserved: false,
    },
    MySqlKeyword {
        word: "RESPECT",
        reserved: false,
    },
    MySqlKeyword {
        word: "RESTART",
        reserved: false,
    },
    MySqlKeyword {
        word: "RESTORE",
        reserved: false,
    },
    MySqlKeyword {
        word: "RESTRICT",
        reserved: true,
    },
    MySqlKeyword {
        word: "RESUME",
        reserved: false,
    },
    MySqlKeyword {
        word: "RETAIN",
        reserved: false,
    },
    MySqlKeyword {
        word: "RETURN",
        reserved: true,
    },
    MySqlKeyword {
        word: "RETURNED_SQLSTATE",
        reserved: false,
    },
    MySqlKeyword {
        word: "RETURNING",
        reserved: false,
    },
    MySqlKeyword {
        word: "RETURNS",
        reserved: false,
    },
    MySqlKeyword {
        word: "REUSE",
        reserved: false,
    },
    MySqlKeyword {
        word: "REVERSE",
        reserved: false,
    },
    MySqlKeyword {
        word: "REVOKE",
        reserved: true,
    },
    MySqlKeyword {
        word: "RIGHT",
        reserved: true,
    },
    MySqlKeyword {
        word: "RLIKE",
        reserved: true,
    },
    MySqlKeyword {
        word: "ROLE",
        reserved: false,
    },
    MySqlKeyword {
        word: "ROLLBACK",
        reserved: false,
    },
    MySqlKeyword {
        word: "ROLLUP",
        reserved: false,
    },
    MySqlKeyword {
        word: "ROTATE",
        reserved: false,
    },
    MySqlKeyword {
        word: "ROUTINE",
        reserved: false,
    },
    MySqlKeyword {
        word: "ROW",
        reserved: true,
    },
    MySqlKeyword {
        word: "ROWS",
        reserved: true,
    },
    MySqlKeyword {
        word: "ROW_COUNT",
        reserved: false,
    },
    MySqlKeyword {
        word: "ROW_FORMAT",
        reserved: false,
    },
    MySqlKeyword {
        word: "ROW_NUMBER",
        reserved: true,
    },
    MySqlKeyword {
        word: "RTREE",
        reserved: false,
    },
    MySqlKeyword {
        word: "SAVEPOINT",
        reserved: false,
    },
    MySqlKeyword {
        word: "SCHEDULE",
        reserved: false,
    },
    MySqlKeyword {
        word: "SCHEMA",
        reserved: true,
    },
    MySqlKeyword {
        word: "SCHEMAS",
        reserved: true,
    },
    MySqlKeyword {
        word: "SCHEMA_NAME",
        reserved: false,
    },
    MySqlKeyword {
        word: "SECOND",
        reserved: false,
    },
    MySqlKeyword {
        word: "SECONDARY",
        reserved: false,
    },
    MySqlKeyword {
        word: "SECONDARY_ENGINE",
        reserved: false,
    },
    MySqlKeyword {
        word: "SECONDARY_ENGINE_ATTRIBUTE",
        reserved: false,
    },
    MySqlKeyword {
        word: "SECONDARY_LOAD",
        reserved: false,
    },
    MySqlKeyword {
        word: "SECONDARY_UNLOAD",
        reserved: false,
    },
    MySqlKeyword {
        word: "SECOND_MICROSECOND",
        reserved: true,
    },
    MySqlKeyword {
        word: "SECURITY",
        reserved: false,
    },
    MySqlKeyword {
        word: "SELECT",
        reserved: true,
    },
    MySqlKeyword {
        word: "SENSITIVE",
        reserved: true,
    },
    MySqlKeyword {
        word: "SEPARATOR",
        reserved: true,
    },
    MySqlKeyword {
        word: "SERIAL",
        reserved: false,
    },
    MySqlKeyword {
        word: "SERIALIZABLE",
        reserved: false,
    },
    MySqlKeyword {
        word: "SERVER",
        reserved: false,
    },
    MySqlKeyword {
        word: "SESSION",
        reserved: false,
    },
    MySqlKeyword {
        word: "SET",
        reserved: true,
    },
    MySqlKeyword {
        word: "SHARE",
        reserved: false,
    },
    MySqlKeyword {
        word: "SHOW",
        reserved: true,
    },
    MySqlKeyword {
        word: "SHUTDOWN",
        reserved: false,
    },
    MySqlKeyword {
        word: "SIGNAL",
        reserved: true,
    },
    MySqlKeyword {
        word: "SIGNED",
        reserved: false,
    },
    MySqlKeyword {
        word: "SIMPLE",
        reserved: false,
    },
    MySqlKeyword {
        word: "SKIP",
        reserved: false,
    },
    MySqlKeyword {
        word: "SLAVE",
        reserved: false,
    },
    MySqlKeyword {
        word: "SLOW",
        reserved: false,
    },
    MySqlKeyword {
        word: "SMALLINT",
        reserved: true,
    },
    MySqlKeyword {
        word: "SNAPSHOT",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOCKET",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOME",
        reserved: false,
    },
    MySqlKeyword {
        word: "SONAME",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOUNDS",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_AUTO_POSITION",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_BIND",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_COMPRESSION_ALGORITHMS",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_CONNECTION_AUTO_FAILOVER",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_CONNECT_RETRY",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_DELAY",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_HEARTBEAT_PERIOD",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_HOST",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_LOG_FILE",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_LOG_POS",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_PASSWORD",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_PORT",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_PUBLIC_KEY_PATH",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_RETRY_COUNT",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_SSL",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_SSL_CA",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_SSL_CAPATH",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_SSL_CERT",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_SSL_CIPHER",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_SSL_CRL",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_SSL_CRLPATH",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_SSL_KEY",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_SSL_VERIFY_SERVER_CERT",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_TLS_CIPHERSUITES",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_TLS_VERSION",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_USER",
        reserved: false,
    },
    MySqlKeyword {
        word: "SOURCE_ZSTD_COMPRESSION_LEVEL",
        reserved: false,
    },
    MySqlKeyword {
        word: "SPATIAL",
        reserved: true,
    },
    MySqlKeyword {
        word: "SPECIFIC",
        reserved: true,
    },
    MySqlKeyword {
        word: "SQL",
        reserved: true,
    },
    MySqlKeyword {
        word: "SQLEXCEPTION",
        reserved: true,
    },
    MySqlKeyword {
        word: "SQLSTATE",
        reserved: true,
    },
    MySqlKeyword {
        word: "SQLWARNING",
        reserved: true,
    },
    MySqlKeyword {
        word: "SQL_AFTER_GTIDS",
        reserved: false,
    },
    MySqlKeyword {
        word: "SQL_AFTER_MTS_GAPS",
        reserved: false,
    },
    MySqlKeyword {
        word: "SQL_BEFORE_GTIDS",
        reserved: false,
    },
    MySqlKeyword {
        word: "SQL_BIG_RESULT",
        reserved: true,
    },
    MySqlKeyword {
        word: "SQL_BUFFER_RESULT",
        reserved: false,
    },
    MySqlKeyword {
        word: "SQL_CALC_FOUND_ROWS",
        reserved: true,
    },
    MySqlKeyword {
        word: "SQL_NO_CACHE",
        reserved: false,
    },
    MySqlKeyword {
        word: "SQL_SMALL_RESULT",
        reserved: true,
    },
    MySqlKeyword {
        word: "SQL_THREAD",
        reserved: false,
    },
    MySqlKeyword {
        word: "SQL_TSI_DAY",
        reserved: false,
    },
    MySqlKeyword {
        word: "SQL_TSI_HOUR",
        reserved: false,
    },
    MySqlKeyword {
        word: "SQL_TSI_MINUTE",
        reserved: false,
    },
    MySqlKeyword {
        word: "SQL_TSI_MONTH",
        reserved: false,
    },
    MySqlKeyword {
        word: "SQL_TSI_QUARTER",
        reserved: false,
    },
    MySqlKeyword {
        word: "SQL_TSI_SECOND",
        reserved: false,
    },
    MySqlKeyword {
        word: "SQL_TSI_WEEK",
        reserved: false,
    },
    MySqlKeyword {
        word: "SQL_TSI_YEAR",
        reserved: false,
    },
    MySqlKeyword {
        word: "SRID",
        reserved: false,
    },
    MySqlKeyword {
        word: "SSL",
        reserved: true,
    },
    MySqlKeyword {
        word: "STACKED",
        reserved: false,
    },
    MySqlKeyword {
        word: "START",
        reserved: false,
    },
    MySqlKeyword {
        word: "STARTING",
        reserved: true,
    },
    MySqlKeyword {
        word: "STARTS",
        reserved: false,
    },
    MySqlKeyword {
        word: "STATS_AUTO_RECALC",
        reserved: false,
    },
    MySqlKeyword {
        word: "STATS_PERSISTENT",
        reserved: false,
    },
    MySqlKeyword {
        word: "STATS_SAMPLE_PAGES",
        reserved: false,
    },
    MySqlKeyword {
        word: "STATUS",
        reserved: false,
    },
    MySqlKeyword {
        word: "STOP",
        reserved: false,
    },
    MySqlKeyword {
        word: "STORAGE",
        reserved: false,
    },
    MySqlKeyword {
        word: "STORED",
        reserved: true,
    },
    MySqlKeyword {
        word: "STRAIGHT_JOIN",
        reserved: true,
    },
    MySqlKeyword {
        word: "STREAM",
        reserved: false,
    },
    MySqlKeyword {
        word: "STRING",
        reserved: false,
    },
    MySqlKeyword {
        word: "SUBCLASS_ORIGIN",
        reserved: false,
    },
    MySqlKeyword {
        word: "SUBJECT",
        reserved: false,
    },
    MySqlKeyword {
        word: "SUBPARTITION",
        reserved: false,
    },
    MySqlKeyword {
        word: "SUBPARTITIONS",
        reserved: false,
    },
    MySqlKeyword {
        word: "SUPER",
        reserved: false,
    },
    MySqlKeyword {
        word: "SUSPEND",
        reserved: false,
    },
    MySqlKeyword {
        word: "SWAPS",
        reserved: false,
    },
    MySqlKeyword {
        word: "SWITCHES",
        reserved: false,
    },
    MySqlKeyword {
        word: "SYSTEM",
        reserved: true,
    },
    MySqlKeyword {
        word: "TABLE",
        reserved: true,
    },
    MySqlKeyword {
        word: "TABLES",
        reserved: false,
    },
    MySqlKeyword {
        word: "TABLESPACE",
        reserved: false,
    },
    MySqlKeyword {
        word: "TABLE_CHECKSUM",
        reserved: false,
    },
    MySqlKeyword {
        word: "TABLE_NAME",
        reserved: false,
    },
    MySqlKeyword {
        word: "TEMPORARY",
        reserved: false,
    },
    MySqlKeyword {
        word: "TEMPTABLE",
        reserved: false,
    },
    MySqlKeyword {
        word: "TERMINATED",
        reserved: true,
    },
    MySqlKeyword {
        word: "TEXT",
        reserved: false,
    },
    MySqlKeyword {
        word: "THAN",
        reserved: false,
    },
    MySqlKeyword {
        word: "THEN",
        reserved: true,
    },
    MySqlKeyword {
        word: "THREAD_PRIORITY",
        reserved: false,
    },
    MySqlKeyword {
        word: "TIES",
        reserved: false,
    },
    MySqlKeyword {
        word: "TIME",
        reserved: false,
    },
    MySqlKeyword {
        word: "TIMESTAMP",
        reserved: false,
    },
    MySqlKeyword {
        word: "TIMESTAMPADD",
        reserved: false,
    },
    MySqlKeyword {
        word: "TIMESTAMPDIFF",
        reserved: false,
    },
    MySqlKeyword {
        word: "TINYBLOB",
        reserved: true,
    },
    MySqlKeyword {
        word: "TINYINT",
        reserved: true,
    },
    MySqlKeyword {
        word: "TINYTEXT",
        reserved: true,
    },
    MySqlKeyword {
        word: "TLS",
        reserved: false,
    },
    MySqlKeyword {
        word: "TO",
        reserved: true,
    },
    MySqlKeyword {
        word: "TRAILING",
        reserved: true,
    },
    MySqlKeyword {
        word: "TRANSACTION",
        reserved: false,
    },
    MySqlKeyword {
        word: "TRIGGER",
        reserved: true,
    },
    MySqlKeyword {
        word: "TRIGGERS",
        reserved: false,
    },
    MySqlKeyword {
        word: "TRUE",
        reserved: true,
    },
    MySqlKeyword {
        word: "TRUNCATE",
        reserved: false,
    },
    MySqlKeyword {
        word: "TYPE",
        reserved: false,
    },
    MySqlKeyword {
        word: "TYPES",
        reserved: false,
    },
    MySqlKeyword {
        word: "UNBOUNDED",
        reserved: false,
    },
    MySqlKeyword {
        word: "UNCOMMITTED",
        reserved: false,
    },
    MySqlKeyword {
        word: "UNDEFINED",
        reserved: false,
    },
    MySqlKeyword {
        word: "UNDO",
        reserved: true,
    },
    MySqlKeyword {
        word: "UNDOFILE",
        reserved: false,
    },
    MySqlKeyword {
        word: "UNDO_BUFFER_SIZE",
        reserved: false,
    },
    MySqlKeyword {
        word: "UNICODE",
        reserved: false,
    },
    MySqlKeyword {
        word: "UNINSTALL",
        reserved: false,
    },
    MySqlKeyword {
        word: "UNION",
        reserved: true,
    },
    MySqlKeyword {
        word: "UNIQUE",
        reserved: true,
    },
    MySqlKeyword {
        word: "UNKNOWN",
        reserved: false,
    },
    MySqlKeyword {
        word: "UNLOCK",
        reserved: true,
    },
    MySqlKeyword {
        word: "UNREGISTER",
        reserved: false,
    },
    MySqlKeyword {
        word: "UNSIGNED",
        reserved: true,
    },
    MySqlKeyword {
        word: "UNTIL",
        reserved: false,
    },
    MySqlKeyword {
        word: "UPDATE",
        reserved: true,
    },
    MySqlKeyword {
        word: "UPGRADE",
        reserved: false,
    },
    MySqlKeyword {
        word: "URL",
        reserved: false,
    },
    MySqlKeyword {
        word: "USAGE",
        reserved: true,
    },
    MySqlKeyword {
        word: "USE",
        reserved: true,
    },
    MySqlKeyword {
        word: "USER",
        reserved: false,
    },
    MySqlKeyword {
        word: "USER_RESOURCES",
        reserved: false,
    },
    MySqlKeyword {
        word: "USE_FRM",
        reserved: false,
    },
    MySqlKeyword {
        word: "USING",
        reserved: true,
    },
    MySqlKeyword {
        word: "UTC_DATE",
        reserved: true,
    },
    MySqlKeyword {
        word: "UTC_TIME",
        reserved: true,
    },
    MySqlKeyword {
        word: "UTC_TIMESTAMP",
        reserved: true,
    },
    MySqlKeyword {
        word: "VALIDATION",
        reserved: false,
    },
    MySqlKeyword {
        word: "VALUE",
        reserved: false,
    },
    MySqlKeyword {
        word: "VALUES",
        reserved: true,
    },
    MySqlKeyword {
        word: "VARBINARY",
        reserved: true,
    },
    MySqlKeyword {
        word: "VARCHAR",
        reserved: true,
    },
    MySqlKeyword {
        word: "VARCHARACTER",
        reserved: true,
    },
    MySqlKeyword {
        word: "VARIABLES",
        reserved: false,
    },
    MySqlKeyword {
        word: "VARYING",
        reserved: true,
    },
    MySqlKeyword {
        word: "VCPU",
        reserved: false,
    },
    MySqlKeyword {
        word: "VIEW",
        reserved: false,
    },
    MySqlKeyword {
        word: "VIRTUAL",
        reserved: true,
    },
    MySqlKeyword {
        word: "VISIBLE",
        reserved: false,
    },
    MySqlKeyword {
        word: "WAIT",
        reserved: false,
    },
    MySqlKeyword {
        word: "WARNINGS",
        reserved: false,
    },
    MySqlKeyword {
        word: "WEEK",
        reserved: false,
    },
    MySqlKeyword {
        word: "WEIGHT_STRING",
        reserved: false,
    },
    MySqlKeyword {
        word: "WHEN",
        reserved: true,
    },
    MySqlKeyword {
        word: "WHERE",
        reserved: true,
    },
    MySqlKeyword {
        word: "WHILE",
        reserved: true,
    },
    MySqlKeyword {
        word: "WINDOW",
        reserved: true,
    },
    MySqlKeyword {
        word: "WITH",
        reserved: true,
    },
    MySqlKeyword {
        word: "WITHOUT",
        reserved: false,
    },
    MySqlKeyword {
        word: "WORK",
        reserved: false,
    },
    MySqlKeyword {
        word: "WRAPPER",
        reserved: false,
    },
    MySqlKeyword {
        word: "WRITE",
        reserved: true,
    },
    MySqlKeyword {
        word: "X509",
        reserved: false,
    },
    MySqlKeyword {
        word: "XA",
        reserved: false,
    },
    MySqlKeyword {
        word: "XID",
        reserved: false,
    },
    MySqlKeyword {
        word: "XML",
        reserved: false,
    },
    MySqlKeyword {
        word: "XOR",
        reserved: true,
    },
    MySqlKeyword {
        word: "YEAR",
        reserved: false,
    },
    MySqlKeyword {
        word: "YEAR_MONTH",
        reserved: true,
    },
    MySqlKeyword {
        word: "ZEROFILL",
        reserved: true,
    },
    MySqlKeyword {
        word: "ZONE",
        reserved: false,
    },
];

/// Looks up an uppercase MySQL keyword in the versioned registry.
/// Whether an unquoted native keyword is admitted by role_ident_keyword.
/// This is grammar qualification only; it grants no account/source authority.
pub fn mysql_role_identifier_keyword(word: &str) -> bool {
    ![
        "EXECUTE",
        "RESTART",
        "SHUTDOWN",
        "EVENT",
        "FILE",
        "NONE",
        "PROCESS",
        "PROXY",
        "RELOAD",
        "REPLICATION",
        "RESOURCE",
        "SUPER",
    ]
    .iter()
    .any(|keyword| word.eq_ignore_ascii_case(keyword))
}

/// Looks up an exact canonical word in the sorted fixed MySQL 8.0.46 keyword table.
pub fn mysql_8_0_46_keyword(word: &str) -> Option<&'static MySqlKeyword> {
    MYSQL_8_0_46_KEYWORDS
        .binary_search_by_key(&word, |keyword| keyword.word)
        .ok()
        .map(|index| &MYSQL_8_0_46_KEYWORDS[index])
}
