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
- The local-first path must remain useful without signup or a network service.

## Delivery sequence

POC0: synthetic history -> profile -> deterministic recommendation -> explanation.

POC1: real local process observation -> DuckDB history -> recommendation.

POC2: GitHub Actions dogfood -> first real CI history.

Only after the local model stabilizes should an HTTP/OpenAPI ingestion surface become normative.
