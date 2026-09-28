# ADR-0001: Rust for the CIShape executable and core

Status: Accepted

## Context

The long-term executable is expected to supervise child processes, inspect Linux process/cgroup accounting, collect low-overhead telemetry, and ship as a single cross-platform binary.

The observer should distort the measured workload as little as practical.

## Decision

Use Rust for:

- the CLI
- observation
- normalization
- profiling
- optimization
- local storage integration

## Consequences

Benefits:

- low runtime overhead
- no garbage collector
- strong process/system programming ecosystem
- single-binary distribution
- a natural path toward cgroup, namespace, PSI, and optional eBPF integrations

Cost:

- implementation complexity is higher than a comparable Go prototype
- DuckDB native build/linking may make CI compilation heavier

This is acceptable because the observer itself is part of the product, not only a temporary POC shell.
