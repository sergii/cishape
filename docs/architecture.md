# Architecture

CIShape is a vendor-neutral profiler and optimizer for CI workloads.

## Core flow

```text
observe/import
    |
    v
RunObservation
    |
    v
history
    |
    v
JobShape
    |
    + RunnerCatalog
    |
    v
deterministic optimizer
    |
    v
Recommendation + feasible candidates
    |
    + optional DecisionProvider
    |      |
    |      v
    |   DecisionRecord
    |
    v
CI control plane executes elsewhere
```

CIShape does not execute CI jobs on behalf of GitHub Actions, Buildkite, GitLab, Jenkins, or another control plane.

## Boundaries

### Observation

Collects runtime evidence from the execution environment.

Future Linux observation may use process accounting, `/proc`, cgroup v2, PSI, and optional lower-level collectors.

Observation must report evidence, not recommendations.

### Normalization

Maps provider-specific names and metadata to stable CIShape concepts.

Provider labels such as `large` and `xlarge` are never canonical capacity identifiers.

### History

Stores normalized runs and derives historical distributions.

POC0 uses DuckDB locally.

Raw high-frequency telemetry may later be stored separately as Parquet while summaries remain queryable through the analytical layer.

### Profiling

Builds a `JobShape` from historical evidence.

A JobShape describes observed demand such as duration percentiles, CPU demand, memory demand, I/O, queue behavior, and reliability.

### Runner catalog

A `RunnerShape` describes offered capacity with explicit numeric dimensions.

Display identity:

```text
CPU8-MEM16
```

means 8 vCPU and 16 GiB RAM. The stable external identifier is `cpu8-mem16`; the uppercase form is presentation only.

CPU and memory remain independent dimensions.

### Optimization

The deterministic optimizer eliminates infeasible candidates and ranks feasible candidates against explicit objectives and constraints.

It must be reproducible and explainable without an LLM.

### Decision providers

Optional decision providers, beginning with a Jev shadow adapter, may choose among already-valid alternatives when policy trade-offs include price, queue state, latency objectives, provider reliability, or other uncertain context.

The deterministic optimizer defines the feasible set first. A provider response outside that set fails closed. DecisionRecord is persisted separately from runtime observations and the deterministic recommendation so decisions can later be evaluated against actual outcomes.

JEV1 has no execution side effects and no required network dependency.

### Execution planning

Execution still belongs to an external CI control plane.

CIShape may emit a provider-neutral advisory `ExecutionPlan` that combines the deterministic RunnerShape recommendation with explicit execution requirements. The plan has no provisioning, runner-registration, scheduling, workflow-mutation, or remote-execution side effects.

Execution environment and tenancy remain separate facets. For example, `microvm + exclusive CPU + shared host` is a valid combination; `dedicated` is not treated as another isolation environment beside process/container/microVM.

Provider/executor binding happens after the advisory plan and belongs to an external control plane. See `docs/execution-planning.md`.

## Telemetry interoperability

Prefer OpenTelemetry CI/CD semantic conventions for shared vocabulary where applicable.

CIShape-specific fields should cover only concepts the common standards do not express cleanly, for example:

- canonical RunnerShape
- provider runner alias
- billed cost
- billing interval
- isolation class
- optimizer algorithm/version
- recommendation evidence

Provider adapters translate into this model instead of extending the core with provider-specific semantics.
