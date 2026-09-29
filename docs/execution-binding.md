# Execution binding

Status: BINDING1 contract  
Side effects: None

BINDING1 matches a portable `ExecutionPlan` against a static `ExecutorCatalog`.

It answers:

> Which executor targets are capable of satisfying this plan, and why are the others incompatible?

It does not answer:

> Which currently available worker should run this job?

That second question requires live capacity and scheduling state and is intentionally deferred.

## Flow

```text
ExecutionPlan
   +
ExecutorCatalog
   |
   v
BindingReport
   |
   +-- compatible targets
   |
   +-- explicit exclusion reasons
   |
   +-- non-binding preference evidence
   |
   v
external control plane
```

## Static capability catalog

An `ExecutorTarget` describes stable or configuration-level capabilities:

- supported execution environments;
- maximum allocatable CPU and memory;
- supported CPU tenancy;
- supported host tenancy;
- supported architectures;
- explicit capabilities;
- supported cache modes.

It deliberately does not contain:

- free CPU or RAM right now;
- queue depth;
- running jobs;
- worker heartbeat/liveness;
- current cache warmth;
- price;
- billing;
- provider queue semantics;
- scheduling priority.

Those facts change at runtime and belong in a future live-capacity layer.

## Compatibility checks

BINDING1 checks:

```text
environment
CPU capacity
memory capacity
CPU tenancy
host tenancy
architecture
required capabilities
cache requirement
```

Every rejected target carries structured exclusion evidence such as:

```text
unsupported_environment
insufficient_cpu
insufficient_memory
unsupported_cpu_tenancy
unsupported_host_tenancy
unsupported_architecture
missing_capability
ephemeral_cache_unavailable
persistent_cache_unavailable
```

No hidden score is used.

Targets and compatible target IDs are returned in stable executor-ID order.

## Cache semantics

Cache requirements are intentionally split into hard requirements and preferences.

`ephemeral` is a hard capability requirement.

`persistent_required` is a hard capability requirement.

`warm_preferred` is not a hard compatibility filter.

A target with persistent reusable cache capability may satisfy the structural preference, but BINDING1 does not claim that its cache is warm now. Current cache state is runtime evidence.

## Example

First create an ExecutionPlan:

```bash
cishape execution-plan test \
  --repository acme/api \
  --requirements examples/execution-requirements/untrusted-pr.json \
  --output .cishape/execution/test.json
```

Then fit it to the illustrative executor catalog:

```bash
cishape execution-fit \
  --plan .cishape/execution/test.json \
  --catalog catalogs/executors-v1.json
```

Representative shape:

```json
{
  "schema_version": 2,
  "algorithm": "executor-fit-v1",
  "catalog_id": "executors-v1-example",
  "job": "test",
  "repository": "acme/api",
  "plan": {
    "...": "exact ExecutionPlan evidence embedded here"
  },
  "compatible_executor_ids": [
    "boxd-like-microvm-x86_64",
    "byoc-dedicated-x86_64",
    "firecracker-like-microvm-x86_64"
  ],
  "fits": []
}
```

The checked-in catalog is illustrative. Names containing `boxd-like` or `firecracker-like` describe capability examples only. CIShape does not call those systems in BINDING1.

BindingReport schema v2 embeds the exact ExecutionPlan used for capability matching. Downstream admission must reject a binding artifact whose embedded plan differs from the supplied plan, preventing stale binding evidence from being reused after CPU/RAM/isolation requirements change.

## Why there is no selected executor

Compatibility and selection are different decisions.

Suppose three targets can execute the plan:

```text
boxd-like-microvm-x86_64
firecracker-like-microvm-x86_64
byoc-dedicated-x86_64
```

Picking one may depend on:

- worker availability;
- queue state;
- cache locality;
- data locality;
- price;
- customer/account boundary;
- provider preference;
- failure domain;
- scheduling policy.

BINDING1 has none of that evidence, so it must not invent a winner.

A future layer can combine the compatibility set with live state and an explicit versioned policy.

## Relationship to provider economics

The executor catalog is not the provider offer catalog.

```text
ExecutionPlan
   -> ExecutorCatalog
   -> capability compatibility

RunnerShape / workload demand
   -> ProviderCatalog
   -> provider economics
```

A later control plane may join these layers through explicit IDs or bindings, but static capability matching must not silently turn into price selection.

## Future progression

```text
BINDING1
ExecutionPlan + static capabilities
        |
        v
compatible executor set

CAPACITY / WORKERSTATE
live worker/pool state
        |
        v
eligible available targets

SCHEDULING
explicit policy + reservation
        |
        v
BoundExecution

EXECUTOR
provision / start / run / collect / destroy
```

Each layer should remain separately testable and evidence-preserving.
