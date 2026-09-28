# Workflow DAG economics

CAPACITY4 extends CIShape from one job or N identical independent jobs to a heterogeneous CI workflow DAG.

The input is a versioned `WorkflowDemand`:

```text
stable job id
+ canonical RunnerShape
+ predicted warm runtime
+ dependency ids
```

CIShape validates that job IDs are unique, dependencies exist, and the graph is acyclic before any economics evaluation.

## Evaluation boundary

CAPACITY4 evaluates one provider offer as the execution environment for the entire workflow.

It intentionally does not mix providers or runner offers inside one workflow evaluation. Cross-provider placement and automatic routing would be a different optimization/control-plane problem.

For every capacity snapshot state, CIShape:

1. checks that the provider offer can satisfy every job shape
2. computes each job's effective runtime and cost using CAPACITY1 billing/cache semantics
3. seeds slot availability from currently running work and already queued work
4. schedules workflow jobs only after all dependencies complete and a slot is available
5. emits per-job start, finish, slot, runtime, and cost
6. emits intrinsic critical path, workflow time-to-green, and total effective cost
7. marks the workflow Pareto frontier
8. applies the same versioned EconomicsPolicy to workflow totals

## Deterministic scheduler

The scheduler uses explicit list scheduling.

For each unscheduled job whose dependencies have already been scheduled:

- dependency readiness is the latest dependency finish
- each slot's feasible start is `max(slot_available_at, dependency_ready_at)`
- the next assignment is the lowest tuple of:
  1. feasible start
  2. job ID
  3. slot index

Existing queued work is placed before workflow jobs using the observed `slot_turnover_ms`.

The reported `critical_path_ms` is dependency-only and uses the effective runtimes for that offer. `time_to_green_ms` additionally includes queue-ahead work and slot contention.

## CLI

```bash
cargo run -- workflow-economics \
  --workflow examples/workflow-demand-v1.json \
  --snapshot examples/workflow-capacity-snapshot-v1.json \
  --policy policies/economics-default-v1.json \
  --format markdown
```

The example contains fan-out/fan-in jobs with different canonical runner shapes. The synthetic snapshot demonstrates that a cheap two-slot persistent pool can lose the 30-second policy SLA while a wider managed pool remains eligible.

## Boundary

CAPACITY4 remains read-only and advisory-only.

It does not:

- rewrite CI configuration
- dispatch jobs
- split one workflow across providers
- infer queue depth or autoscaling behavior
- guess cache penalties
- claim provider capacity that is not present in the snapshot

The next capacity slice should replace synthetic capacity fixtures with real provider adapters that emit the same time-scoped `CapacitySnapshot` contract.
