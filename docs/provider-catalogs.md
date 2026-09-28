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

The 2026-09-28 snapshot contains:

- GitHub Actions managed Linux x64 offers
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
