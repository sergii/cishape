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

Status: In progress.

Add import/export for normalized run history.

Priorities:

- JSON/JSONL
- merge history from multiple workflow runs
- OpenTelemetry mapping
- Parquet
- provider metadata adapters

## VS4 - Jev decision experiment

Keep deterministic feasibility/ranking.

Give Jev only already-valid alternatives plus history, price, queue, latency objectives, and policy context.

Persist DecisionRecord separately from Recommendation.

## VS5 - Cloud ingest

Only after local dogfood proves useful:

- HTTP ingestion
- first normative OpenAPI
- authentication
- multi-repository history
- cloud UI

Cloudflare is a possible hosting platform, not part of the core product identity.

## VS6 - Advisory CI integration

Emit suggestions for existing CI systems without changing workflows automatically.

## VS7 - Controlled routing

Only after advisory recommendations prove reliable, allow opt-in automation through existing CI control planes.
