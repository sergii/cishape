# Execution ownership and Boxd backend

Status: EXECUTOR1 contract  
First concrete backend: Boxd CLI

EXECUTOR1 turns an authoritative active reservation into a real execution lifecycle.

```text
active ReservationLease
  |
  v
atomic execution claim
  |
  +-- lease: active -> claimed
  +-- reserved_capacity -> allocated_capacity
  +-- reserved_allocations - 1
  +-- running_allocations + 1
  +-- worker state_revision + 1
  +-- ExecutionRecord(STARTING)
  |
  v
backend create
  |
  v
ExecutionRecord(RUNNING)
  |
  v
backend exec argv
  |
  v
backend teardown
  |
  v
ExecutionRecord(SUCCEEDED | FAILED)
  |
  +-- allocated_capacity released
  +-- running_allocations - 1
  +-- worker state_revision + 1
```

## Why lease claim is a separate transition

A reservation owns resources before a backend process exists.

Once execution starts, those same resources must not remain in `reserved_capacity`, because reservation expiry/release could otherwise return resources while a VM is still running.

Claim therefore atomically changes ownership:

```text
reserved -> running allocation
```

The lease becomes `claimed`. A claimed lease is terminal from the reservation subsystem's perspective.

Only execution completion returns the running allocation.

## Execution states

### STARTING

The control plane owns the reserved resources and has created an authoritative execution record, but no backend resource is known to be ready yet.

### RUNNING

A backend resource exists and its identifier is persisted.

### SUCCEEDED

The command returned exit code 0 and backend teardown succeeded.

### FAILED

Any of the following makes the execution fail:

- backend creation failure;
- backend invocation failure;
- non-zero guest command exit status;
- teardown failure.

A failed execution still returns its CIShape running allocation through the authoritative store transition.

## Authoritative execution records

`ExecutionRecord` stores:

- execution ID;
- lease ID;
- worker/executor identity;
- exact ExecutionPlan;
- backend kind;
- command argv;
- lifecycle timestamps;
- backend resource ID;
- stdout/stderr;
- exit code;
- error evidence;
- cleanup outcome.

The reservation database adds an `executions` table with indexed identity/status fields plus canonical ExecutionRecord JSON.

Reads validate the indexed fields against the canonical JSON and fail closed on disagreement.

## Atomic claim

`ReservationStore::claim_execution` performs one transaction:

1. check execution-id idempotency;
2. prove the lease is active and unexpired;
3. prove the lease maps to no other execution;
4. load authoritative worker state;
5. move lease resources from reserved to allocated;
6. advance worker revision;
7. mark the lease claimed;
8. insert ExecutionRecord(STARTING);
9. commit.

The same execution ID with the same lease/backend/argv is idempotent.

The same execution ID with different semantics fails closed.

A lease may belong to at most one execution.

## RUNNING transition

After the backend creates a machine, the backend resource ID is persisted with a status compare-and-swap:

```text
STARTING -> RUNNING
```

This does not change worker allocation accounting, so the worker scheduling revision does not advance.

## Finish transition

Execution finish is another database transaction:

```text
STARTING | RUNNING
  -> SUCCEEDED | FAILED
```

and atomically:

- subtracts the execution RunnerShape from `allocated_capacity`;
- decrements `running_allocations`;
- advances worker revision;
- persists terminal execution evidence.

## ExecutorBackend

The runtime backend is provider-neutral:

```text
validate_plan
create
exec
destroy
```

EXECUTOR1 provides the first implementation: `BoxdCliBackend`.

## Boxd CLI backend

The lifecycle used by the adapter is intentionally small:

```text
boxd machine new <machine> --isolated
boxd machine exec <machine> -- <argv...>
boxd machine remove <machine> -y
```

Arguments are passed with `std::process::Command`; CIShape does not join the workload command into a host shell string.

Machine names are deterministic opaque hashes of `execution_id`, so repository/job names are not leaked into the backend resource name.

### Shape boundary

The public Boxd machine shape used by this proof is:

```text
2 vCPU
8 GiB RAM
```

The adapter therefore accepts exactly `CPU2-MEM8`.

It does not silently map:

```text
CPU1-MEM4 -> Boxd CPU2-MEM8
CPU4-MEM8 -> Boxd CPU2-MEM8
```

Both would make CIShape accounting inconsistent with actual execution capacity.

Future resource-aware provider APIs can add explicit shape mappings through a versioned backend catalog.

The initial backend also requires:

- microVM execution environment;
- shared CPU tenancy;
- shared host tenancy.

It fails closed for exclusive CPU or dedicated-host requirements because the public backend contract does not prove those semantics.

## CLI

Given an active authoritative CPU2-MEM8 lease:

```bash
cishape executor-run-boxd \
  --db .cishape/reservations.duckdb \
  --lease-id req-example-001 \
  --execution-id exec-example-001 \
  -- /bin/sh -c 'echo cishape'
```

The local `boxd` CLI must already be authenticated, or the environment must provide authentication accepted by Boxd.

For testing a non-standard CLI path:

```bash
cishape executor-run-boxd \
  --boxd-binary /path/to/boxd \
  ...
```

## Cleanup semantics

CIShape attempts backend teardown after command execution whether the command succeeds or fails.

A teardown failure makes the execution FAILED even when the guest command returned 0, because leaked runtime resources are not a successful execution outcome.

If backend creation fails before a resource exists, the execution is finished as FAILED with `cleanup=not_attempted`.

## Crash boundary

EXECUTOR1 does not yet implement a sweeper for a process crash between:

```text
STARTING
RUNNING
terminal completion
```

The authoritative record makes those states visible, but automated reconciliation is a follow-up.

Until reconciliation exists, an interrupted STARTING/RUNNING execution must not be blindly re-run. The CLI fails visibly when it finds a non-terminal replay it cannot safely recover.

## CI proof versus real microVM proof

CI uses a fake `boxd` executable to prove:

- exact command construction;
- argv is not interpolated through a shell;
- create/exec/remove orchestration;
- exit-code/stdout capture;
- execution accounting transitions.

That is not presented as a real KVM boot.

A real proof requires authenticated Boxd access and runs the same `executor-run-boxd` command against the real CLI.

## Non-goals

EXECUTOR1 does not add:

- GitHub runner registration;
- repository checkout/bootstrap;
- scheduler ranking;
- arbitrary provider shape mapping;
- streaming logs;
- lease renewal;
- crash reconciliation;
- multi-instance control-plane storage;
- native Firecracker lifecycle.

## Next boundaries

After the first real Boxd proof:

- EXECUTOR2 can add repository checkout/bootstrap/scenario semantics;
- RECOVERY1 can reconcile stale STARTING/RUNNING records and leaked backend resources;
- FIRECRACKER1 can implement the same ExecutorBackend contract on a BYOC KVM host;
- a Postgres ReservationStore remains required before horizontally scaling the control plane.
