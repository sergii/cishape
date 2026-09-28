# Security Policy

CIShape runs inside CI execution environments and may eventually observe process and infrastructure metadata. Security boundaries are therefore part of the product contract.

## Current status

CIShape is pre-release research software. There is not yet a supported stable release line.

Do not use the project as a security boundary.

## Reporting a vulnerability

Do not publish vulnerability details in a public issue.

Prefer GitHub private vulnerability reporting from the repository Security tab when available. If a private reporting channel is unavailable, open a minimal public issue asking the maintainers for a private contact path without including exploit details.

## Data-minimization policy

The observer should not collect by default:

- environment variable values
- secrets or credentials
- source code
- command stdout/stderr payloads
- arbitrary file contents

Telemetry schemas must explicitly document any new collected field.

## Supply-chain direction

Before the first public binary release, CIShape intends to add:

- dependency advisory and license policy checks
- GitHub Actions security analysis
- SemVer compatibility checks where applicable
- reproducible release automation
- release checksums
- SBOM generation
- artifact provenance/attestations

See `docs/releasing.md`.
