# Telemetry model v0

Status: POC contract through PORTABLE1.

CIShape separates raw runtime evidence from derived historical profiles.

## RunObservation

One observed command produces one normalized `RunObservation`.

Core fields:

- `schema_version`
- `job`
- `observed_at_unix_ms`
- `duration_ms`
- `cpu_seconds`
- `cpu_peak_millis`
- `memory_peak_bytes`
- `read_bytes`
- `write_bytes`
- `runner`
- provider-neutral CI identity: provider/repository/workflow/run/attempt/workflow-job/commit/ref
- optional runner instance name
- optional queue/cost metadata
- `exit_code`

Unknown provider, queue, or cost values are represented as unknown, not zero.

## Exact vs sampled evidence

OBSERVE1 is Linux-first and deliberately distinguishes accounting from sampling.

`duration_ms`
: wall-clock measurement around the observed child process.

`cpu_seconds`
: process-family CPU accounting from Linux child resource usage.

`cpu_peak_millis`
: sampled estimate of instantaneous CPU demand across the observed process tree.

`memory_peak_bytes`
: sampled aggregate resident memory across the observed process tree.

`read_bytes` / `write_bytes`
: best-effort cumulative process-tree I/O from `/proc/<pid>/io`.

Short-lived descendants can exist between samples. A later cgroup-based collector can provide stronger attribution where CI environments permit cgroup delegation.

The schema must preserve this distinction instead of presenting sampled values as exact kernel accounting.

## RunnerShape

Runner capacity is normalized independently from provider marketing labels.

Machine-readable ID:

```text
cpu4-mem16
```

Human display:

```text
CPU4-MEM16
```

OBSERVE1 detects CPU capacity from the process-visible CPU set/quota and memory capacity from host memory constrained by cgroup v2 limits when available.

## CI identity

CI identity is optional enrichment, not the workload telemetry source of truth.

RunObservation v2 can carry:

- CI provider
- repository
- workflow
- run id
- run attempt
- workflow job
- commit SHA
- git ref

The logical CIShape workload `job` remains separate. For example, GitHub workflow job `check` can contain CIShape workloads `test`, `clippy`, and `demo`.

The observer reads only an allowlist of provider metadata variables. It does not enumerate or persist arbitrary environment values.

Schema v1 provider fields remain import-compatible and are normalized into the v2 identity.

## Privacy

The default observer does not persist:

- source code
- environment variable values
- secrets
- stdout/stderr payloads
- arbitrary file contents
- command-line arguments

The observed child inherits normal stdin/stdout/stderr behavior, but CIShape does not capture those streams into telemetry.

## History

Local runs are stored in DuckDB and can also be emitted as JSON evidence.

Portable JSON/JSONL observations can be imported from multiple ephemeral CI runs into one local DuckDB. Imports are idempotent.

Historical `JobShape` values are derived from multiple `RunObservation` rows. They are not stored as if they were raw measurements.

DuckDB is a local analytical implementation detail; JSON/JSONL is the portable interchange boundary in PORTABLE1.

## Compatibility

The observation carries an explicit `schema_version`.

The current schema version is 2. Portable import accepts versions 1 and 2 and canonicalizes old provider fields to v2.

Before the first stable release, local DuckDB schema evolution may require moving or deleting a POC database. Portable JSON contracts will receive explicit migration guarantees before OpenAPI v1 is declared stable.
