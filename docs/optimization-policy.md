# Optimization policy

CIShape deterministic recommendations are a function of four inputs:

```text
evidence + canonical runner catalog + optimization policy + algorithm version
```

POLICY1 makes the policy explicit and versioned.

## Default policy

The repository ships:

```text
policies/default-v1.json
```

Current defaults preserve the pre-policy behavior:

```json
{
  "schema_version": 1,
  "policy_id": "default-v1",
  "min_runs": 10,
  "cpu_safety_factor": 1.5,
  "memory_safety_factor": 1.5,
  "latency_penalty": 1.05,
  "max_predicted_p95_ms": null,
  "objective": "minimize_cost"
}
```

## Fields

`min_runs`
: minimum historical observations before advisory output becomes actionable.

`cpu_safety_factor`
: required runner CPU capacity divided by historical CPU p95 demand.

`memory_safety_factor`
: required runner memory capacity divided by historical memory p99 demand.

`latency_penalty`
: conservative multiplier applied to historical p95 duration while CIShape still lacks a learned runner-performance model.

`max_predicted_p95_ms`
: optional hard latency guard. If the predicted p95 exceeds it, the current deterministic model returns no feasible recommendation.

`objective`
: deterministic ordering objective. POLICY1 supports `minimize_cost`.

## CLI

All decision-producing commands accept a policy file:

```bash
cishape recommend \
  --repository sergii/cishape \
  --policy policies/default-v1.json \
  test

cishape explain \
  --repository sergii/cishape \
  --policy policies/default-v1.json \
  test

cishape decide \
  --repository sergii/cishape \
  --policy policies/default-v1.json \
  test

cishape advisory \
  --repository sergii/cishape \
  --policy policies/default-v1.json \
  --format markdown
```

The Jev shadow request preparation command also consumes the same policy so the optional provider sees only the same deterministic feasible set.

## Reproducibility

Recommendation evidence includes an algorithm identity such as:

```text
deterministic-fit-v1@default-v1:policy-schema-v1
```

Advisory JSON/Markdown also records the policy ID and schema version.

A policy file must be treated like code:

- review changes
- commit it
- keep source control history
- do not silently mutate defaults based on remote provider state

## Safety

Policy validation rejects:

- unknown schema versions
- empty policy IDs
- zero evidence thresholds
- CPU or memory safety factors below 1.0
- latency penalty below 1.0
- non-positive latency guards

Policy is deterministic configuration. It cannot bypass provider trust/isolation requirements that will be modeled as separate hard constraints in later slices.


## Provider economics policy

The canonical runner optimization policy answers a different question from provider economics.

`policies/default-v1.json` controls workload-shape safety:

```text
JobShape -> canonical RunnerShape
```

`policies/economics-default-v1.json` controls provider-offer selection after CAPACITY1 has produced cost and time-to-green evidence:

```text
EconomicsReport -> eligible provider offers -> deterministic selection
```

The policy classes remain separate so provider queue pressure, utilization, and billing behavior cannot leak into the canonical workload-shape model.
