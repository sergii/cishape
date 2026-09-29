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
  --policy policies/default-v1.json \
  --format markdown
```

Low-history workloads are explicitly marked `insufficient_evidence`. Actionable rows are classified as `keep`, `downsize`, `upsize`, or `reshape`.

The safety factors, evidence threshold, latency penalty, optional p95 guard, and deterministic objective live in a versioned optimization policy instead of hidden constants.

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

## Provider catalog

CIShape keeps canonical resource recommendations separate from provider products. A dated offline catalog can map a target shape onto comparable managed offers:

```bash
cargo run -- catalog

cargo run -- catalog-fit \
  --cpu 2 \
  --memory-gib 4 \
  --duration-ms 11000 \
  --repository-visibility public
```

Provider records carry pricing provenance and billing semantics. Missing RAM, price, or billing increments are kept as unknown rather than fabricated. CATALOG1 still keeps fixed self-hosted VM pricing separate from ephemeral per-job fitting.

CATALOG2 makes repository context explicit where a provider reuses one runner label for different products. For GitHub standard `ubuntu-latest`, public and private repositories are separate offers. Use `catalog-fit --repository-visibility public|private` when evaluating those contextual offers; omitting visibility excludes them instead of guessing.

CATALOG3 carries the same context through live capacity evidence. The GitHub adapter records the repository's actual public/private visibility in `CapacitySnapshot`, and economics rejects a contextual offer when the live context is missing or does not match. Runner labels and offer IDs are never used to guess visibility.

## Capacity economics

CAPACITY1 combines static provider offers with a time-scoped runtime capacity snapshot:

```bash
cargo run -- economics \
  --snapshot examples/capacity-snapshot-v1.json \
  --policy policies/economics-default-v1.json \
  --cpu 2 \
  --memory-gib 4 \
  --duration-ms 11000 \
  --format markdown
```

The evaluator models queue depth, busy/parallel slots, slot turnover, warm/cold cache state, billing granularity, and fixed-server utilization. It emits effective cost and time-to-green as separate dimensions and marks Pareto-optimal options without hiding an arbitrary weighted score. CAPACITY2 applies a versioned economics policy such as "minimize effective cost while time-to-green stays under 30 seconds" and records the deterministic provider/offer selection. CAPACITY3 adds `--jobs N` so the same model can price and schedule a batch of parallel CI jobs.

CAPACITY4 extends the same evidence model to heterogeneous workflow DAGs:

```bash
cargo run -- workflow-economics \
  --workflow examples/workflow-demand-v1.json \
  --snapshot examples/workflow-capacity-snapshot-v1.json \
  --policy policies/economics-default-v1.json
```

Each workflow job can have its own canonical RunnerShape, predicted runtime, and dependency list. CIShape validates the DAG, schedules fan-out/fan-in work against explicit provider slots and queue-ahead state, and applies EconomicsPolicy to total workflow cost and time-to-green. It remains advisory-only and does not place individual jobs across providers.

CAPACITY5 adds the first live provider adapter. GitHub Actions queued/running jobs can be normalized into the same CapacitySnapshot contract:

```bash
cargo run -- capacity-github \
  --repository owner/repo \
  --provider github-actions \
  --offer-id ubuntu-latest-private-x64 \
  --runner-label ubuntu-latest \
  --parallel-slots 20 \
  --slot-turnover-ms 12000 \
  --cache-state warm \
  --cache-penalty-ms 0 \
  --output .cishape/capacity/github-actions.json
```

Queue depth and running-job counts come from the GitHub Actions API. The runner provider is explicit because GitHub Actions can execute GitHub-hosted, Depot, or self-hosted jobs. Concurrency capacity, turnover, and cache behavior stay explicit inputs when the control-plane API does not expose those facts for the selected pool.

CAPACITY6 adds a versioned multi-pool plan so one live observation can compare several runner providers/offers:

```bash
cargo run -- capacity-github-plan \
  --config examples/github-capacity-plan-v1.json \
  --repository owner/repo \
  --output .cishape/capacity/live.json
```

Pool selectors require all configured runner labels and ambiguous matches fail closed. The example plan contains illustrative capacity values that must be replaced with evidence for the actual account/pools.

CAPACITY7 connects that live evidence to historical JobShape profiles:

```bash
cargo run -- economics-advisory \
  --db .cishape/cishape.duckdb \
  --repository owner/repo \
  --snapshot .cishape/capacity/live.json \
  --optimization-policy policies/default-v1.json \
  --economics-policy policies/economics-default-v1.json \
  --format markdown
```

For each workload scope, CIShape first enforces the historical evidence threshold, derives the same canonical safe target used by deterministic advisory, then evaluates current provider capacity and applies EconomicsPolicy. JSON keeps the complete economics and exclusion evidence.

CAPACITY8 wires this chain into the existing `CI Advisory` GitHub Actions workflow without treating example capacity as real. Deterministic advisory still runs on every eligible advisory workflow. Economics-aware advisory activates only when a real capacity plan is explicitly supplied to a manual run or checked in at `config/cishape/github-capacity-plan.json`. Otherwise it is visibly skipped.

When enabled, the workflow collects live queue/running state, writes `capacity.json`, generates Markdown plus full JSON economics evidence, appends the Markdown to the GitHub job summary, and uploads all advisory evidence together. Execution remains read-only.


CAPACITY9 makes concurrency evidence scope explicit. The current GitHub live adapter scans one repository, so its states are tagged `repository:<owner/repo>`. Standard GitHub-hosted runner offers require `provider_account` scope; repository-only queue counts are therefore rejected for those offers instead of being combined with account-wide concurrency limits. A future account/org-wide observer must supply scope-complete evidence for that live comparison.


CAPACITY10 provides that scope-complete path for personal GitHub accounts. `capacity-github-account` proves it can enumerate the authenticated user's complete owned-repository inventory, aggregates active GitHub-hosted jobs across every owned repository, and emits `provider_account:github:user:<login>` evidence. It fails closed on partial repository visibility and still requires the actual concurrency limit as an explicit input.

## Outcomes

A deterministic DecisionRecord can later be compared with matching post-decision observations:

```bash
cargo run -- outcome \
  --decision .cishape/decisions/test.json \
  --policy policies/default-v1.json \
  --format markdown
```

CIShape distinguishes `not_applied`, `insufficient_evidence`, `within_prediction`, `outside_prediction`, and `mixed_failures`. The evaluation is observational and does not claim that a runner change caused a workload failure or latency change.

## Direction

Jev is not on the critical path. Capacity economics, workflow scheduling, live multi-pool capacity evidence, and economics-aware advisory are deterministic and local-first. The complete evidence chain is now wired into CI as an explicit-evidence opt-in. OpenTelemetry/Parquet interchange and broader provider discovery remain follow-ups.

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
- [Provider catalogs](docs/provider-catalogs.md)
- [Capacity economics](docs/capacity-economics.md)
- [Economics policy](docs/economics-policy.md)
- [Parallel-job batch economics](docs/batch-economics.md)
- [Workflow DAG economics](docs/workflow-economics.md)
- [Capacity adapters](docs/capacity-adapters.md)
- [Economics-aware advisory](docs/economics-advisory.md)
- [Optimization policy](docs/optimization-policy.md)
- [Decision outcomes](docs/outcomes.md)
- [Future HTTP boundary](docs/contracts/http-boundary-v0.md)

## License

License is not selected yet. Do not treat the repository as licensed for redistribution until a license is added.
