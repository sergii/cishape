# Authoritative reservation store

Status: RESERVATIONSTORE1  
Adapter: DuckDB, single authoritative writer process

RESERVATIONSTORE1 turns the pure RESERVATION1 state machine into an actual database transaction boundary.

```text
AdmissionReport
  + ReservationRequest
  + ReservationPolicy
  |
  v
ReservationStore
  |
  +-- load authoritative WorkerState
  +-- load lease/idempotency history
  +-- evaluate domain transition
  +-- CAS worker revision
  +-- persist lease
  +-- COMMIT
```

## Why a separate reservation database

The default file is:

```text
.cishape/reservations.duckdb
```

It is intentionally separate from:

```text
.cishape/cishape.duckdb
```

The existing CIShape database is analytical history used for profiling, recommendations, and outcome evidence.

The reservation database is mutable control-plane state.

Keeping them separate prevents analytical history concerns from silently becoming scheduler-state concerns.

## Storage boundary

The domain-facing trait is `ReservationStore`.

The first implementation is `DuckDbReservationStore`.

The adapter stores two authoritative record classes:

### reservation_workers

Indexed fields:

```text
worker_id
executor_id
state_revision
```

plus the complete canonical `WorkerState` JSON.

### reservation_leases

Indexed fields:

```text
lease_id
request_id
worker_id
status
expires_at_unix_ms
```

plus the complete canonical `ReservationLease` JSON.

On read, the adapter parses and validates the JSON and checks that indexed identity/revision/status fields agree with the domain record.

A disagreement fails closed instead of choosing one representation as silently authoritative.

## Seeding workers

Initial worker evidence can be inserted with:

```bash
cishape reservation-store-seed \
  --db .cishape/reservations.duckdb \
  --workers examples/worker-state/workers-v1.json
```

Seeding is intentionally conservative.

If the worker does not exist, it is inserted.

If the exact same worker state already exists, seeding is idempotent.

If a worker already exists with different authoritative state, the command fails.

The seed path is not a worker-heartbeat or synchronization protocol and must not overwrite reservation state.

## Atomic reserve transaction

```bash
cishape reservation-store-reserve \
  --db .cishape/reservations.duckdb \
  --admission .cishape/execution/test-admission.json \
  --request examples/reservation/request-v1.json \
  --policy policies/reservation-default-v1.json \
  --now-unix-ms 1790701200000
```

Inside one database transaction the adapter:

1. loads the current authoritative worker;
2. loads lease/idempotency history;
3. runs the RESERVATION1 domain transition;
4. if the outcome is `reserved`, performs:
   ```sql
   UPDATE reservation_workers
   ...
   WHERE worker_id = ? AND state_revision = ?
   ```
5. requires exactly one worker row to change;
6. inserts the active lease;
7. commits.

If any DB operation fails before commit, the transaction is not committed.

The CPU/RAM claim still comes only from the exact admitted `ExecutionPlan`.

## Stale state

A request based on revision 7 cannot reserve a worker already at revision 8.

At the domain layer this returns:

```text
stale_worker_state
```

At the persistence layer the worker update also carries the expected revision in its `WHERE` clause.

The database CAS is therefore preserved even though the domain transition already checked the revision.

## Idempotency

`request_id` remains the deterministic lease identity.

The database enforces uniqueness for both:

```text
lease_id
request_id
```

Exact request replay returns the existing lease without changing worker state.

Conflicting reuse fails closed.

## Release and expiry

Release:

```bash
cishape reservation-store-release \
  --db .cishape/reservations.duckdb \
  --lease-id req-example-001 \
  --now-unix-ms 1790701260000
```

Expiry:

```bash
cishape reservation-store-expire \
  --db .cishape/reservations.duckdb \
  --worker-id worker-a \
  --now-unix-ms 1790701260000
```

Both operations update worker reservation accounting and terminal lease state in one transaction.

Terminal lease records remain stored for idempotency and audit evidence.

## DuckDB concurrency boundary

DuckDB is already pinned in CIShape and provides ACID transactions.

The RESERVATIONSTORE1 adapter is deliberately scoped to **one authoritative writer process**.

This matters because DuckDB's normal stable concurrency model is centered on writes within one process. CIShape must not interpret a shared DuckDB file as a general horizontally scaled control-plane database.

The first executor proof can therefore be:

```text
one control-plane process
  + one ReservationStore
  + one or more workers
```

That is enough to prove the execution lifecycle without introducing another database prematurely.

For multiple independent control-plane instances, add a Postgres-backed `ReservationStore` implementation and test CAS behavior across independent clients.

The domain reservation state machine does not need to change.

## What is authoritative

After a worker is seeded, the reservation database is authoritative for:

- worker scheduling revision;
- reserved CPU/RAM;
- reserved allocation count;
- reservation leases and terminal states.

A stale external WorkerStateSnapshot must not overwrite this state.

Future worker-agent updates need their own revisioned compare-and-update path.

## Non-goals

RESERVATIONSTORE1 does not add:

- multi-process DuckDB writer claims;
- Postgres;
- worker heartbeat;
- scheduler ranking;
- lease renewal;
- reserved-to-running transition;
- executor startup;
- Boxd or Firecracker;
- VM/container creation;
- runner registration.

## Next boundary

The first real executor proof can now consume an active lease from the single-authority store.

The next domain transition should make execution ownership explicit:

```text
RESERVED
  -> STARTING
  -> RUNNING
  -> FINISHED / FAILED
```

The `RESERVED -> STARTING` transition must atomically convert reserved accounting into execution ownership before a Boxd/Firecracker process is launched.
