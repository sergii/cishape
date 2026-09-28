# Deterministic advisory

CIShape can turn accumulated workload history into a read-only advisory report without Jev, credentials, or a backend.

## CLI

Markdown:

```bash
cishape advisory \
  --repository sergii/cishape \
  --min-runs 10 \
  --format markdown
```

Machine-readable JSON:

```bash
cishape advisory \
  --repository sergii/cishape \
  --min-runs 10 \
  --format json
```

## Evidence gate

A workload is not actionable until it has at least `min-runs` observations.

Before that threshold the action is:

```text
insufficient_evidence
```

The default CLI threshold is 10 runs.

## Actions

For workloads with enough evidence, CIShape compares the current canonical RunnerShape with the deterministic target:

- `keep` - target CPU and RAM equal the current shape
- `downsize` - neither CPU nor RAM increases, and at least one decreases
- `upsize` - neither CPU nor RAM decreases, and at least one increases
- `reshape` - one resource dimension increases while another decreases
- `no_candidate` - the current catalog contains no feasible target

CPU and memory remain independent dimensions.

## Important limitation

ADVISORY1 intentionally does not claim provider-specific dollar savings.

The current built-in catalog is a canonical shape catalog used to exercise the optimizer. Real provider aliases, prices, billing intervals, isolation classes, and queue behavior belong to the provider-catalog slice.

## GitHub Actions dogfood

`.github/workflows/advisory.yml` consumes the newest non-expired `cishape-history-*` artifact and publishes:

- `advisory.md` to the GitHub job summary
- `advisory.md` as an artifact
- `advisory.json` as an artifact

The workflow is secret-free and has read-only repository/actions permissions.

It never changes `runs-on`, edits workflow files, or reroutes jobs.
