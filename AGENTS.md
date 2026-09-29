# CIShape agent guidance

CIShape is a vendor-neutral CI workload profiler and runner-shape optimizer.

## Product boundary

Keep these responsibilities separate:

1. Observe what a job actually consumed.
2. Normalize provider-specific metadata into CIShape domain types.
3. Build historical JobShape profiles.
4. Evaluate RunnerShape candidates deterministically.
5. Let optional decision providers such as Jev choose among already-valid alternatives.
6. Let CI control planes execute decisions.

CIShape is not a CI control plane and must not become one by accident.

CIShape may emit provider-neutral advisory execution plans for another control plane to consume, but planning must remain side-effect free.

## Core invariants

- Canonical runner capacity is numeric. Provider labels such as small, large, or xlarge are aliases only.
- Use RunnerShape external identities such as `cpu4-mem8` and display identities such as `CPU4-MEM8`, backed by explicit numeric fields.
- CPU and memory are independent dimensions. Never collapse them into a single scalar size.
- Raw telemetry and historical summaries are separate data classes.
- A recommendation must be reproducible from recorded evidence, policy, catalog, and algorithm version.
- Optimization thresholds must live in versioned policy data rather than hidden constants.
- Deterministic constraints run before optional AI or decision-provider reasoning.
- Jev is optional. The core optimizer must produce useful recommendations without it.
- Provider-specific integrations adapt to the core model. The core model must not depend on GitHub, Buildkite, Depot, Namespace, Blacksmith, or another provider.
- Provider prices and machine offers are dated observations with provenance, not eternal constants.
- A provider runner label is not sufficient offer identity. If the same label has different capacity or commercial semantics by repository/account context, model distinct offer IDs and require the relevant context explicitly.
- Context-specific offers must be checked against explicit runtime context evidence. If required repository/account context is missing or mismatched, skip the offer rather than inferring context from its label or ID.
- Never fabricate missing provider capacity, billing increments, or prices. Incomplete offers may remain visible but must not become selectable candidates.
- Do not compare fixed self-hosted server pricing to ephemeral per-job runner pricing without an explicit utilization/capacity model.
- Runtime queue/capacity snapshots are time-scoped observations and must remain separate from static provider catalog facts.
- Concurrency evidence must cover the same scope as the capacity limit it is paired with. If an offer requires provider-account scope, repository-scoped queue/running observations are insufficient and must fail closed.
- Provider-account evidence may be emitted only when the adapter can prove its inventory is complete for that account scope. Partial repository visibility must fail the whole snapshot rather than be treated as zero activity.
- Provider capacity adapters are read-only evidence collectors. They must normalize into CapacitySnapshot instead of leaking provider API types into core economics.
- If a provider endpoint does not reliably expose concurrency capacity, turnover, cache behavior, or another required field, require explicit evidence/configuration rather than inventing a default.
- Multi-pool capacity observation must not double-count an active job. If configured runner-label selectors overlap for an observed job, fail closed until the selectors are made unambiguous.
- Queue wait must come from explicit concurrency/turnover inputs; do not invent provider queue behavior.
- Cache effects must be explicit evidence. A warm cache has no penalty; a cold cache penalty must be supplied rather than guessed.
- Keep effective cost and time-to-green as separate dimensions unless a versioned policy explicitly combines or constrains them.
- Economics selection policy must be versioned data with explicit hard constraints and deterministic tie-breaking; do not hide provider preference in code.
- Economics-aware advisory must honor the historical minimum-evidence gate before provider selection and must reuse the canonical deterministic sizing path rather than inventing a second runner-sizing algorithm.
- CI dogfood must never promote illustrative capacity-plan values into live evidence. Economics-aware CI runs require an explicit real-evidence plan; otherwise skip the economics path visibly.
- Parallel-job time-to-green must be derived from explicit slot availability, queue-ahead work, and workload runtime. Do not approximate concurrency with an unexplained multiplier.
- Workflow DAG scheduling must respect both dependency completion and explicit slot availability, with deterministic tie-breaking and no hidden provider preference.
- Workflow-level economics evaluates one provider offer as a whole-workflow scenario unless a future versioned placement model explicitly says otherwise; do not silently turn advisory evaluation into cross-provider routing.
- Outcome evaluation is observational. Never claim a runner caused a failure or latency change from temporal correlation alone.
- OpenTelemetry CI/CD semantics should be reused where they fit instead of inventing equivalent vocabulary.
- Execution environment and resource tenancy are independent dimensions. Do not collapse process/container/microVM isolation and shared/dedicated allocation into one class label.
- Execution planning must not infer trust, isolation, cache, concurrency, or placement requirements from job names or provider labels. Missing evidence stays explicit.
- An ExecutionPlan is advisory evidence only. Provisioning, runner registration, scheduling, and workflow mutation remain control-plane responsibilities.
- Executor catalogs describe static/configuration-level capabilities only. Keep live worker availability, queue depth, free resources, cache warmth, and liveness in separate runtime evidence.
- Capability compatibility is not executor selection. If several executors fit, preserve the compatible set unless an explicit versioned selection/scheduling policy with the required evidence is present.
- Keep provider-offer queue/economics capacity separate from concrete worker/host runtime state. CapacitySnapshot and WorkerStateSnapshot answer different questions.
- Worker physical capacity, configured allocation limits, and current allocations are distinct evidence. Never infer an oversubscription ratio from physical CPU count.
- Worker pressure observations are evidence only until an explicit versioned admission policy defines thresholds and behavior.
- Binding evidence consumed by admission must be tied to the exact ExecutionPlan; reject stale/mismatched binding artifacts even when job/repository identity matches.
- Admission capacity basis, reserves, and pressure thresholds must come from versioned policy data. Do not silently treat allocation limits above physical capacity as usable oversubscription.
- An admission result is advisory snapshot-time eligibility, not a reservation. Never start execution from admission evidence without a later atomic reservation/lease step that prevents double allocation.
- Exclusive CPU admission must fail closed until worker evidence can prove reservable exclusive cores; free aggregate CPU millis are insufficient.
- WorkerState must distinguish running allocation from reserved-but-not-started allocation; availability and allocation-count ceilings include both.
- Worker state_revision is the CAS token for admission-relevant state. Any lifecycle/capacity/allocation/reservation/pressure change used by admission must advance it.
- Reservation resource amounts come from the exact admitted ExecutionPlan, never caller-supplied CPU/RAM fields.
- Reservation request IDs are idempotency keys. Exact replay must not double-reserve; conflicting reuse must fail closed.
- The reservation state machine is not a distributed transaction by itself. A production adapter must atomically compare revision, mutate reserved capacity/count, advance revision, and insert the lease.
- Lease release/expiry returns only reserved resources and must leave terminal lease evidence instead of deleting idempotency history.
- Keep analytical history storage separate from authoritative reservation/control-plane state. Do not reuse CI history tables as scheduler state.
- The first DuckDB ReservationStore is single-authority/single-writer-process only. Do not present a shared DuckDB file as a horizontally scaled control-plane database.
- ReservationStore reads must validate canonical domain JSON against indexed CAS/identity fields and fail closed on disagreement.
- Initial worker seeding is idempotent only for identical state. It must never overwrite a worker whose authoritative reservation state has changed.
- Claiming execution must atomically convert one active lease from reserved resources to running allocation, mark the lease claimed, advance worker revision, and insert the STARTING execution record.
- Claimed leases are terminal for reservation release/expiry. Running resources return only through execution completion to prevent double-free accounting.
- Validate backend capability before claiming a lease. Do not consume authoritative capacity and then discover the backend cannot represent the plan.
- Executor commands must preserve argv boundaries. Do not join untrusted workload arguments into a host shell command.
- Backend teardown is part of execution correctness. A cleanup failure must remain visible and cannot be reported as successful execution.
- The Boxd EXECUTOR1 adapter supports only the explicitly configured exact public proof shape; never infer arbitrary RunnerShape mappings from provider defaults.
- Do not automatically replay unknown STARTING/RUNNING executions after a control-plane crash until an explicit reconciliation protocol exists.
- The local-first path must remain useful without signup or a network service.

## Delivery sequence

POC0: synthetic history -> profile -> deterministic recommendation -> explanation.

POC1: real local process observation -> DuckDB history -> recommendation.

POC2: GitHub Actions dogfood -> first real CI history.

Only after the local model stabilizes should an HTTP/OpenAPI ingestion surface become normative.
