# Provider catalogs

CIShape keeps canonical workload shape separate from provider-specific execution products.

A canonical recommendation such as:

```text
CPU2-MEM4
```

means the workload needs a capacity envelope. It does not mean a provider must sell a product with that exact marketing shape.

The provider catalog maps the canonical target onto real execution offers.

## Snapshot

The checked-in catalog is:

```text
catalogs/providers-v1.json
```

It is a dated offline snapshot, not a live pricing API.

Every offer records:

- provider
- provider offer ID
- optional CI runner label
- optional repository visibility context when the same runner label maps to different capacity/commercial semantics
- CPU and memory when verified
- OS and architecture
- execution model
- pricing model
- billing increment when verified
- source URL
- observation date
- optional region/notes

## CLI

Inspect the catalog:

```bash
cishape catalog
```

Fit a canonical target to comparable managed offers:

```bash
cishape catalog-fit \
  --cpu 2 \
  --memory-gib 4 \
  --duration-ms 11000
```

When an offer is repository-context specific, pass that context explicitly:

```bash
cishape catalog-fit \
  --cpu 4 \
  --memory-gib 8 \
  --duration-ms 11000 \
  --repository-visibility public
```

Context-specific offers are excluded when visibility is omitted. CIShape does not infer commercial/capacity context from a runner label alone.

CATALOG1 ranks only offers that are safe to compare:

```text
managed ephemeral
+ complete CPU/RAM
+ known per-minute price
+ known billing increment
```

An offer can satisfy a smaller canonical target. For example, a published CPU2-MEM8 offer can fit a CPU2-MEM4 target.

## Incomplete data is valid

Provider data is often incomplete or account-specific.

CIShape does not invent missing facts.

An offer with unknown memory, billing increment, or price remains visible in the catalog but is not selected by `catalog-fit`.

This is intentional. Provider coverage and optimization safety are separate concerns.

## Context-specific runner labels

A CI runner label is an adapter alias, not a globally unique machine shape.

GitHub currently documents `ubuntu-latest` differently by repository visibility:

- public repository standard Ubuntu: 4 CPU / 16 GB and no standard-runner usage charge
- private repository standard Ubuntu: 2 CPU / 8 GB, using included minutes and then the published Linux minute rate

CIShape therefore stores these as different offer IDs even though both use the `ubuntu-latest` workflow label:

```text
github-actions/ubuntu-latest-public-x64
github-actions/ubuntu-latest-private-x64
```

A live capacity plan must choose the offer ID that matches its repository context. The label is used to observe jobs; the offer ID identifies the economics/capacity contract.

## Pricing models

### Per-job managed runners

A per-minute runner may have a billing increment.

For a workload lasting 11 seconds:

```text
$0.006/min + 60-second increment -> billed 60 seconds
$0.006/min + 1-second increment  -> billed 11 seconds
```

CATALOG1 includes that rounding in the estimated cost.

Plan subscriptions, included minutes, negotiated discounts, taxes, and organization-specific credits are not allocated into per-job cost yet.

### Fixed self-hosted servers

A persistent VM such as a Hetzner server has different economics:

```text
fixed capacity
+ hourly/monthly price
+ utilization
+ queueing
+ operator overhead
```

It is therefore represented in the catalog but not ranked directly against ephemeral per-job runners by `catalog-fit`.

CAPACITY1 adds a separate economics evaluator. It may compare fixed servers with managed offers only when a runtime capacity snapshot supplies explicit utilization, parallel-slot, queue, turnover, and cache inputs.

## Initial source snapshot

The 2026-09-29 snapshot contains:

- GitHub Actions managed Linux x64 offers, including distinct public/private standard Ubuntu contexts
- Depot managed Linux x64 offers
- Blacksmith partial public pricing data
- Namespace as a configurable managed provider with intentionally incomplete public price/shape data
- Hetzner CX33 as a fixed self-hosted VM example

The source URLs are stored alongside each catalog record so a future refresh can be reviewed as data provenance.

## Boundary

Provider catalogs are adapters.

The core remains:

```text
RunObservation
  -> JobShape
  -> canonical RunnerShape
  -> provider catalog fit
```

Provider names and runner labels must not leak into the core workload model.
