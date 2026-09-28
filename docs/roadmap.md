# Roadmap

CIShape advances through narrow vertical slices.

## VS0 - Synthetic proof

Status: Done.

Goal:

```text
synthetic runs -> profile -> fit -> explain
```

Deliverables:

- Rust CLI
- DuckDB history
- JobShape
- RunnerShape
- deterministic optimizer
- `cishape demo`
- GitHub Actions CI

## VS1 - Real local observation

Status: Done.

Goal:

```text
cishape observe -- <command>
```

Collect:

- wall duration
- process CPU time
- process-tree CPU where available
- peak RSS
- read/write bytes
- exit status
- host/limit RunnerShape

Linux-first implementation is merged.

## VS2 - Dogfood GitHub Actions

Status: Done.

Run CIShape around its own:

- tests
- clippy
- build/demo

Persist normalized run summaries and local DuckDB history as run-scoped workflow artifacts first.

Do not require a SaaS backend.

## VS3 - Portable interchange

Status: Done for JSON/JSONL rolling history. OpenTelemetry and Parquet remain follow-ups.

Add import/export for normalized run history.

Priorities:

- JSON/JSONL
- merge history from multiple workflow runs
- OpenTelemetry mapping
- Parquet
- provider metadata adapters
- rolling GitHub Actions history from main-branch artifacts
- repository-safe historical report

## VS4 - Decision providers

Status: Deterministic provider is the primary path. Jev shadow/live adapters are implemented but deferred and non-blocking.

Keep deterministic feasibility/ranking.

JEV1:

- expose all deterministic feasible runner candidates
- prepare the official Jev typed-choice request without a network dependency
- reject choices outside the feasible set
- persist DecisionRecord separately from Recommendation/RunObservation
- compare Jev choice with the deterministic baseline in shadow mode

JEV2 adds the explicit live TypeSafe HTTP transport while preserving raw response evidence and the same fail-closed validator.

LIVE1 adds a manual GitHub Actions Jev proof that consumes rolling history. Running it is optional and can happen later.

## VS5 - Deterministic advisory

Status: Implemented.

Turn accumulated history into actionable, backend-free suggestions:

- evaluate every workload scope against the deterministic catalog
- classify insufficient_evidence/keep/downsize/upsize/reshape/no_candidate
- emit Markdown/JSON advisory reports
- require minimum evidence before actionable changes
- dogfood the report from rolling GitHub Actions history
- keep execution unchanged

## VS6 - Provider catalogs

Status: CATALOG1 implemented for dated offline snapshots and per-job offer fitting.

- provider runner aliases
- complete or explicitly incomplete CPU/RAM capacity
- OS/architecture/execution model
- dated price provenance
- billing increments
- fixed-server pricing kept separate from ephemeral per-job pricing

Next: refresh adapters/API-backed catalogs where providers expose reliable machine/price data.

## VS7 - Policy and outcomes

Status: POLICY1 and OUTCOME1 implemented.

POLICY1:

- versioned optimization policy
- explicit evidence threshold
- CPU/RAM safety factors
- latency penalty and optional p95 guard
- deterministic objective
- policy identity in recommendation/advisory evidence

OUTCOME1:

- DecisionRecord v2 carries baseline/selected RunnerShape and predicted latency
- post-decision observations are matched by repository/job/selected shape
- statuses distinguish not applied, insufficient evidence, within/outside prediction, and mixed failures
- Markdown/JSON outcome evidence remains local and deterministic
- no causal claim is made from temporal correlation alone

Next: surface economics-aware advisory results directly in CI.

## VS8 - Capacity economics

Status: CAPACITY1 and CAPACITY2 implemented.

CAPACITY1:

- keep runtime queue/capacity observations separate from static provider catalog facts
- model queue wait from queue depth, running jobs, parallel slots, and slot turnover
- model warm/cold cache state with an explicit cold-cache penalty
- preserve provider billing increments for managed runners
- allocate persistent fixed-server cost using explicit utilization and parallel-slot capacity
- emit effective runtime, queue wait, billed time, effective cost, and time-to-green
- mark Pareto-optimal offers without collapsing cost and latency into a hidden score
- remain local, deterministic, and read-only

CAPACITY2:

- version economics selection policy as checked data
- support minimize-effective-cost with an optional time-to-green SLA
- support minimize-time-to-green with an optional effective-cost budget
- emit explicit eligibility/exclusion evidence
- preserve deterministic tie-breaking with no hidden weighted score
- keep selection advisory-only with no routing side effects

Next: ingest real runtime capacity snapshots from provider adapters and surface economics-aware advisory results directly in CI.

## VS9 - Cloud ingest

Only after local dogfood proves useful:

- HTTP ingestion
- first normative OpenAPI
- authentication
- multi-repository history
- cloud UI

Cloudflare is a possible hosting platform, not part of the core product identity.

## VS10 - Advisory CI integration

Emit suggestions for existing CI systems without changing workflows automatically.

## VS11 - Controlled routing

Only after advisory recommendations prove reliable, allow opt-in automation through existing CI control planes.
