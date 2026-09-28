# CI strategy

CIShape treats CI time and compute as product concerns.

## Goals

- cancel stale work immediately
- fail fast on cheap deterministic checks
- do expensive native compilation only after quick gates pass
- reuse build artifacts aggressively
- keep one stable required `check` job
- avoid paying runner startup/toolchain setup twice
- keep CI reproducible enough to explain and dogfood with CIShape itself

## Pipeline

```text
new commit
   |
   +--> cancel older check job for the same PR/ref
   |
   v
check
   |
   +--> checkout
   +--> toolchain
   +--> rustfmt                 cheap / fail-fast
   +--> resolve dependency graph
   +--> restore Rust cache
   +--> cargo test              expensive
   +--> clippy --no-deps        reuses build
   +--> POC demo                reuses build
```

A single job is intentional. Step failure already prevents later expensive steps, while one runner avoids duplicate checkout/toolchain startup and keeps the restored build tree hot for tests, clippy, and the demo.

## Cancellation

The `check` job is scoped by PR number or ref:

```yaml
concurrency:
  group: ci-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: true
```

A new commit therefore supersedes the older expensive job for the same PR/ref.

Job-level concurrency is used instead of workflow-level concurrency so a newer workflow run does not wait behind a stale workflow merely to get permission to cancel it.

## DuckDB

CIShape currently uses bundled DuckDB and pins the direct crate version exactly because native DuckDB compilation dominates cold Rust CI time.

The Rust cache persists Cargo registry data and dependency build artifacts, including the expensive native dependency build outputs when reusable.

Changing the pinned DuckDB version intentionally invalidates the explicit CI cache key.

## Rust cache

CI uses `Swatinem/rust-cache`, pinned to a commit SHA.

The cache is shared under `cishape-ci` and is additionally keyed by the pinned DuckDB version. Cache writes are allowed on failed jobs so an expensive successful dependency compilation is not discarded merely because a later test fails.

The action also keys on the Rust environment and Cargo manifests.

## Build profiles

CI disables dev/test debug info because release-grade debug symbols are not useful for ordinary validation and increase native compile/link work and cache size.

## Dependency lock

CI currently generates a lockfile before the cached build and executes expensive Cargo commands with `--locked`.

Before the first public binary release, `Cargo.lock` must be committed to the repository so dependency resolution is reproducible outside CI as well.

## Ordering policy

Checks are ordered by expected cost:

1. syntax/format checks that do not compile dependencies
2. dependency/cache preparation
3. tests and compilation
4. lints that can reuse prior compilation
5. smoke/demo execution

As new checks are added, put the cheapest high-signal checks first and avoid parallelism when it would duplicate a large native build merely to save a few seconds of wall time.

## Future optimizations

Only add these when measurements justify them:

- committed `Cargo.lock`
- compiler-level `sccache`
- prebuilt/system DuckDB instead of bundled compilation
- path-aware skipping for documentation-only changes
- separate architecture/platform release builds
- CIShape telemetry around its own jobs once `observe` exists

Avoid adding optimization infrastructure whose maintenance cost exceeds the measured CI benefit.
