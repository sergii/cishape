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
cargo run -- export --format jsonl --output history.jsonl
```

Imports are idempotent, and portable CI identity can correlate repository, workflow, run, job, commit, and ref across artifacts.

## Direction

The next slices extend portable interchange with OpenTelemetry/Parquet adapters and use accumulated history for richer recommendations.

CIShape is not a CI control plane. GitHub Actions, Buildkite, GitLab, Jenkins, and other systems remain responsible for execution. CIShape observes, models, recommends, and later may provide routing decisions.

See:

- [Architecture](docs/architecture.md)
- [RFC-0001](docs/rfc/0001-poc-and-core-model.md)
- [Roadmap](docs/roadmap.md)
- [CI strategy](docs/ci.md)
- [Telemetry model v0](docs/telemetry-model-v0.md)
- [Portable history](docs/portable-history.md)
- [Future HTTP boundary](docs/contracts/http-boundary-v0.md)

## License

License is not selected yet. Do not treat the repository as licensed for redistribution until a license is added.
