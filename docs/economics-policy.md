# Economics selection policy

CAPACITY2 turns CAPACITY1 evidence into a deterministic provider-offer selection without inventing a weighted score.

The policy is separate from the canonical workload-shape policy:

```text
OptimizationPolicy
  JobShape -> canonical RunnerShape

EconomicsPolicy
  EconomicsReport -> provider offer
```

The repository ships `policies/economics-default-v1.json`:

```json
{
  "schema_version": 1,
  "policy_id": "economics-default-v1",
  "objective": "minimize_effective_cost",
  "max_time_to_green_ms": 30000,
  "max_effective_cost_usd": null
}
```

This means: discard offers whose estimated time-to-green is over 30 seconds, then choose the lowest effective cost.

For the synthetic CAPACITY1 snapshot this selects Depot. The persistent Hetzner example is cheaper at the supplied utilization, but its synthetic queue pushes time-to-green above the default SLA.

## Objectives

`minimize_effective_cost` orders by effective cost, then time-to-green, provider, and offer ID. An optional `max_time_to_green_ms` is a hard SLA.

`minimize_time_to_green` orders by time-to-green, then effective cost, provider, and offer ID. An optional `max_effective_cost_usd` is a hard budget.

Both constraints may be present for either objective.

## No weighted score

CAPACITY2 never hides preference in a formula such as `0.7 * cost + 0.3 * latency`. Cost and latency stay in their real units. Hard constraints are explicit, and one declared objective supplies deterministic ordering.

## Evidence

Selection records the policy ID/schema, objective, hard constraints, eligible count, selected provider/offer, selected cost/time, Pareto status, and exact exclusion reasons.

## CLI

```bash
cargo run -- economics \
  --snapshot examples/capacity-snapshot-v1.json \
  --policy policies/economics-default-v1.json \
  --cpu 2 \
  --memory-gib 4 \
  --duration-ms 11000 \
  --format markdown
```

The result remains advisory evidence. CIShape does not mutate runner labels or schedule jobs.
