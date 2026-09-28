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

## Core invariants

- Canonical runner capacity is numeric. Provider labels such as small, large, or xlarge are aliases only.
- Use RunnerShape external identities such as `cpu4-mem8` and display identities such as `CPU4-MEM8`, backed by explicit numeric fields.
- CPU and memory are independent dimensions. Never collapse them into a single scalar size.
- Raw telemetry and historical summaries are separate data classes.
- A recommendation must be reproducible from recorded evidence, policy, catalog, and algorithm version.
- Deterministic constraints run before optional AI or decision-provider reasoning.
- Jev is optional. The core optimizer must produce useful recommendations without it.
- Provider-specific integrations adapt to the core model. The core model must not depend on GitHub, Buildkite, Depot, Namespace, Blacksmith, or another provider.
- OpenTelemetry CI/CD semantics should be reused where they fit instead of inventing equivalent vocabulary.
- The local-first path must remain useful without signup or a network service.

## Delivery sequence

POC0: synthetic history -> profile -> deterministic recommendation -> explanation.

POC1: real local process observation -> DuckDB history -> recommendation.

POC2: GitHub Actions dogfood -> first real CI history.

Only after the local model stabilizes should an HTTP/OpenAPI ingestion surface become normative.
