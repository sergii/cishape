# Decision outcomes

CIShape can evaluate a recorded deterministic decision against later observations without changing CI execution.

The loop is:

```text
history
  -> deterministic decision
  -> operator changes runner
  -> later observations
  -> outcome evidence
```

## CLI

First create a decision:

```bash
cishape decide \
  --repository sergii/cishape \
  --policy policies/default-v1.json \
  --output .cishape/decisions/test.json \
  test
```

Later, after new runs have accumulated:

```bash
cishape outcome \
  --decision .cishape/decisions/test.json \
  --policy policies/default-v1.json \
  --format markdown
```

JSON is also available:

```bash
cishape outcome \
  --decision .cishape/decisions/test.json \
  --policy policies/default-v1.json \
  --format json
```

## Matching rules

OUTCOME1 uses only observations that:

- occur after the decision timestamp
- match the same repository and logical job
- ran on the selected canonical RunnerShape

This lets CIShape distinguish a recommendation that was never applied from one that has real follow-up evidence.

## Statuses

`not_applied`
: no later observation ran on the selected RunnerShape.

`insufficient_evidence`
: the selected shape has been observed, but fewer than policy `min_runs` matching runs exist.

`within_prediction`
: there is enough successful evidence and observed p95 duration is at or below the recorded predicted p95.

`outside_prediction`
: there is enough successful evidence but observed p95 duration is above the recorded prediction.

`mixed_failures`
: there is enough matching evidence and at least one matching run exited non-zero.

## Non-causal boundary

Outcome evaluation is observational.

A failed test, build error, infrastructure outage, flaky test, dependency problem, or application regression can all occur after a runner change. CIShape therefore records `mixed_failures` rather than claiming that the selected runner caused the failure.

Likewise, a latency change is recorded as an observed difference, not as proof of causation.

## DecisionRecord v2

Decision evidence now contains the fields needed for later evaluation:

- baseline RunnerShape
- selected RunnerShape
- baseline p95 duration
- predicted p95 duration
- deterministic algorithm/policy identity

Older DecisionRecord JSON remains parseable, but it cannot be outcome-evaluated unless those fields are present.

## Current boundary

OUTCOME1 is local, deterministic, and backend-free.

It does not:

- modify `runs-on`
- trigger migrations between providers
- claim cost savings from fixed-capacity hosts
- make causal claims
- require Jev or another external decision provider
