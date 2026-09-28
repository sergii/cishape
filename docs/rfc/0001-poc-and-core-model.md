# RFC-0001: Local-first CI workload shaping

Status: Accepted for POC0

## Problem

CI systems often bind jobs to provider-defined runner labels without enough evidence about actual CPU, memory, I/O, queue time, or cost.

This creates two failure modes:

- overprovisioning: short/light jobs consume unnecessarily large runners
- underprovisioning: critical jobs wait or run slowly on insufficient capacity

Provider-specific labels such as small, medium, large, and xlarge are unstable across vendors and over time.

## Assumption

A useful optimizer can be built from four stable concepts:

```text
RunObservation
JobShape
RunnerShape
Recommendation
```

A recommendation should be computed from historical evidence and explicit constraints before any optional AI reasoning is introduced.

## POC0

POC0 proves the loop using deterministic synthetic data:

```text
synthetic history
  -> DuckDB
  -> JobShape
  -> RunnerCatalog
  -> deterministic fit
  -> Recommendation
  -> explanation
```

Required command:

```bash
cishape demo
```

The demo must show an intentionally oversized CI job and produce a smaller feasible RunnerShape with an explicit cost/latency trade-off and safety margins.

## POC1

POC1 replaces synthetic observations with a real child process:

```bash
cishape observe -- npm test
```

The observer records real runtime evidence locally and feeds the same POC0 profiling and optimization pipeline.

## POC2

POC2 dogfoods CIShape in its own GitHub Actions workflow.

The project becomes its first real CI dataset.

## Non-goals

POC0 does not include:

- a SaaS service
- a control plane
- automatic workflow mutation
- provider purchasing or provisioning
- Jev
- OpenAPI
- account/authentication
- production-grade performance prediction

## Success criteria

POC0 succeeds when:

1. `cargo run -- demo` works deterministically.
2. history is queryable through DuckDB.
3. JobShape and RunnerShape are separate explicit types.
4. provider marketing size labels are not canonical domain types.
5. the optimizer produces a reproducible recommendation.
6. the explanation states the resource margins and model assumptions.
