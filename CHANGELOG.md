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
- Add an opt-in live TypeSafe Jev transport that preserves the same fail-closed shadow boundary.
- Add explicit live TypeSafe Jev transport with Rust TLS and preserved provider response evidence.
- Add a manual GitHub Actions Jev shadow proof over rolling CIShape history.
- Add a first-class deterministic DecisionProvider and local `decide` flow so Jev is not on the critical path.
- Add deterministic Markdown/JSON advisory reports with evidence gating and rolling-history GitHub Actions dogfood.
- Add versioned provider catalogs with pricing provenance, billing increments, incomplete-data safety, and deterministic managed-offer fitting.
- Add versioned deterministic optimization policies for evidence thresholds, CPU/RAM safety factors, latency guardrails, and objectives.
- Add DecisionRecord v2 and backend-free outcome evaluation against later matching CI observations.

### Documentation

- Define architecture boundaries, roadmap, telemetry model, CI strategy, and the future HTTP boundary.
