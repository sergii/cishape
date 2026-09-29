# CIShape v0.1 quickstart

CIShape can be useful locally without a CIShape account, SaaS backend, Jev credential, or GitHub account-wide token.

The default path is:

```text
real command
  -> RunObservation
  -> local DuckDB history
  -> JobShape
  -> deterministic recommendation/advisory
```

## 1. Build or install from the repository

CIShape currently ships from source. No binary release channel is claimed yet.

Build in place:

```bash
cargo build --release
./target/release/cishape --help
```

Or install the current checkout into Cargo's binary directory:

```bash
cargo install --path .
cishape --help
```

Requirements:

- the Rust toolchain pinned by `rust-toolchain.toml`
- a C/C++ build toolchain for bundled DuckDB

Resource observation is Linux-first. The portable history, profiling, advisory, decision, and economics layers are platform-neutral once observations exist.

## 2. Observe one real workload

Wrap a command that represents one logical CI job:

```bash
cishape observe --job test -- cargo test
```

By default CIShape writes normalized history to:

```text
.cishape/cishape.duckdb
```

The observer does not capture source code, secrets, environment values, or stdout/stderr payloads as telemetry.

Use a stable `--job` name for repeated runs of the same logical workload.

## 3. Inspect the evidence immediately

One observation is already inspectable:

```bash
cishape report
cishape profile test
```

The profile summarizes duration, CPU, memory, and the observed runner shape from the evidence currently available.

A small number of runs is useful for inspection, but CIShape deliberately does not treat it as enough evidence for an actionable optimization decision.

## 4. Accumulate representative history

The checked-in `default-v1` policy requires 10 observations before a workload becomes actionable.

For a local experiment, repeat the same representative workload:

```bash
for i in {1..10}; do
  cishape observe --job test -- cargo test
done
```

In real CI, do not manufacture repeats just to satisfy the threshold. Let CIShape accumulate normal runs over time.

The threshold is explicit in:

```text
policies/default-v1.json
```

## 5. Get the deterministic recommendation

For one workload:

```bash
cishape recommend test
```

To inspect the safety reasoning:

```bash
cishape explain test
```

For all workloads in the local history:

```bash
cishape advisory --format markdown
```

The advisory can return:

```text
insufficient_evidence
keep
downsize
upsize
reshape
no_candidate
```

This path is local, deterministic, and read-only. It does not modify CI configuration or runner routing.

## 6. Add repository scope when history contains multiple repositories

CIShape refuses to silently mix workloads with the same job name from different repositories.

Use an explicit scope:

```bash
cishape profile --repository owner/repo test

cishape advisory \
  --repository owner/repo \
  --format markdown
```

GitHub Actions dogfooding already records repository identity into the rolling history used by this repository.

## 7. Optional: Jev shadow decision

Jev is not required for the default workflow.

After deterministic feasibility has already produced a bounded candidate set:

```bash
cishape decision-prepare \
  --repository owner/repo \
  --output .cishape/decisions/test-request.json \
  test
```

With a Jev API key:

```bash
export JEV_API_KEY=...

cishape decision-run \
  --request .cishape/decisions/test-request.json
```

Jev cannot select a runner outside CIShape's deterministic feasible set, and shadow mode has no execution side effects.

## 8. Optional: provider and capacity economics

Provider offer mapping and live capacity are a separate evidence layer.

Start with the checked-in catalog:

```bash
cishape catalog

cishape catalog-fit \
  --cpu 2 \
  --memory-gib 4 \
  --duration-ms 11000 \
  --repository-visibility public
```

Live capacity economics requires explicit provider evidence. CIShape does not substitute example capacity as though it were real.

Account-wide GitHub capacity is optional advanced functionality. CAPACITY10 intentionally fails closed when the credential cannot prove complete account repository visibility. The auth-model research is tracked separately and does not block the deterministic v0.1 workflow.

## What to read next

- [Deterministic advisory](advisory.md)
- [Optimization policy](optimization-policy.md)
- [Portable history](portable-history.md)
- [Decision providers](decision-providers.md)
- [Provider catalogs](provider-catalogs.md)
- [Capacity adapters](capacity-adapters.md)
- [Capacity economics](capacity-economics.md)
