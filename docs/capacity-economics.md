# Capacity and queue economics

CAPACITY1 adds a deterministic economics layer between canonical runner fitting and any future routing policy.

The input is deliberately split into two data classes:

```text
ProviderCatalog
  static/durable offer facts
  shape + execution model + pricing + billing granularity

CapacitySnapshot
  time-scoped runtime facts
  queue depth + running jobs + parallel slots
  slot turnover + utilization + cache state
  optional repository visibility context
```

They combine into:

```text
canonical RunnerShape
+ provider offer
+ runtime capacity snapshot
+ predicted warm runtime
-> effective runtime
-> queue wait
-> billed time / allocated fixed-capacity cost
-> effective cost
-> time-to-green
-> Pareto frontier
```

## Why the split matters

A GitHub Actions price or Hetzner machine shape is not the same kind of fact as today's queue depth.

Provider catalog records may live for days or weeks. Queue depth, concurrency pressure, cache state, and utilization may change in seconds.

CIShape therefore does not store queue behavior in the provider catalog and does not pretend that a checked-in queue value is live provider truth.

## Runtime snapshot

A snapshot contains one state per provider offer:

- `queue_depth` - jobs already waiting ahead of the new job
- `running_jobs` - slots currently occupied
- `parallel_slots` - effective concurrency available to this workload
- `slot_turnover_ms` - estimated time for one busy wave to free capacity
- `cache_state` - `warm` or `cold`
- `cache_penalty_ms` - explicit extra runtime for a cold cache
- `utilization` - productive historical slot utilization, required for fixed servers
- optional `scope` - the boundary covered by queue/running/concurrency evidence, such as repository, provider account, or runner pool

The checked-in `examples/capacity-snapshot-v1.json` is synthetic. It exists to make the model reproducible, not to claim current provider queue conditions.

CATALOG3 adds optional `repository_visibility` to the snapshot. Generic provider offers remain valid without it. Context-specific offers such as GitHub's public/private standard `ubuntu-latest` offers require a matching snapshot visibility and are skipped when the context is missing or mismatched. This prevents a valid queue observation from being combined with the wrong capacity/commercial contract.

## Capacity evidence scope

Queue depth, running jobs, and parallel slots must describe the same capacity domain.

A `CapacityState` can therefore carry:

```json
{
  "scope": {
    "kind": "provider_account",
    "key": "example-account"
  }
}
```

If a ProviderCatalog offer declares a required capacity-scope kind, economics checks it before doing queue or cost math. Missing or mismatched scope skips the offer with an explicit reason.

This prevents a repository-local active-job scan from being combined with an account-wide concurrency limit as if both described the same pool.

## Queue model

CIShape first consumes any currently free slots.

If queued jobs fill those slots, the new job waits for one or more slot-turnover waves:

```text
free_slots = parallel_slots - running_jobs

if queue_depth < free_slots:
  queue_waves = 0
else:
  queue_waves = floor((queue_depth - free_slots) / parallel_slots) + 1

queue_wait = queue_waves * slot_turnover
```

This is intentionally simple and explicit. A future provider adapter can supply a better observed turnover estimate without changing the economics contract.

## Cache model

The predicted duration passed to CAPACITY1 is the warm-cache baseline.

- warm cache -> no penalty
- cold cache -> add the supplied `cache_penalty_ms`

CIShape does not guess a cold-cache penalty from provider identity.

## Cost models

### Managed per-job billing

For managed runners, effective runtime is rounded to the provider billing increment:

```text
effective runtime -> billed seconds -> per-minute price
```

Queue wait is not billed to the waiting job.

### Persistent fixed capacity

For a persistent host, there is no fake per-job billing interval.

CAPACITY1 allocates the server hourly rate across productive slot capacity:

```text
productive slot-hour rate =
  host hourly price / (parallel slots * utilization)

effective job cost =
  productive slot-hour rate * effective runtime
```

This lets a persistent Hetzner host enter the same report as GitHub Actions or Depot without pretending their billing models are identical.

Monthly caps, taxes, IP charges, operator labor, and reserved-capacity contracts are not allocated yet.

## Cost versus time-to-green

CAPACITY1 does not invent one weighted score.

It emits both:

- effective cost
- time-to-green

and marks the Pareto frontier.

An offer is dominated only when another offer is no worse on both dimensions and strictly better on at least one.

CAPACITY2 adds that versioned policy explicitly. It can apply constraints such as:

```text
time_to_green <= 45s
then minimize effective_cost
```

or:

```text
effective_cost <= $0.003
then minimize time_to_green
```

without changing the underlying evidence model.

The default policy is `policies/economics-default-v1.json`:

```json
{
  "schema_version": 1,
  "policy_id": "economics-default-v1",
  "objective": "minimize_effective_cost",
  "max_time_to_green_ms": 30000,
  "max_effective_cost_usd": null
}
```

Selection remains advisory. It records which provider offer is eligible and selected under the supplied economics evidence and policy, but does not mutate the CI scheduler.

## CLI

Run the deterministic demo snapshot:

```bash
cargo run -- economics \
  --snapshot examples/capacity-snapshot-v1.json \
  --policy policies/economics-default-v1.json \
  --cpu 2 \
  --memory-gib 4 \
  --duration-ms 11000 \
  --format markdown
```

JSON is also available:

```bash
cargo run -- economics \
  --snapshot examples/capacity-snapshot-v1.json \
  --policy policies/economics-default-v1.json \
  --cpu 2 \
  --memory-gib 4 \
  --duration-ms 11000 \
  --format json
```

CAPACITY1 is read-only. It does not mutate CI workflows, schedule jobs, or call live provider APIs.


## Parallel jobs

CAPACITY3 extends this model with `cishape economics --jobs N`.

For `N > 1`, CIShape keeps the per-job economics above, simulates slot availability for the batch, calculates total effective cost and the time until the last new job completes, then applies EconomicsPolicy to those batch totals.

See [Parallel-job batch economics](batch-economics.md).
