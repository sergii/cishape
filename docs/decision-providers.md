# Decision providers

CIShape separates deterministic feasibility from optional decision intelligence.

## Flow

```text
RunObservation history
        |
        v
     JobShape
        |
        v
hard deterministic constraints
        |
        v
feasible RunnerShape candidates
        |
        +--> deterministic baseline
        |
        v
optional DecisionProvider
        |
        v
   DecisionRecord
```

A DecisionProvider never receives runner choices that failed hard CIShape resource constraints.

## Jev shadow provider

JEV1 targets TypeSafe AI's Jev System One interface.

CIShape prepares an offline request bundle:

```bash
cishape decision-prepare \
  --repository sergii/cishape \
  --output .cishape/decisions/test-request.json \
  test
```

The bundle contains two layers:

- a provider-neutral CIShape `DecisionRequest`
- the exact Jev wire request with a typed `choice` question

The request state contains only normalized workload evidence and feasible candidate metrics. It does not include source code, environment values, secrets, stdout/stderr, or arbitrary logs.

JEV1 deliberately does not call the network. A Jev response can be recorded offline:

```bash
cishape decision-record \
  --request .cishape/decisions/test-request.json \
  --response jev-response.json
```

The response becomes a `DecisionRecord` containing:

- selected feasible candidate
- confidence
- per-candidate probabilities
- resolved Jev model
- token usage
- whether Jev agreed with the deterministic baseline
- evidence run count and workload scope

The record is persisted separately from runtime observations.

## Why shadow mode first

Shadow mode has no execution side effects.

This lets CIShape compare:

```text
deterministic baseline
vs
Jev decision
vs
later observed outcome
```

before any automated runner routing exists.

The progression remains:

```text
shadow -> recommend -> explicitly approved automation -> bounded low-risk autonomy
```

## Fail-closed behavior

CIShape rejects a Jev response when:

- the answer is missing
- the answer type is not `choice`
- the selected candidate was not in the deterministic feasible set
- a probability refers to an unknown candidate
- confidence/probabilities are outside 0..1

Jev cannot widen the candidate set.

## Live Jev transport

JEV2 adds an explicit network command while preserving the JEV1 contract:

```bash
export JEV_API_KEY=...

cishape decision-run \
  --request .cishape/decisions/test-request.json
```

By default CIShape sends the exact prepared wire request to:

```text
https://api.typesafe.ai/v1/systemone
```

The endpoint can be overridden for testing or a compatible gateway.

The API key is read from `JEV_API_KEY` by default, can be redirected to another environment variable with `--api-key-env`, and is never persisted or printed.

The live path is:

```text
prepared request
   -> HTTPS
   -> raw Jev response evidence
   -> existing fail-closed validator
   -> DecisionRecord
```

A transport error or non-success HTTP status produces no DecisionRecord. A syntactically valid response that selects an infeasible candidate is saved as provider evidence but still fails before a DecisionRecord is accepted.

JEV2 remains shadow-only and has no CI execution side effects.


## Credentialed GitHub proof

The repository includes a manual-only `Jev Shadow Proof` workflow.

It reads a repository secret named `JEV_API_KEY`, downloads the newest non-expired `cishape-history-*` artifact, prepares a bounded request from that rolling history, performs one live Jev call, validates it through the same fail-closed path, and uploads:

- the prepared request
- the raw Jev response
- the validated DecisionRecord

The workflow has only `contents: read` and `actions: read` permissions and never performs runner routing or workflow mutation.

This workflow is intentionally `workflow_dispatch` only. It does not run on pushes, pull requests, or a schedule.
