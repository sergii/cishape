# Reservation and lease state machine

Status: RESERVATION1 contract  
Storage atomicity: adapter responsibility

RESERVATION1 is the first state-changing execution-plane contract.

It turns admission evidence into a compare-and-swap lease transition without starting a container or microVM.

```text
AdmissionReport
  +
current WorkerState
  +
ReservationRequest
  +
ReservationLedger
  +
ReservationPolicy
  |
  v
ReservationTransition
```

## Why reservation exists

Admission is snapshot-time advice.

Two schedulers can both observe:

```text
worker-a
state_revision = 7
available CPU  = 8
```

and both decide that a CPU8 workload fits.

Reservation adds a compare-and-swap boundary:

```text
request A expects revision 7
  -> reserve
  -> worker revision 8

request B expects revision 7
  -> stale_worker_state
```

Only the transition that atomically compares revision 7 and commits revision 8 may own the resources.

## Important storage boundary

The Rust functions in RESERVATION1 define and test the deterministic transition semantics.

The CLI produces offline transition evidence.

It does **not** claim that writing JSON files is a distributed atomic transaction.

A production control-plane adapter must commit, in one atomic transaction:

1. compare current worker state revision;
2. update reserved CPU/RAM and reserved allocation count;
3. advance worker state revision;
4. insert the lease/idempotency record.

If those actions are split across independent writes, the double-allocation protection is not valid.

## WorkerState v2

Reservation changes the meaning of worker scheduling state.

A worker now distinguishes:

```text
allocated_capacity    resources already running
reserved_capacity     resources leased but not started

running_allocations
reserved_allocations
max_allocations
```

Available allocation capacity is:

```text
allocation_limit
  - allocated_capacity
  - reserved_capacity
```

The count ceiling is:

```text
running_allocations
  + reserved_allocations
  <= max_allocations
```

### state_revision

Every worker carries a positive monotonic `state_revision`.

Any admission-relevant change must advance it, including changes to:

- lifecycle;
- physical/allocation capacity;
- running allocations;
- reservations;
- pressure evidence used by admission.

AdmissionReport v2 records the revision it evaluated.

Reservation requires the same revision to still be current.

## Reservation request

A request contains:

```json
{
  "schema_version": 1,
  "request_id": "req-example-001",
  "worker_id": "worker-a",
  "expected_worker_state_revision": 7,
  "lease_ttl_ms": 60000
}
```

The request does not contain CPU or RAM.

Those values come from the exact `ExecutionPlan` embedded in the AdmissionReport. This prevents a caller from changing the resource claim between admission and reservation.

`request_id` is also the deterministic lease ID in RESERVATION1.

## Reservation policy

The checked-in baseline:

```json
{
  "schema_version": 1,
  "policy_id": "reservation-default-v1",
  "min_lease_ttl_ms": 1000,
  "max_lease_ttl_ms": 900000
}
```

The one-second minimum and fifteen-minute maximum are explicit operational defaults, not empirically optimal values.

They can change only through versioned policy.

## Successful reserve transition

Given an admitted CPU4-MEM8 plan:

```text
before:
  revision              7
  reserved CPU          0
  reserved RAM          0
  reserved allocations  0

reserve:
  CPU4-MEM8

after:
  revision              8
  reserved CPU          4
  reserved RAM          8 GiB
  reserved allocations  1
```

The resulting active lease records:

- exact ExecutionPlan;
- worker/executor identity;
- reserved RunnerShape;
- creation and expiry timestamps;
- requested revision;
- revision before and after reservation;
- lease status.

## Idempotency

Repeating the exact same request ID with the same worker, revision, TTL, and plan returns:

```text
idempotent_replay
```

and does not reserve resources twice.

Reusing a request ID with different semantics fails closed.

Terminal leases remain in the ledger, so a released or expired request is not silently reused as a new reservation.

## Stale admission

If a different reservation already advanced the worker revision:

```text
admission revision  7
current revision    8
```

the result is:

```text
stale_worker_state
```

The caller must collect/recompose current state and run admission again.

## Release

Release moves an active lease to `released` and returns its reserved capacity/count.

It also advances worker revision once.

Release is idempotent at the lease-state level: trying to release an already terminal lease returns `already_terminal` without changing worker state.

## Expiry

`expire_due` finds active leases for one worker whose expiry is at or before the supplied deterministic clock value.

All due leases are expired in one transition:

- their reserved resources are returned;
- their status becomes `expired`;
- the worker revision advances once for the atomic batch.

Future leases remain active.

## CLI proof

Generate an AdmissionReport as described in `docs/admission.md`, then run:

```bash
cishape reservation-reserve \
  --admission .cishape/execution/test-admission.json \
  --workers examples/worker-state/workers-v1.json \
  --request examples/reservation/request-v1.json \
  --policy policies/reservation-default-v1.json \
  --ledger examples/reservation/empty-ledger-v1.json \
  --now-unix-ms 1790701200000
```

The output includes:

- worker state before;
- worker state after;
- lease;
- updated ledger;
- transition outcome.

The CLI does not overwrite authoritative state.

For release:

```bash
cishape reservation-release \
  --workers <updated-worker-snapshot.json> \
  --ledger <updated-ledger.json> \
  --lease-id req-example-001 \
  --now-unix-ms 1790701260000
```

For expiry:

```bash
cishape reservation-expire \
  --workers <updated-worker-snapshot.json> \
  --ledger <updated-ledger.json> \
  --worker-id worker-a \
  --now-unix-ms 1790701260000
```

## What this proves

RESERVATION1 proves the domain state machine for:

```text
ADMITTED
  -> RESERVED
  -> RELEASED

ADMITTED
  -> RESERVED
  -> EXPIRED
```

and proves that two independent requests based on one worker revision cannot both reserve successfully if the storage adapter honors the CAS transaction.

## What it does not prove

RESERVATION1 does not provide:

- a database-backed atomic transaction;
- distributed consensus;
- worker heartbeat;
- lease renewal;
- scheduler ranking;
- executor startup;
- reserved -> running transition;
- VM/container creation;
- GitHub runner registration.

## Next boundary

Before concurrent real execution, add an authoritative reservation store adapter that implements the CAS mutation atomically.

After that, the executor lifecycle can safely add:

```text
RESERVED
  -> STARTING
  -> RUNNING
  -> COLLECTING
  -> FINISHED
```

The first microVM proof can then bind the reserved plan to Boxd or native Firecracker without relying on advisory state alone.
