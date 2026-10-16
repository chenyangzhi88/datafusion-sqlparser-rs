# sqlparser 0.62.0 local corrections

Upstream: https://github.com/apache/datafusion-sqlparser-rs
Crates.io package SHA-256: 13c6d1b651dc4edf07eead2a0c6c78016ce971bc2c10da5266861b13f25e7cec

This is Uni5DB's Git-submodule fork of sqlparser 0.62.0, maintained at `chenyangzhi88/datafusion-sqlparser-rs`. The version number is unchanged, but the fork has public AST extensions and is not API-identical to the published crate. The root workspace uses this path dependency; DataFusion's separate sqlparser 0.61.0 dependency remains from crates.io.

- CREATE/DROP function and aggregate arguments copy the identifier value, quote style and span from the tokenizer Word, instead of putting rendered SQL into Ident.value.
- Double-quoted identifiers serialize decoded quotes individually. Consecutive quotes and quotes following backslashes are data, not pre-escaped SQL.
- Parser and tokenizer instances accept one shared cooperative cancellation probe. Long single-token scanners and statement, list, expression, query, set-operation, and join loops check it at bounded intervals and return `ParserError::Cancelled` through result-bearing parser boundaries.
- PostgreSQL custom type modifiers accept a negative numeric constant, so catalog-qualified builtins such as `pg_catalog.numeric(10,-2)` retain the same modifier as unqualified `numeric(10,-2)`. Unary plus remains rejected, matching PostgreSQL 16, and malformed signed modifiers report the following token's source location.
- Single-quoted strings, backtick/bracket identifiers and other formatter branches are unchanged.

Downstream regression coverage belongs to the parent Uni5DB repository. Fork-local private parser tests live in `src/parser/tests/`, AST tests in `src/ast/tests/`, and tokenizer tests in `src/tests/tokenizer.rs`; public dialect and formatting integration tests live in `tests/`. Unlike the published archive, this Git fork includes the external integration suite. Retain upstream licensing, and remove each correction when an upstream release provides equivalent behavior; never restore the deleted gateway-level argument-name workaround.

The gateway and its SQL adapters use decoded tokenizer values (the default unescape mode). A caller manually supplying pre-escaped double-quoted Ident.value values is outside this patched formatter contract.

- PostgreSQL BETWEEN SYMMETRIC expands into the two ordinary BETWEEN trees used by native parser transformation, with OR (AND for NOT BETWEEN); ASYMMETRIC retains the ordinary tree. No AST type or public API is added. RESET TIME ZONE binds the existing TimeZone parameter. Source operand spans and quoted parameter identities remain intact. Regression coverage is mirrored in crates/postgres/src/semantics/tests/dialect.rs and real SQL period binding tests.

## Merge-resolution contracts

- MySQL admitted Source, CPP, native program-body and allocation ownership remain distinct from portable AST data. Reattachment restores readable bytes and budget ownership, not lexer or Alias issuance authority; explicit declaration reattachment keeps its separate contract.
- PostgreSQL's dedicated SQL/JSON, partition, routine and DDL ASTs and strict syntax are retained. Tests that intentionally exercise different dialect grammars assert their individual structures and valid/invalid boundaries. The shared exact-AST comparator is not weakened.
- Parenthesized CREATE TABLE LIKE clauses live in ordered table elements. A table COMMENT has one semantic owner rather than a second mirrored table option.
- OUTPUT INTO remains a table destination, including SQL Server table variables; it does not inherit MySQL SELECT INTO variable/file semantics. DELETE inheritance formatting retains pretty-print indentation, and an empty JSON_OBJECT RETURNING clause has no leading blank.

Parser-package validation uses `cargo test --all-features`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets --all-features -- -D warnings`. Passing these checks does not certify the parent server's SQL/protocol compatibility or replace its downstream integration tests. Existing ignored tests remain explicitly unexecuted.
