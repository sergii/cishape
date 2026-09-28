# Deterministic advisory

CIShape can turn accumulated workload history into a read-only advisory report without Jev, credentials, or a backend.

## CLI

Markdown:

```bash
cishape advisory \
  --repository sergii/cishape \
  --policy policies/default-v1.json \
  --format markdown
```

Machine-readable JSON:

```bash
cishape advisory \
  --repository sergii/cishape \
  --policy policies/default-v1.json \
  --format json
```

## Evidence gate

A workload is not actionable until it has at least the policy's `min_runs` observations.

Before that threshold the action is:

```text
insufficient_evidence
```

The checked-in `default-v1` policy uses 10 runs. The advisory records the policy ID and schema version in both Markdown and JSON evidence.

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

The current built-in catalog is the canonical shape catalog used by the deterministic optimizer. Provider offer mapping is modeled separately in the versioned provider catalog, and outcome/capacity economics remain follow-ups.

## GitHub Actions dogfood

`.github/workflows/advisory.yml` consumes the newest non-expired `cishape-history-*` artifact and publishes:

- `advisory.md` to the GitHub job summary
- `advisory.md` as an artifact
- `advisory.json` as an artifact

The workflow is secret-free and has read-only repository/actions permissions.

It never changes `runs-on`, edits workflow files, or reroutes jobs.
