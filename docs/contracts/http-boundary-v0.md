# Future HTTP boundary

Status: Non-normative design note

A full OpenAPI contract is intentionally deferred until POC1 stabilizes the serialized RunObservation and JobShape models.

The boundary should still be defined now so the local core does not become coupled to a future transport.

## Candidate resources

The likely cloud-facing concepts are:

```text
RunObservation
JobProfile
RunnerCatalog
Recommendation
DecisionRecord
```

A future HTTP surface may resemble:

```text
POST /v1/runs
GET  /v1/jobs/{job}/profile
GET  /v1/jobs/{job}/recommendations
POST /v1/decisions
```

These endpoint names are placeholders, not a compatibility promise.

## Rules before OpenAPI exists

- domain types must not import HTTP concepts
- provider adapters must not become the canonical API model
- serialized IDs should be opaque
- adding optional evidence should remain forward-compatible
- raw telemetry ingestion and summarized run ingestion are separate concerns
- recommendations must identify algorithm/policy versions
- decisions must preserve the evidence/recommendation they were based on
- execution side effects remain outside POC0/POC1

## Gate for OpenAPI v1

Create the first normative OpenAPI document only after:

1. real `cishape observe` data exists
2. RunObservation fields have survived real CI use
3. the project has dogfooded at least one CI provider
4. we know whether ingestion should be per-sample, per-run, OTLP-derived, or a combination
