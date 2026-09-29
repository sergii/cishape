# Worker runtime state

Status: WORKERSTATE1 contract  
Side effects: None

WORKERSTATE1 introduces host-level runtime evidence for concrete execution workers.

It is intentionally separate from both the static executor capability catalog and the existing provider-capacity economics snapshot.

## Three different layers

```text
ExecutorCatalog
  static capability envelope
  "what can this executor type support?"

WorkerStateSnapshot
  concrete host runtime state
  "what is allocated on this worker right now?"

CapacitySnapshot
  provider offer / queue economics state
  "what queue/capacity economics does this provider offer expose?"
```

These models may eventually be joined by a control plane, but they do not represent the same facts.

## Worker identity

Every worker has:

```text
worker_id
executor_id
```

`worker_id` identifies the concrete host or worker instance.

`executor_id` links that worker to the static capability envelope in `ExecutorCatalog`.

Several workers may share the same `executor_id`.

## Capacity model

WorkerState schema v2 records four capacity shapes plus a monotonic scheduling revision.

### physical_capacity

The actual physical CPU/RAM of the worker.

### allocation_limit

The explicit configured allocation envelope used by a future admission layer.

This may differ from physical capacity.

For example:

```text
physical CPU       32 cores
allocation limit   48 vCPU
```

is allowed.

That does **not** mean CIShape has decided that 1.5x CPU oversubscription is safe. It means the operator or a future versioned policy explicitly supplied that limit.

WORKERSTATE1 never invents an oversubscription ratio.

### allocated_capacity

The capacity already assigned to running allocations.

### reserved_capacity

The capacity leased by RESERVATION1 but not yet converted into running execution.

The snapshot fails closed if running plus reserved capacity exceeds the explicit allocation limit.

Available allocation capacity is derived deterministically:

```text
committed = allocated_capacity + reserved_capacity
available = allocation_limit - committed
```

### state_revision

Every worker has a positive monotonic `state_revision`.

Any admission-relevant change must advance it. This includes lifecycle, capacity, running allocation, reservation, and pressure evidence changes. Admission records the revision it evaluated so a later reservation can reject stale evidence.

## Allocation count

CPU and memory are not the only finite resource.

A worker also carries:

```text
running_allocations
reserved_allocations
max_allocations
```

The hard ceiling applies to committed count:

```text
running_allocations + reserved_allocations <= max_allocations
```

A worker may have free CPU and memory and still reject new work later because its allocation-count ceiling has been reached.

## Lifecycle

```text
ready
draining
offline
```

Only `ready` workers can report `accepting_new_work = true`.

A draining worker may continue to carry running allocations while refusing new ones.

An offline worker may remain in evidence so the control plane can explain why it was not considered.

## Pressure evidence

A snapshot may include normalized runtime samples:

```text
cpu_utilization_ratio
memory_utilization_ratio
io_pressure_ratio
```

Each value is in `[0, 1]`.

WORKERSTATE1 records these values only. It does not yet decide acceptable thresholds and does not use them to admit or reject a workload.

Those thresholds belong to ADMISSION1 policy.

## CLI

Inspect the checked-in example:

```bash
cishape worker-state \
  --snapshot examples/worker-state/workers-v1.json
```

The command emits a deterministic report containing both raw capacity evidence and derived fields:

```json
{
  "worker_id": "worker-a",
  "executor_id": "firecracker-like-microvm-x86_64",
  "state_revision": 7,
  "lifecycle": "ready",
  "physical_capacity": {
    "cpu_millis": 32000,
    "memory_bytes": 137438953472
  },
  "allocation_limit": {
    "cpu_millis": 48000,
    "memory_bytes": 128849018880
  },
  "allocated_capacity": {
    "cpu_millis": 20000,
    "memory_bytes": 42949672960
  },
  "reserved_capacity": {
    "cpu_millis": 0,
    "memory_bytes": 0
  },
  "committed_capacity": {
    "cpu_millis": 20000,
    "memory_bytes": 42949672960
  },
  "available_capacity": {
    "cpu_millis": 28000,
    "memory_bytes": 85899345920
  },
  "running_allocations": 6,
  "reserved_allocations": 0,
  "committed_allocations": 6,
  "max_allocations": 16,
  "accepting_new_work": true
}
```

## What accepting_new_work means

WORKERSTATE1 uses only structural runtime facts:

```text
lifecycle == ready
AND running_allocations + reserved_allocations < max_allocations
AND available CPU > 0
AND available memory > 0
```

It does **not** mean a specific job fits.

A worker with 1 free CPU can accept some work in principle while still being unable to accept a `cpu8-mem16` ExecutionPlan.

That plan-specific decision belongs to ADMISSION1.

## Why this is not scheduling

WORKERSTATE1 does not:

- select a worker;
- create reservations itself;
- mutate authoritative state from the inspection CLI;
- compare job demand with a worker;
- prioritize queues;
- provision a microVM;
- start a container;
- register a CI runner;
- infer a safe oversubscription factor.

It is an evidence contract only.

## Next slice

ADMISSION1 can combine:

```text
ExecutionPlan
  +
BindingReport
  +
WorkerStateSnapshot
  +
AdmissionPolicy
  |
  v
AdmissionReport
```

That layer can answer, without side effects:

- which compatible workers can safely accept the plan now;
- whether CPU/RAM/allocation-count limits permit it;
- whether pressure thresholds permit it;
- why a worker is excluded;
- whether the result is ADMIT, DEFER, or no eligible worker.

ADMISSION1 is implemented. RESERVATION1 adds the CAS lease transition that consumes this revisioned worker evidence; see `docs/reservation.md`.
