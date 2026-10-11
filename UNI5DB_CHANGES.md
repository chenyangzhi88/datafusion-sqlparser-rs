# Uni5DB maintenance changes

## Provenance

- Fork: `git@github.com:chenyangzhi88/datafusion-sqlparser-rs.git`
- Upstream: `https://github.com/apache/datafusion-sqlparser-rs.git`
- Published version and official tag: `0.62.0`, `v0.62.0`
- Exact baseline: `3dd0e30d8bb1d2a6775f62d2b84839b60133effb`
- Published archive SHA-256:
  `13c6d1b651dc4edf07eead2a0c6c78016ce971bc2c10da5266861b13f25e7cec`
- Maintenance branch: `uni5db-0.62`; `main` is not a product-patch branch.
- Input: the complete Uni5DB working-tree snapshot, including uncommitted files,
  at parent HEAD `b88e963757b2eb583266eb17a310f397623c0d00`.

All 44 published source files match the corresponding baseline Git files.
All 64 existing product source files are preserved byte-for-byte: 24 modified
upstream files and 20 added files. The preservation digest (sorted path and
SHA-256 inventory) is
`a9c1507d83e71875c358ed4d96654bd62bd8a4e83e1d3de16b2b6e04131ca5a8`.
The migration does not introduce new parser behavior or upgrade dependencies.

## Existing product contracts

The accumulated patches include:

- Typed MySQL statement/DDL/DML identity, keyword and lexical handling, SQL-mode
  context, expressions, numeric/literal identity, optimizer hints and histograms.
- Typed PostgreSQL relation/column/routine syntax, ordered table elements,
  exclusion constraints, extended statistics, cursor and MERGE-related syntax.
- Shared cooperative parser/tokenizer cancellation and signed PostgreSQL type
  modifiers; identifier/token value and SQL formatting corrections.
- Existing parser, AST and tokenizer regression coverage. Moved upstream unit
  tests remain upstream code; use Git movement detection to inspect provenance.

AST, parser, tokenizer and tests depend on the same public types and contracts.
They are retained in one atomic functional commit, rather than presenting
unusable intermediate states as independent fixes. This is not a claim that
all contracts have passed the latest upstream external test suite. Individual
upstream contributions still need focused extraction and review.

## Manifest integration

The complete upstream manifest replaces Cargo's normalized package manifest.
It retains the product snapshot's `serde/rc` and dev `serde_json` requirements.
`sqlparser_derive 0.5.0` stays a registry dependency, as in the published package,
instead of adopting the full repository's `derive/` path dependency. The upstream
derive source itself is unchanged. The product dependency graph, production
features, package versions and root lockfile remain unchanged.

DataFusion's independent official `sqlparser 0.61.0` dependency is intentionally
not upgraded or redirected to this incompatible version.

## Migration validation and existing gaps

The source-preservation and 733-package dependency-graph checks passed.
The parser library suite with the product feature combination `serde,visitor`
ran 327 tests: 326 passed and one failed
(`analyze_modifiers_and_table_synonym_share_the_typed_histogram_action`, input
`OPTIMIZE LOCAL TABLE t`). The compatibility evidence utility passed 24 tests.

The required all-feature test was attempted, but existing numeric code and
tests treat `Value::Number` as a string when the BigDecimal feature changes its
representation; compilation failed. Format checking and all-target/all-feature
Clippy were also attempted and did not pass. Source bytes were verified unchanged
before and after checks. These failures are preserved and reported, not hidden
by deleting assertions, changing features or claiming upstream CI is green.
This source migration is not full application certification or an upstream-ready
PR. Behavior fixes and formatting would be separate work.

Commands (development checks, not a release):

```sh
cargo test --locked --offline --lib --features serde,visitor
cargo test --locked --offline --all-features
cargo fmt --all -- --check
cargo clippy --locked --offline --all-targets --all-features -- -D warnings
```

## Inspection and maintenance

```sh
git log --reverse --oneline 3dd0e30d8bb1d2a6775f62d2b84839b60133effb..HEAD
git diff --color-moved 3dd0e30d8bb1d2a6775f62d2b84839b60133effb...HEAD -- src Cargo.toml
git blame -C -C -- src/parser/mod.rs
```

The parent repository pins a specific Git commit. `branch = uni5db-0.62` is a
maintenance hint, not automatic upgrading. A new checkout normally has detached
HEAD and does not inherit local `upstream` remotes or branch aliases. Add/check
the upstream remote and use an explicit integration branch before merging a
selected compatible upstream release. Never rewrite published maintenance
history. Test product AST and dialect contracts before updating the parent pin.

For a contribution, branch from current `upstream/main` and extract one relevant
contract with its tests. Adapt to upstream APIs and test conventions; do not
submit the entire product-maintenance branch. New local maintenance commits are
not remotely available until an explicitly authorized push.
