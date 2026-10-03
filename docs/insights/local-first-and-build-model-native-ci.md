# Local-first, build-model-native, and agent-native CI

Snapshot date: 2026-10-03.

Status: **insight / benchmark hypothesis**, not shipped CIShape behavior and not a commitment to become a CI control plane.

## Why preserve this

CI provider comparisons are usually reduced to runner CPU, RAM, startup latency, and price. That is too narrow for CIShape.

NixCI and Preloop expose two different architectural shifts that can materially change the developer feedback loop even when raw runner performance is similar:

- **build-model-native CI** - derive CI work from the build model instead of duplicating it in CI YAML;
- **local-first / agent-native CI** - run the same CI definition locally, preserve failed execution state, and shorten the failure-to-fix loop.

CIShape should be able to measure these differences without adopting either control-plane model itself.

## Market taxonomy

Treat CI systems as overlapping categories rather than one flat provider list.

### Hosted control plane + runners

Examples:

- GitHub Actions
- GitLab CI

The CI service owns orchestration and supplies or coordinates execution capacity.

### Accelerated runner infrastructure

Examples:

- Blacksmith
- Namespace

The existing CI control plane remains central while execution infrastructure aims to improve startup time, cache behavior, machine performance, or economics.

### Generic orchestration

Example:

- Buildkite

The orchestration layer is separable from the execution fleet.

### Build-model-native CI

Examples / research set:

- NixCI
- Garnix
- Hercules CI
- Hydra

The build model itself becomes a primary CI source of truth.

### Local-first CI compatibility

Examples / research set:

- Preloop
- act

The existing workflow definition is brought closer to the developer machine instead of forcing every verification round-trip through hosted CI.

### Agent-native verification

Preloop is a useful current example of an emerging category in which the verification loop is explicitly designed for software agents and high-frequency automated changes.

This category should be tracked separately from "faster runners".

## NixCI prior-art snapshot

NixCI positions itself as CI for repositories with `flake.nix`.

Observed product ideas worth preserving:

- CI work can be discovered from flake outputs with little or no separate CI configuration.
- The build definition remains usable locally rather than being encoded only in a provider workflow file.
- NixCI provides per-job commands intended to reproduce CI jobs locally.
- Automatic Nix binary caching is part of the product model.
- Hosted workers and bring-your-own workers are supported.
- Hosted workers are currently advertised as 16 vCPU / 64 GB RAM and billed per second of actual build time.
- GitHub, GitLab, Codeberg / Forgejo integrations are advertised.
- Impure tests with secrets/network access and continuous deployment are supported.
- Full self-hosting is commercially available, but NixCI itself is not open source.

Current public pricing observed on 2026-10-03:

- FOSS with own workers: free.
- Hosted FOSS compute: about USD 0.33 / worker-hour.
- Private hosted service: about USD 36 / developer / month plus USD 0.33 / worker-hour.
- Self-hosted leader/cache: commercial offering.

The architectural distinction matters more than the current price:

```text
traditional CI:
workflow YAML -> install/build/test instructions

NixCI:
flake.nix -> packages/checks/dev shells -> CI jobs
```

A useful shorthand is:

> Your build system can define CI.

### CIShape implication

NixCI should not be modeled only as another runner provider. It is a different execution/configuration model that may affect:

- reproducibility;
- cache reuse;
- duplicated CI configuration;
- local reproduction latency;
- failure investigation time;
- total time-to-green.

## Preloop prior-art snapshot

Preloop.dev positions itself as a local/self-hosted, Rust-native, GitHub Actions-compatible CI engine.

Observed product ideas worth preserving:

- Existing `.github/workflows/*.yml` are intended to run without a rewrite.
- The same engine can run locally or self-hosted.
- Jobs run in dedicated microVMs.
- The product advertises sub-second cold starts and under-200 ms VM boot examples, but these are vendor claims that CIShape should benchmark independently.
- A failed job can remain alive in its failure state.
- `preloop debug` can attach to that failed environment.
- A failed step can be retried without re-running the whole workflow.
- Changes made during debugging can be exported back as a patch.
- Machine-readable output is intended to let coding agents drive the same loop.
- `preloop run --sync` represents a **verify before publish** flow: run CI on the local workspace, then push the exact tested commit and report checks to GitHub.
- GitHub becomes a collaboration/status surface rather than the only event source required to start verification.

The core loop is:

```text
working tree
  -> run CI
  -> failure
  -> preserve failed machine
  -> inspect / patch
  -> retry failed step
  -> pass
  -> publish tested SHA
```

This is meaningfully different from the common hosted loop:

```text
push
  -> provision
  -> run
  -> fail
  -> inspect logs
  -> reproduce locally
  -> patch
  -> push again
```

