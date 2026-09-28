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

The entire workflow is scoped by PR number or ref:

```yaml
concurrency:
  group: cishape-ci-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: true
```

A new commit therefore supersedes the entire older workflow for the same PR/ref, including an expensive compilation that is already running.

Because CIShape now uses one validation job, workflow-level cancellation is the simplest control-plane boundary and does not duplicate runner startup.

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

## Economics advisory dogfood

The existing `CI Advisory` workflow always produces the deterministic sizing advisory from rolling DuckDB history.

CAPACITY8 adds an optional live economics path:

```text
rolling history
+ explicit GitHubCapacityPlan
-> live CapacitySnapshot
-> economics-aware advisory
-> step summary + artifacts
```

No capacity defaults are embedded in the workflow. If no plan is configured, the economics section is skipped and the summary states that no synthetic capacity facts were substituted.

Manual runs can pass a repository-relative plan path through the `capacity_plan` workflow input. Automatic `workflow_run` dogfood activates only when this real-evidence file exists:

```text
config/cishape/github-capacity-plan.json
```

An explicitly requested missing plan fails closed. The capacity collection uses only `contents: read` and `actions: read` permissions plus the ephemeral `github.token`; it does not dispatch, cancel, or rewrite workflows.

The uploaded advisory artifact contains the deterministic advisory on every run and, when enabled:

- `capacity.json`
- `economics-advisory.md`
- `economics-advisory.json`

The checked-in example plan remains illustrative and is never used automatically as live evidence.

## Future optimizations

Only add these when measurements justify them:

- committed `Cargo.lock`
- compiler-level `sccache`
- prebuilt/system DuckDB instead of bundled compilation
- path-aware skipping for documentation-only changes
- separate architecture/platform release builds
- CIShape telemetry around its own jobs once `observe` exists

Avoid adding optimization infrastructure whose maintenance cost exceeds the measured CI benefit.
