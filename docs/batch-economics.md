# Parallel-job batch economics

CAPACITY3 extends the single-job economics model to the way CI actually reaches green when several jobs are ready at once.

The single-job model remains the base evidence:

```text
provider offer
+ capacity snapshot
+ one predicted workload runtime
-> per-job cost
-> one-job time-to-green
```

For `--jobs N`, CIShape adds a deterministic slot scheduler:

```text
currently free slots
+ currently running jobs
+ jobs already queued ahead
+ slot turnover estimate
+ N new jobs
-> first new-job start
-> last new-job start
-> batch time-to-green
-> total effective cost
```

## Slot simulation

Each provider starts with `parallel_slots` availability timestamps.

- a free slot is available at `0`
- an occupied slot becomes available after `slot_turnover_ms`
- every already queued job is scheduled before the new batch
- queued jobs consume one turnover interval on the selected slot
- every new batch job consumes its CAPACITY1 `effective_runtime_ms`

At each step the job is assigned to the slot with the earliest availability. Slot index is the deterministic tie-breaker.

This is intentionally a small scheduling model, not a provider control plane.

## Cost

CAPACITY1 already computes a per-job effective cost using the correct billing model:

- managed runner billing increment
- or persistent fixed-capacity allocation by utilization and slot capacity

CAPACITY3 multiplies that allocated per-job cost by the number of new jobs.

That gives batch cost in the same evidence units used by EconomicsPolicy.

## Policy

For one job, policy constraints apply to one-job effective cost and time-to-green.

For `--jobs N` where `N > 1`, the same versioned policy applies to:

- total batch effective cost
- time until the last new job completes

This matters because a cheap two-slot persistent machine can be best for one job but lose a 30-second SLA when 30 jobs arrive together.

## CLI

Single-job behavior remains the default:

```bash
cargo run -- economics \
  --snapshot examples/capacity-snapshot-v1.json \
  --policy policies/economics-default-v1.json \
  --cpu 2 \
  --memory-gib 4 \
  --duration-ms 11000
```

Evaluate 30 parallel jobs:

```bash
cargo run -- economics \
  --snapshot examples/capacity-snapshot-v1.json \
  --policy policies/economics-default-v1.json \
  --cpu 2 \
  --memory-gib 4 \
  --duration-ms 11000 \
  --jobs 30
```

The output keeps the original per-job economics table, adds the batch schedule/economics table, and then applies deterministic selection to batch totals.

## Boundary

CAPACITY3 assumes identical jobs for this first slice.

It does not yet model:

- different job shapes within one workflow
- dependency DAGs
- matrix jobs with different runtimes
- cache warming from one job to another
- provider autoscaling lag
- preemption

Those belong in later workflow-level slices after this concurrency primitive is proven.
