# ADR-0002: DuckDB for local analytical history

Status: Accepted for local-first POC

## Context

CIShape history is analytical rather than transactional.

Typical queries include:

- p50/p95/p99 duration
- p95 CPU demand
- p99 memory demand
- comparison by job, runner shape, provider, and time window
- cost and queue distributions
- historical trend analysis

## Decision

Use DuckDB for the local-first history store.

POC0 stores normalized run summaries in DuckDB.

Future high-frequency raw samples may be written to Parquet and queried through DuckDB instead of being retained permanently in the main database.

## Why not SQLite as the primary analytical store

SQLite would be a good fit for configuration and transactional application state.

CIShape's dominant local workload is percentile-heavy historical analysis over append-oriented observations, which better matches DuckDB's columnar analytical design.

## Boundary

DuckDB is not hereby selected as the future multi-tenant SaaS transactional database.

A cloud ingestion service will have different concurrency and durability requirements and can use a separate metadata/ingest store while preserving DuckDB/Parquet for analytics.
