# RFC-0002: Cache integrity and isolated comparison execution

Status: Accepted design guidance for future execution/apply paths
Captured: 2026-09-30

## Context

CIShape's current core remains profiling, normalization, deterministic recommendations, and outcome analysis. CIShape is not required to become a compute provider, remote build system, or cache service.

This RFC applies only when CIShape benchmarks alternatives, applies a recommendation, drives a managed/BYOC executor, or consumes enough execution metadata to compare cache-sensitive runs.

The external signal is useful, but the product boundary matters:

- Namespace exposes cache-volume lineage, protected-branch cache updates, remote execution, and isolated compute.
- BoxD and Namespace both make isolated execution cheap enough that agents and CI systems can treat compute as replaceable.
- None of that means CIShape should own VM lifecycle in its core model.

## Decision

### Cache state is evidence

A run's cache condition materially affects runtime, CPU, I/O, and cost. CIShape must not silently compare a cold run with a warm run as if runner shape were the only changed variable.

When available, observations should preserve cache-relevant facts such as:

- cold / warm / unknown;
- cache key or logical lineage;
- hit / miss information;
- cache source or parent revision;
- whether the run was allowed to mutate a shared cache;
- whether a cache candidate was promoted.

The exact wire fields remain implementation-specific until a concrete provider integration requires them.

### Shared mutable cache is not a comparison primitive

Two executions that are being compared must not race through the same mutable workspace or mutable cache state.

Safe patterns include:

1. a common immutable/read-only parent with isolated writable descendants;
2. independent cache copies derived from the same logical parent;
3. sequential execution with an explicit restore/reset between runs.

A shared mutable cache may still exist for ordinary CI acceleration, but CIShape must treat it as a confounder unless its effects are explicitly modeled.

### Promotion is a separate decision

If CIShape ever controls cache publication, a run writes to a candidate lineage first. Promotion to a trusted/shared cache is a separate post-run action.

Minimum promotion requirements:

- the producing execution completed successfully;
- there was no infrastructure failure;
- the cache provenance is known;
- the producing source is allowed by trust policy;
- the cache identity is compatible with the consumer inputs/toolchain.

Success alone is not sufficient for an untrusted pull request to update a trusted shared cache.

This is a CIShape design rule. It should not be attributed to Namespace as a universal product guarantee. Namespace currently provides cache lineage and protected-branch cache updates; CIShape adopts the stronger explicit-promotion invariant for its own future apply paths.

### Isolation fallback

Preferred comparison order:

1. isolated parallel workers from equivalent inputs;
2. sequential runs on the same substrate with deterministic reset/restore;
3. fresh equivalent workers with recorded environment fingerprints.

If CIShape cannot establish an adequately comparable environment, it must not manufacture a confident optimization outcome. The run should remain advisory and surface an explicit non-comparability result such as insufficient_evidence.

For performance-sensitive conclusions, falling back to a different provider/shape without recording the change is not acceptable.

## Evidence to preserve

A future execution/outcome record may need fields equivalent to:

~~~text
cache_state
cache_lineage
cache_parent
cache_mutability
cache_promotion_decision

isolation_strategy
environment_fingerprint
fallback_reason
comparability_status
~~~

These are evidence concepts, not necessarily new top-level domain entities.

## Product boundary

This RFC does not add the following to CIShape core:

- VM or Devbox lifecycle;
- EnvironmentBlueprint / EnvironmentLease;
- a general cache service;
- a build graph engine;
- a scheduler;
- a remote execution protocol.

Provider adapters may expose these capabilities later. CIShape should consume the smallest normalized facts needed to make trustworthy recommendations.

## Relationship to RFC-0001

RFC-0001 remains authoritative for the current core model:

~~~text
RunObservation
JobShape
RunnerShape
Recommendation
~~~

This RFC only constrains how future execution-backed comparisons and recommendation outcomes preserve cache and isolation integrity.

## References

- Namespace changelog - Cache Volumes, protected branches, Devboxes, and remote execution: https://namespace.so/changelog
- Namespace Devbox: https://namespace.so/devbox
- BoxD documentation: https://docs.boxd.sh/llms-full.txt
