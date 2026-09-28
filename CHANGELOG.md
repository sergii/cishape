# Changelog

All notable user-facing changes to CIShape will be documented in this file.

The project follows Conventional Commits and will generate release changelogs deterministically from Git history.

## [Unreleased]

### Features

- Bootstrap the local-first synthetic CI workload shaping proof of concept.
- Add explicit runner shape identities such as `CPU2-MEM4`.
- Add DuckDB-backed historical profiling and deterministic recommendations.
- Add Linux child-process observation with versioned JSON evidence.
- Dogfood CIShape by observing its own GitHub Actions workloads and uploading run evidence.
- Add provider-neutral CI identity plus idempotent JSON/JSONL history import and export.
- Aggregate rolling main-branch CI observations into a portable historical report.
- Add an offline Jev shadow-decision contract over deterministic feasible runner candidates.

### Documentation

- Define architecture boundaries, roadmap, telemetry model, CI strategy, and the future HTTP boundary.