A useful shorthand is:

> Your existing CI should run locally.

### Verify -> publish

The `run --sync` model suggests an important workflow inversion:

```text
VERIFY -> PUBLISH
```

instead of:

```text
PUBLISH -> VERIFY
```

CIShape should treat this as a measurable workflow property rather than assuming every CI system begins at push/webhook time.

### Preserved failure state

Preloop also challenges the assumption that "reproduction" must mean creating a second environment after a CI failure.

If the exact failed machine remains available, then:

```text
failure -> attach -> diagnose -> patch -> retry
```

may replace:

```text
failure -> reconstruct environment -> reproduce -> diagnose -> patch -> full rerun
```

That distinction should appear in CIShape metrics.

## NixCI vs Preloop

These systems should not be put in one undifferentiated provider ranking.

| Dimension | NixCI | Preloop |
| --- | --- | --- |
| Primary source of truth | `flake.nix` | GitHub Actions workflow YAML |
| Main architectural move | derive CI from build model | run existing CI locally |
| Reproducibility model | Nix/build-model reproducibility | same workflow engine + preserved execution environment |
| Existing GHA workflow | not the primary model | compatibility is a core claim |
| Local failure workflow | reproduce via provided command | keep/attach to failed microVM |
| Agent loop | useful but secondary | explicit product thesis |
| Publish order | conventional remote integration | can verify before push via `--sync` |

## CIShape benchmark direction

The benchmark should compare **feedback-loop performance**, not only runner speed.

Candidate systems:

```text
GitHub Actions
Blacksmith
Namespace
Buildkite
NixCI
Preloop
act
```

Not every benchmark has to include every system. The useful comparisons are category-aware, for example:

- GitHub Actions vs Blacksmith vs Namespace - execution infrastructure.
- GitHub Actions vs act vs Preloop - local workflow parity and debugging loop.
- GitHub Actions vs NixCI - workflow-defined CI vs build-model-native CI.
- NixCI vs Garnix vs Hercules CI vs Hydra - Nix-native ecosystem.

### Core execution metrics

Keep existing resource/economics observations:

- cold start;
- warm start;
- dependency restore;
- build duration;
- test duration;
- wall time;
- process CPU;
- peak memory;
- cache hit/miss evidence;
- billed time;
- effective cost.

### Feedback-loop metrics

Add or research explicit lifecycle metrics:

- **TTFV - Time To First Verification**: code/workspace change -> first verification result.
- **TTF - Time To Failure**: verification start -> actionable failure.
- **TTR - Time To Reproduce**: failure observed -> equivalent debuggable state available.
- **TTD - Time To Diagnose**: failure observed -> diagnosis/evidence sufficient to make a fix.
- **TTV - Time To Verify Fix**: fix prepared -> verification verdict for that fix.
- **TTG - Time To Green**: initial change or first failure -> accepted green result.

For preserved-environment systems, TTR may approach zero because the failed environment already exists. CIShape must allow that outcome rather than forcing a "reproduction job" into the model.

### Reproduction quality

Duration alone is insufficient. Future evidence may need to distinguish:

- approximate local reproduction;
- same workflow definition;
- same build model;
- same image/environment;
- same machine state after failure;
- exact tested commit/SHA;
- cache continuity across diagnosis/retry.

Do not collapse these into one opaque reproducibility score until a versioned model exists.

## Product boundary

**Direction:** CIShape should observe and compare these execution models.

**Not direction:** CIShape should become a GitHub Actions replacement, Nix build service, microVM orchestrator, or CI control plane.

The existing boundary remains:

```text
observe -> normalize -> profile -> compare -> advise
```

External CI/control-plane systems continue to execute the work.

Preloop-like preserved failure state and NixCI-like reproducible commands are therefore potential evidence sources and benchmark subjects, not reasons to merge orchestration into CIShape.

## Candidate follow-ups

1. Add a small `GHA vs act vs Preloop` benchmark fixture using the same repository/workflow.
2. Add a `GHA vs NixCI` experiment for a repository that can express the same checks through a Nix flake.
3. Define a versioned feedback-loop observation schema before adding derived TTR/TTD/TTV/TTG reporting.
4. Decide how to represent "failed environment preserved" without pretending it is ordinary cache state.
5. Decide whether exact tested SHA / verify-before-publish should be first-class evidence.
6. Research Garnix, Hercules CI, and Hydra using the same build-model-native taxonomy.
7. Keep vendor performance claims as provenance-bearing observations until independently measured.

## External references

- https://nix-ci.com/
- https://nix-ci.com/comparison
- https://nix-ci.com/pricing
- https://nix-ci.com/documentation/self-hosting
- https://preloop.dev/
