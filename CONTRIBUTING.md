# Contributing to CIShape

CIShape is early-stage. Small, reviewable changes that preserve the architecture boundaries are preferred.

## Development

Requirements:

- Rust toolchain from `rust-toolchain.toml`
- a C/C++ toolchain for bundled DuckDB builds

Validate a change with:

```bash
cargo fmt --all -- --check
cargo test --all-features
cargo clippy --all-targets --all-features --no-deps -- -D warnings
cargo run --quiet -- demo
```

## Conventional Commits

Use Conventional Commits.

Preferred types:

- `feat`: user-visible capability
- `fix`: bug fix
- `perf`: performance improvement
- `refactor`: internal restructuring without intended behavior change
- `docs`: documentation only
- `test`: tests only
- `build`: build or release tooling
- `ci`: CI workflow/infrastructure
- `chore`: repository maintenance

Examples:

```text
feat(observe): collect Linux peak RSS
fix(profile): handle a single observation
perf(store): batch DuckDB inserts
docs(http): clarify ingestion boundary
ci: add supply-chain checks
```

For a breaking public contract change, use `!` and explain the migration in the commit body:

```text
feat(model)!: rename serialized runner shape fields
```

Security fixes use the normal semantic type with a security scope, for example:

```text
fix(security): reject unsafe artifact paths
```

## Changelog

The canonical changelog is derived from Git history with git-cliff.

Preview it with:

```bash
git cliff
```

Release automation may later add human-friendly release highlights, but AI-generated prose must never replace the deterministic canonical changelog.
