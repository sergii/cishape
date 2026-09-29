# Advisory execution planning

Status: EXECUTION1 contract  
Side effects: None

CIShape can emit a provider-neutral `ExecutionPlan` for an external CI control plane without provisioning machines, registering runners, mutating workflows, or routing jobs itself.

## Boundary

```text
RunObservation history
        |
        v
     JobShape
        |
        + OptimizationPolicy
        |
        v
deterministic RunnerShape recommendation
        |
        + explicit ExecutionRequirements
        |
        v
   ExecutionPlan
        |
        v
external control plane / executor binding
```

The `ExecutionPlan` is advisory evidence. CIShape remains a profiler and optimizer.

## Why ExecutionRequirements are separate

The current `JobShape` proves historical duration, CPU demand, memory demand, scope identity, and the observed runner shape.

It does not prove:

- whether repository code is trusted;
- whether a separate guest-kernel boundary is required;
- whether CPU or host tenancy must be exclusive;
- whether a cache should be ephemeral, warm, or persistent;
- useful workload parallelism;
- required architecture or special worker capabilities.

EXECUTION1 therefore does not infer those facts from a job name, repository name, provider label, or hidden heuristic.

The caller supplies a versioned `ExecutionRequirements` document.

## Facets, not one execution-size enum

Execution environment and resource tenancy are independent dimensions.

For example, this is a valid plan:

```text
environment:   microvm
cpu_tenancy:  exclusive
host_tenancy: shared
```

A microVM is an isolation environment. A dedicated host is a placement/tenancy property. Treating `process`, `container`, `microvm`, and `dedicated` as one enum would lose valid combinations.

EXECUTION1 models:

- `environment`: `process`, `container`, or `microvm`;
- `cpu_tenancy`: `shared` or `exclusive`;
- `host_tenancy`: `shared` or `dedicated`;
- optional cache requirement;
- optional maximum parallelism;
- optional provider-neutral placement requirements.

## Planner v1 rules

The sizing decision is reused from the existing deterministic optimizer.

```text
target_runner = Recommendation.recommended.shape
predicted_p95 = Recommendation.predicted_p95_ms
```

Isolation is deterministic:

```text
untrusted code                  -> microvm
minimum isolation = kernel      -> microvm
minimum isolation = container   -> container
otherwise                       -> process
```

CPU tenancy, host tenancy, cache, maximum parallelism, and placement are copied from explicit requirements. They are not inferred from current JobShape evidence.

An untrusted workload can therefore request `process` as its declared minimum and still be escalated to `microvm` by planner v1.

## Example requirements

`examples/execution-requirements/untrusted-pr.json`:

```json
{
  "schema_version": 1,
  "requirements_id": "untrusted-pr-v1",
  "code_trust": "untrusted",
  "minimum_isolation": "container",
  "cpu_tenancy": "shared",
  "host_tenancy": "shared",
  "cache": "warm_preferred",
  "max_parallelism": 4,
  "placement": {
    "architecture": "x86_64",
    "required_capabilities": []
  }
}
```

Generate a plan from existing local history:

```bash
cishape execution-plan test \
  --repository acme/api \
  --requirements examples/execution-requirements/untrusted-pr.json
```

Write portable JSON evidence:

```bash
cishape execution-plan test \
  --repository acme/api \
  --requirements examples/execution-requirements/untrusted-pr.json \
  --output .cishape/execution/test.json
```

Representative output:

```json
{
  "schema_version": 1,
  "planner_version": "execution-plan-v1",
  "requirements_id": "untrusted-pr-v1",
  "job": "test",
  "repository": "acme/api",
  "target_runner": {
    "cpu_millis": 4000,
    "memory_bytes": 8589934592
  },
  "predicted_p95_ms": 42000.0,
  "environment": "microvm",
  "cpu_tenancy": "shared",
  "host_tenancy": "shared",
  "cache": "warm_preferred",
  "max_parallelism": 4,
  "placement": {
    "architecture": "x86_64",
    "required_capabilities": []
  },
  "sizing_algorithm": "deterministic-fit-v1@default-v1:policy-schema-v1"
}
```

## What this does not decide

EXECUTION1 does not select:

- Firecracker versus boxd versus another microVM implementation;
- Kubernetes versus a custom scheduler;
- Hetzner, AWS, GCP, OVH, or another provider;
- a concrete host or availability zone;
- a provider runner offer or price;
- a runner registration mechanism.

Those are later binding and control-plane concerns.

## Future binding boundary

A future external execution plane can bind the provider-neutral plan:

```text
ExecutionPlan
    |
    +-- environment=microvm
    +-- CPU4-MEM8
    +-- x86_64
    +-- warm cache preferred
    |
    v
ExecutorCatalog
    |
    +-- boxd worker
    +-- native Firecracker worker
    +-- cloud VM pool
    +-- on-prem BYOC worker
    |
    v
BoundExecution
```

The binding layer may consider live capacity, provider economics, cache locality, and worker capabilities. It must not retroactively change what the original `ExecutionPlan` claimed was evidenced.

## Non-goals

EXECUTION1 has no:

- VM creation;
- container creation;
- scheduler;
- GitHub runner registration;
- workflow mutation;
- autoscaling;
- secret distribution;
- remote command execution;
- managed compute.

Those capabilities require separate threat models and product decisions.
