# CI strategy

CIShape treats CI time and compute as product concerns.

## Goals

- cancel stale work immediately
- fail fast on cheap deterministic checks
- do expensive native compilation only after quick gates pass
- reuse build artifacts aggressively
- preserve a stable required `check` job name
- keep CI reproducible enough to explain and dogfood with CIShape itself

## Pipeline

```text
new commit
   |
   +--> cancel older run for the same PR/ref
   |
   v
quick
   |
   +--> checkout
   +--> rust toolchain
   +--> rustfmt
   |
   v
check
   |
   +--> resolve dependency graph
   +--> restore Rust/dependency build cache
   +--> cargo test
   +--> clippy --no-deps
   +--> POC0 demo
```

The expensive `check` job never starts when the quick gate fails.

## Cancellation

The `quick` and `check` jobs share the same PR/ref concurrency group and use:

```yaml
concurrency:
  group: ci-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: true
```

A new commit can start its quick gate immediately. Because that quick job claims the same concurrency group as the previous expensive `check` job, it cancels stale expensive work before running. When the quick gate finishes, the new `check` job takes the same group.

This job-level model avoids a stale workflow occupying the whole workflow-level concurrency slot while a newer commit waits behind it.

## DuckDB

CIShape currently uses bundled DuckDB and pins the direct crate version exactly because native DuckDB compilation dominates cold Rust CI time.

The Rust cache persists Cargo registry data and dependency build artifacts, including the expensive native dependency build outputs when reusable.

Changing the pinned DuckDB version intentionally invalidates the CI cache key.

## Rust cache

CI uses `Swatinem/rust-cache`, pinned to a commit SHA.

The cache is shared for the CI build job and is additionally keyed by the pinned DuckDB version. Cache writes are allowed on failed jobs so an expensive successful dependency compilation is not discarded merely because a later test fails.

The action also keys on the Rust environment and Cargo manifests.

## Build profiles

CI disables dev/test debug info because release-grade debug symbols are not useful for ordinary validation and increase native compile/link work and cache size.

## Dependency lock

CI currently generates a lockfile before the cached build and executes expensive Cargo commands with `--locked`.

Before the first public binary release, `Cargo.lock` must be committed to the repository so dependency resolution is reproducible outside CI as well.

## Future optimizations

Only add these when measurements justify them:

- committed `Cargo.lock`
- compiler-level `sccache`
- prebuilt/system DuckDB instead of bundled compilation
- path-aware skipping for documentation-only changes
- separate architecture/platform release builds
- CIShape telemetry around its own jobs once `observe` exists

Avoid adding optimization infrastructure whose maintenance cost exceeds the measured CI benefit.
