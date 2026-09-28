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

## Direction

Next slices replace synthetic history with real process observation and then dogfood CIShape in this repository's own GitHub Actions.

CIShape is not a CI control plane. GitHub Actions, Buildkite, GitLab, Jenkins, and other systems remain responsible for execution. CIShape observes, models, recommends, and later may provide routing decisions.

See:

- [Architecture](docs/architecture.md)
- [RFC-0001](docs/rfc/0001-poc-and-core-model.md)
- [Roadmap](docs/roadmap.md)
- [Future HTTP boundary](docs/contracts/http-boundary-v0.md)

## License

License is not selected yet. Do not treat the repository as licensed for redistribution until a license is added.
