# Economics-aware advisory

CAPACITY7 connects CIShape's historical workload model to current provider capacity evidence.

The pipeline is:

```text
RunObservation history
-> JobShape
-> OptimizationPolicy
-> canonical target RunnerShape + predicted p95
+ ProviderCatalog
+ CapacitySnapshot
+ EconomicsPolicy
-> provider/offer advisory
```

This removes the manual `--cpu`, `--memory-gib`, and `--duration-ms` step when evaluating known CI jobs.

## Evidence gate

Provider selection is not attempted until the workload has at least `OptimizationPolicy.min_runs` observations.

Each workload is classified as:

- `insufficient_evidence` - historical sample is below the policy threshold
- `no_runner_candidate` - the deterministic shape optimizer has no safe canonical candidate
- `no_eligible_offer` - capacity was evaluated, but no offer survived catalog fit and EconomicsPolicy constraints
- `selected` - a deterministic provider/offer selection exists

The target shape and predicted runtime come from the existing deterministic recommendation path. CAPACITY7 does not introduce a second sizing algorithm.

## CLI

```bash
cargo run -- economics-advisory \
  --db .cishape/cishape.duckdb \
  --repository owner/repo \
  --snapshot .cishape/capacity/live.json \
  --optimization-policy policies/default-v1.json \
  --economics-policy policies/economics-default-v1.json \
  --format markdown
```

JSON retains the complete per-job EconomicsReport and EconomicsSelection evidence. Markdown provides the actionable summary.

## Boundary

The command is advisory-only. It does not rewrite workflow YAML, dispatch work, or route jobs.

A later CI integration can collect a live multi-pool snapshot, run this advisory against rolling history, and publish the evidence as an artifact or job summary.
