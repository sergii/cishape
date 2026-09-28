# Portable history

CIShape can aggregate observations from ephemeral CI runners without requiring a backend.

## Flow

```text
CI run A -> run-a.json \
CI run B -> run-b.json  +-> cishape import -> one DuckDB -> profile/recommend
CI run C -> run-c.json /
```

Import one or more JSON or JSONL files:

```bash
cishape import artifact-a/*.json artifact-b/*.json
```

Import is idempotent. Re-importing the same observation does not increase the historical run count.

When one history database contains the same logical job name from multiple repositories, CIShape refuses an unscoped profile:

```bash
cishape profile --repository acme/api test
```

This prevents a generic name such as `test` from accidentally mixing unrelated workloads across repositories.

Export the normalized local history as JSONL:

```bash
cishape export --format jsonl --output history.jsonl
```

JSONL is the first portable interchange format because it is streamable, diffable, easy to archive, and does not require a service.

## CI identity

RunObservation v2 carries optional provider-neutral CI identity:

- provider
- repository
- workflow
- run id
- run attempt
- workflow job
- commit SHA
- git ref

These fields are metadata for correlation. The workload `job` remains a CIShape logical workload name such as `test`, `clippy`, or `demo`.

## Idempotency

CIShape derives a stable observation identity from the CI identity, logical workload, and observation timestamp.

For local observations without CI metadata, the identity falls back to local source markers plus the observation timestamp.

## Compatibility

Portable interchange currently accepts RunObservation schema versions 1 and 2.

Schema v1 provider fields are normalized into the v2 CI identity on import. Exports always use the current canonical schema.

The local DuckDB schema itself is still pre-release and is not a portable contract. Old POC DuckDB files may need to be moved or removed after schema changes. JSON/JSONL is the portable boundary.
