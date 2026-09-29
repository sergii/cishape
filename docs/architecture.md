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


### Executor capability binding

CIShape may compare an `ExecutionPlan` with a versioned static `ExecutorCatalog` and emit a side-effect-free `BindingReport`.

The catalog describes capability envelopes such as supported environment, maximum allocatable CPU/RAM, tenancy modes, architecture, capabilities, and cache modes. It must not contain live queue depth, current free resources, worker liveness, current cache warmth, or scheduling priority.

BINDING1 returns all compatible targets with explicit exclusion reasons. It does not select a winner when multiple targets fit. Selection requires separate live evidence and an explicit policy.

See `docs/execution-binding.md`.


### Worker runtime state

Concrete execution-worker state is modeled separately from both static executor capabilities and provider-offer queue economics.

`WorkerStateSnapshot` records concrete worker identity, executor identity, physical capacity, explicit allocation limits, current allocations, lifecycle, allocation-count ceilings, and optional pressure evidence.

`physical_capacity` and `allocation_limit` are intentionally distinct. A configured allocation limit may be above or below physical capacity, but CIShape must not infer or invent an oversubscription factor.

The existing `CapacitySnapshot` remains the provider-offer queue/economics model. It is not reused as host scheduler state.

WORKERSTATE1 derives free allocation capacity and whether a worker is structurally accepting any new work, but it does not decide whether a specific ExecutionPlan should be admitted.

See `docs/worker-state.md`.


### Worker admission

ADMISSION1 combines the exact `ExecutionPlan`, its `BindingReport`, a time-scoped `WorkerStateSnapshot`, and a versioned `AdmissionPolicy`.

The binding report embeds the plan that produced it, so downstream evaluation fails closed on stale or mismatched capability evidence.

Admission checks concrete runtime eligibility without choosing or mutating a worker. CPU and memory capacity basis, reserves, and pressure thresholds come from policy data rather than hidden code. The default policy uses physical capacity as the ceiling; configured allocation limits above physical capacity are used only when policy explicitly selects that basis.

`admit` means one or more workers are eligible at the snapshot instant. It is not a reservation guarantee. A future atomic reservation/lease boundary is required before execution.

See `docs/admission.md`.


### Reservation and leases

RESERVATION1 introduces the first state-changing execution-plane contract.

WorkerState v2 separates running allocation from reserved-but-not-started allocation and carries a monotonic `state_revision`. AdmissionReport v2 records the revision it evaluated.

A reservation request chooses one admission-approved worker and supplies an idempotency key, expected worker revision, and bounded lease TTL. CPU/RAM come only from the exact admitted ExecutionPlan.

The domain transition is compare-and-swap:

```text
expected revision N
  + admitted plan
  -> reserve resources
  -> revision N+1
  -> active lease
```

A second independent request from revision N becomes stale. Exact request replay is idempotent. Release/expiry returns reserved capacity and advances revision while retaining terminal lease evidence.

The Rust state machine and CLI are deterministic proofs, not a distributed storage implementation. A production adapter must commit the compare, worker mutation, revision advance, and lease insert atomically.

See `docs/reservation.md`.


### Authoritative reservation storage

RESERVATIONSTORE1 adds a `ReservationStore` boundary and a DuckDB implementation for one authoritative writer process.

The reservation database is separate from CIShape's analytical history database. It stores canonical WorkerState/ReservationLease JSON plus indexed identity, revision, status, and expiry fields used for CAS and lookup.

Reserve, release, and expire transitions are persisted in database transactions. Successful reservation updates the worker using `worker_id + expected state_revision`, inserts the lease, and commits as one unit.

The adapter validates indexed fields against canonical domain JSON on read and fails closed on disagreement.

This DuckDB adapter is not a multi-instance control-plane database. It is sufficient for a single-process executor proof. Horizontal control-plane deployment requires another `ReservationStore` implementation, expected to be Postgres-class storage with cross-client transactional tests.

See `docs/reservation-store.md`.

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
