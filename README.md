# CIShape

**Observe. Shape. Fit.**

CIShape is an early-stage open-source experiment for profiling CI workloads and fitting them to better runner shapes without depending on provider marketing labels such as `small`, `large`, or `xlarge`.

Canonical runner capacity is numeric:

```text
CPU2-MEM4   = 2 vCPU / 4 GiB RAM
CPU4-MEM8   = 4 vCPU / 8 GiB RAM
CPU8-MEM16  = 8 vCPU / 16 GiB RAM
```

CPU and memory remain separate dimensions. External machine-readable IDs use lowercase hyphenated forms such as `cpu8-mem16`; human-facing output uses `CPU8-MEM16`.

## POC0

The first vertical slice is intentionally local and synthetic:

```text
synthetic history
  -> DuckDB
  -> JobShape
  -> RunnerCatalog
  -> deterministic optimizer
  -> Recommendation
  -> explanation
```

Run:

```bash
cargo run -- demo
```

Or persist synthetic history:

```bash
cargo run -- synth --job lint --runs 100
cargo run -- profile lint
cargo run -- recommend lint
cargo run -- explain lint
```

## Real observation

OBSERVE1 adds a Linux-first wrapper mode:

```bash
cargo run -- observe --job smoke -- sh -c 'sleep 0.2'
```

CIShape runs as the parent process, observes the child process tree, writes JSON evidence, stores the normalized run in DuckDB, and preserves the child exit status.

The default telemetry contract intentionally excludes source code, secrets, environment values, and stdout/stderr payload capture.

## Dogfooding

CIShape's own GitHub Actions workflow is its first real CI workload.

After the observer binary is built, CI runs its tests, Clippy, and demo through `cishape observe`. Each workflow run uploads normalized JSON observations plus the local DuckDB history as a short-lived GitHub Actions artifact.

This keeps the first real dataset completely OSS and backend-free.

## Portable history

Observations from ephemeral runners can be merged locally without CIShape Cloud:

```bash
cargo run -- import artifact-a/*.json artifact-b/*.json
cargo run -- profile test
cargo run -- profile --repository sergii/cishape test
cargo run -- export --format jsonl --output history.jsonl
```

Imports are idempotent, and portable CI identity can correlate repository, workflow, run, job, commit, and ref across artifacts. If the same logical job name exists in multiple repositories, profiling requires an explicit `--repository` scope instead of silently mixing workloads.

A human-readable history summary is available with:

```bash
cargo run -- report --format markdown
```

The repository also dogfoods rolling history in `.github/workflows/history.yml`: it downloads non-expired main-branch observation artifacts, imports their JSON evidence, exports canonical JSONL, renders a Markdown report, and uploads the resulting history bundle. No CIShape backend is required.

## Decisions

The primary decision path is deterministic and requires no API key or network service:

```bash
cargo run -- decide \
  --repository sergii/cishape \
  test
```

CIShape profiles the historical workload, filters infeasible runner shapes, chooses the deterministic baseline, and persists a provider-neutral DecisionRecord.

For all workload scopes at once, generate a read-only advisory:

```bash
cargo run -- advisory \
  --repository sergii/cishape \
  --min-runs 10 \
  --format markdown
```

Low-history workloads are explicitly marked `insufficient_evidence`. Actionable rows are classified as `keep`, `downsize`, `upsize`, or `reshape`.

Jev remains an optional shadow experiment. CIShape can prepare a bounded Jev decision request without giving the decision provider control over feasibility or execution:

```bash
cargo run -- decision-prepare \
  --repository sergii/cishape \
  --output .cishape/decisions/test-request.json \
  test
```

The request contains only deterministic feasible runner candidates. A recorded Jev response can be validated and persisted with `decision-record`, while any choice outside the feasible set fails closed.

With an explicit server-side key, the same prepared request can be sent live:

```bash
export JEV_API_KEY=...
cargo run -- decision-run \
  --request .cishape/decisions/test-request.json
```

The live adapter uses the official TypeSafe System One endpoint by default, stores the raw response as evidence, and then runs the same fail-closed JEV1 validation. Shadow decisions have no CI execution side effects.

For a reproducible first live proof, the repository also contains a manual-only `Jev Shadow Proof` GitHub Actions workflow. It consumes the latest rolling CIShape history artifact and requires only a repository secret named `JEV_API_KEY`.

## Direction

Jev is not on the critical path. The next product slices focus on deterministic advisory output, real provider catalogs/pricing, outcome evaluation, and richer queue/cost context. OpenTelemetry/Parquet interchange remains a follow-up.

CIShape is not a CI control plane. GitHub Actions, Buildkite, GitLab, Jenkins, and other systems remain responsible for execution. CIShape observes, models, recommends, and later may provide routing decisions.

See:

- [Architecture](docs/architecture.md)
- [RFC-0001](docs/rfc/0001-poc-and-core-model.md)
- [Roadmap](docs/roadmap.md)
- [CI strategy](docs/ci.md)
- [Telemetry model v0](docs/telemetry-model-v0.md)
- [Portable history](docs/portable-history.md)
- [Decision providers](docs/decision-providers.md)
- [Deterministic advisory](docs/advisory.md)
- [Future HTTP boundary](docs/contracts/http-boundary-v0.md)

## License

License is not selected yet. Do not treat the repository as licensed for redistribution until a license is added.
