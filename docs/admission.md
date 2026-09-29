# Worker admission

Status: ADMISSION1 contract  
Side effects: None

ADMISSION1 evaluates whether concrete workers are currently eligible for a portable `ExecutionPlan`.

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

The result is advisory evidence. It does not reserve capacity and does not start work.

## Why admission is separate from scheduling

Admission answers:

> Which workers satisfy the current plan, runtime state, and explicit policy?

Scheduling answers a later question:

> Which admissible worker should receive the job, and can capacity be atomically reserved?

Those must remain separate.

Two schedulers can observe the same free capacity at the same time. Therefore `outcome=admit` is never a reservation guarantee. A future reservation/lease layer must perform an atomic state transition before execution.

## Inputs

### ExecutionPlan

Carries the workload's requested runner shape and execution requirements.

### BindingReport

Carries static executor compatibility evidence.

BindingReport schema v2 embeds the exact `ExecutionPlan` that produced it. ADMISSION1 rejects a binding artifact produced for a different plan.

### WorkerStateSnapshot

Carries concrete host runtime evidence:

- worker identity;
- executor identity;
- lifecycle;
- physical capacity;
- configured allocation limit;
- current allocated capacity;
- running/max allocation counts;
- optional pressure evidence.

### AdmissionPolicy

Carries all admission thresholds and capacity-basis choices as versioned data.

No threshold is hidden in code.

## Capacity basis

CPU and memory can independently use one of two bases.

### physical

The effective ceiling is:

```text
min(physical_capacity, allocation_limit)
```

This is the default.

A 32-core host with a 48-vCPU allocation limit therefore still exposes at most 32 cores to admission under the default CPU basis.

### allocation_limit

The configured allocation envelope may be used even when it exceeds physical capacity.

For CPU this can represent an explicit oversubscription policy.

For memory it can represent an explicit overcommit policy.

To prevent silent overcommit, ADMISSION1 requires a matching pressure threshold whenever `allocation_limit` is selected as the capacity basis.

This is still policy, not learned safety evidence. Future profiling-aware admission can replace or constrain operator-configured overcommit with workload-demand evidence.

## Default policy

`policies/admission-default-v1.json` is intentionally explicit:

```json
{
  "schema_version": 1,
  "policy_id": "admission-default-v1",
  "cpu_capacity_basis": "physical",
  "memory_capacity_basis": "physical",
  "reserve_cpu_millis": 1000,
  "reserve_memory_bytes": 1073741824,
  "max_cpu_utilization_ratio": 0.9,
  "max_memory_utilization_ratio": 0.9,
  "max_io_pressure_ratio": 0.9
}
```

The 90% thresholds and reserves are versioned operational policy choices. They are not presented as empirically optimal values.

If a configured threshold requires a pressure metric and the worker snapshot does not contain that metric, the worker fails closed for that admission evaluation.

## Worker checks

For each worker ADMISSION1 checks:

```text
executor is statically compatible
lifecycle == ready
running_allocations < max_allocations
policy-eligible CPU >= target CPU
policy-eligible RAM >= target RAM
post-admission reserve remains intact
configured pressure evidence is present
configured pressure thresholds are not exceeded
dedicated-host requirement is not already occupied
```

Exclusive CPU tenancy currently fails closed because WORKERSTATE1 does not yet prove free exclusive CPU/core topology.

That is deliberate. Generic free CPU millis are not enough evidence to claim that exclusive cores can be reserved.

## Outcomes

### admit

At least one concrete worker is currently admissible.

The report still includes every worker and its exclusions.

### defer

At least one worker belongs to a statically compatible executor type, but current runtime state or policy prevents admission.

Examples:

- insufficient current CPU;
- worker draining;
- pressure above policy;
- allocation-count limit reached.

### no_eligible_worker

The snapshot contains no worker attached to any statically compatible executor.

This is different from a temporarily busy compatible worker.

## Example flow

Create a plan:

```bash
cishape execution-plan test \
  --repository acme/api \
  --requirements examples/execution-requirements/untrusted-pr.json \
  --output .cishape/execution/test.json
```

Bind it to static executor capabilities:

```bash
cishape execution-fit \
  --plan .cishape/execution/test.json \
  --catalog catalogs/executors-v1.json \
  --output .cishape/execution/test-binding.json
```

Evaluate live worker admission:

```bash
cishape admission \
  --plan .cishape/execution/test.json \
  --binding .cishape/execution/test-binding.json \
  --workers examples/worker-state/workers-v1.json \
  --policy policies/admission-default-v1.json
```

Representative shape:

```json
{
  "schema_version": 1,
  "algorithm": "worker-admission-v1",
  "policy_id": "admission-default-v1",
  "job": "test",
  "repository": "acme/api",
  "outcome": "admit",
  "admissible_worker_ids": ["worker-a"],
  "workers": []
}
```

There is intentionally no `selected_worker_id`.

## Oversubscription example

Consider:

```text
physical CPU       32 cores
allocation limit   48 vCPU
allocated          30 vCPU
new plan            4 vCPU
```

With:

```json
"cpu_capacity_basis": "physical"
```

effective free CPU is 2 cores, so the plan is deferred.

With an explicit policy using:

```json
"cpu_capacity_basis": "allocation_limit",
"max_cpu_utilization_ratio": 0.9
```

effective allocation capacity is 18 vCPU before the new plan. The worker can pass the capacity check only if the required CPU pressure evidence is present and below the configured threshold.

That does not prove that oversubscription is optimal. It proves only that an explicit policy permitted it under the observed state.

## Non-goals

ADMISSION1 does not:

- mutate worker state;
- reserve capacity;
- choose a winner;
- prioritize a queue;
- create a VM or container;
- register a CI runner;
- infer workload demand from job names;
- learn an oversubscription ratio;
- call a provider;
- route a real job.

## Next boundary

The next control-plane contract should be reservation/lease semantics:

```text
AdmissionReport
  + current WorkerState version
  + requested worker
  |
  v
atomic Reservation / Lease
```

Only after that boundary exists is it safe to connect admission to a real Boxd or Firecracker executor.
