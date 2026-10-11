# sqlparser 0.62.0 local corrections

Upstream: https://github.com/apache/datafusion-sqlparser-rs
Crates.io package SHA-256: 13c6d1b651dc4edf07eead2a0c6c78016ce971bc2c10da5266861b13f25e7cec

This keeps the dependency version and public API unchanged. The root workspace already uses excluded third_party dependencies; this package uses that same mechanism. DataFusion's separate sqlparser 0.61.0 dependency remains from crates.io.

- CREATE/DROP function and aggregate arguments copy the identifier value, quote style and span from the tokenizer Word, instead of putting rendered SQL into Ident.value.
- Double-quoted identifiers serialize decoded quotes individually. Consecutive quotes and quotes following backslashes are data, not pre-escaped SQL.
- Parser and tokenizer instances accept one shared cooperative cancellation probe. Long single-token scanners and statement, list, expression, query, set-operation, and join loops check it at bounded intervals and return `ParserError::Cancelled` through result-bearing parser boundaries.
- PostgreSQL custom type modifiers accept a negative numeric constant, so catalog-qualified builtins such as `pg_catalog.numeric(10,-2)` retain the same modifier as unqualified `numeric(10,-2)`. Unary plus remains rejected, matching PostgreSQL 16, and malformed signed modifiers report the following token's source location.
- Single-quoted strings, backtick/bracket identifiers and other formatter branches are unchanged.

Project regression coverage is in `crates/postgres/src/semantics/tests/dialect.rs`, `crates/postgres/src/semantics/tests/parse_context.rs`, `crates/postgres/src/tests/sql_compiler.rs`, and `crates/postgres/src/tests/routine_execution.rs`. Fork-local parser coverage for signed PostgreSQL type modifiers is in `src/parser/tests/postgres_data_type.rs`. All files in the checksum-verified published crate, including licensing and embedded unit tests, are retained. The published archive does not include the upstream external integration suite. Remove each correction when an upstream release provides equivalent behavior; never restore the deleted gateway-level argument-name workaround.

The gateway and its SQL adapters use decoded tokenizer values (the default unescape mode). A caller manually supplying pre-escaped double-quoted Ident.value values is outside this patched formatter contract.

- PostgreSQL BETWEEN SYMMETRIC expands into the two ordinary BETWEEN trees used by native parser transformation, with OR (AND for NOT BETWEEN); ASYMMETRIC retains the ordinary tree. No AST type or public API is added. RESET TIME ZONE binds the existing TimeZone parameter. Source operand spans and quoted parameter identities remain intact. Regression coverage is mirrored in crates/postgres/src/semantics/tests/dialect.rs and real SQL period binding tests.
