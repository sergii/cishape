# Continuous CI Profiler & Advisor - insight snapshot

Date: 2026-09-29

Status: **canonical product/architecture insight snapshot**. This preserves the current direction discussed after the v0.1.2 release. It is not, by itself, an RFC or implementation commitment.

## Product direction

CIShape should evolve from a one-shot runner-size recommender into an **open, vendor-neutral, continuous CI profiler and advisor**.

Working category description:

> CIShape - Continuous CI Profiler & Advisor.

The CIShape brand remains useful. "CI Profiler" describes the central subsystem/category; it is not currently a decision to rename the product.

The intended value loop is:

```text
CI workload
  -> observe every run
  -> history
  -> workload profile
  -> drift detection
  -> recommendation
  -> explicit human action
  -> notification only when something materially changes
```

Runner sizing is one action family, not the whole product.

## Current implementation baseline

As of v0.1.2, CIShape already has:

- black-box command observation;
- duration, CPU, memory, process I/O, exit status;
- local DuckDB history plus portable JSON/JSONL;
- historical JobShape percentiles;
- deterministic RunnerShape recommendations;
- advisory actions such as keep/downsize/upsize/reshape;
- provider catalogs and a separate provider-economics layer;
- release-package acceptance testing.

Important current gaps:

- `RunObservation` does not yet model network telemetry;
- process `read_bytes` / `write_bytes` are process I/O evidence, not network evidence;
- workload semantics such as test/build/lint/render/ingest are not first-class;
- the CLI still renders RunnerShape human display IDs in uppercase;
- `Recommendation` itself is still shape-centric and does not yet expose the full human-action model described below;
- drift/state transitions and notification policy are not yet first-class.

## RunnerShape identity - one lowercase form

Direction:

Use the stable lowercase identity everywhere humans and machines can reasonably share it:

```text
cpu1-mem2
cpu2-mem4
cpu8-mem16
cpu10-mem15
```

Avoid maintaining two visible representations such as:

```text
machine: cpu2-mem4
display: CPU2-MEM4
```

The lowercase form reads cleanly in CLI, JSON, YAML, logs, tables, links, and APIs.

Current implementation note:

v0.1.2 still exposes an uppercase presentation form such as `CPU2-MEM4`. Treat migration to lowercase presentation as a future compatibility/UX slice, not as already shipped behavior.

Candidate key: **DISPLAY1**.

## Recommendation should tell the human what to do

A recommendation should not require the user to mentally diff two shapes.

Minimum output model:

```text
current
recommended
action
evidence
reason
```

Example:

```text
Recommendation

  action            downsize

  current           cpu10-mem15
  recommended       cpu1-mem2

  CPU               10 -> 1 cores
  memory            15 -> 2 GiB

  evidence           31 runs
  CPU p95            0.20 cores
  memory p99         6 MiB
  safety margin      1.5x
```

### Overall action vocabulary

The existing advisory vocabulary is a good basis:

- `keep`
- `downsize`
- `upsize`
- `reshape`
- `observe` / `insufficient_evidence` when evidence is not yet actionable

Do not reduce the model to only "upgrade" vs "downgrade". CPU and memory are independent dimensions.

For example:

```text
current       cpu4-mem16
recommended   cpu8-mem8
```

is a **reshape**, not a simple upgrade or downgrade.

### Per-resource direction

Recommendation evidence should also expose direction per dimension:

```text
cpu_change:     increase | decrease | keep
memory_change:  increase | decrease | keep
```

Future dimensions may add their own actions instead of being collapsed into one scalar "size".

Examples:

```text
action: increase_cpu
action: increase_memory
action: increase_cpu_and_memory
action: decrease_cpu
action: decrease_memory
action: reshape
action: investigate_network
action: investigate_disk
action: investigate_cache
```

The machine-readable contract and the human-facing wording may differ, but the underlying action should be explicit.

Candidate key: **ACTION1**.

## Shape recommendation and provider economics are separate layers

Core questions:

1. **Shape** - what capacity does the workload need?
2. **Economics** - what does that capacity cost on a concrete provider/offer/account/context?

Desired separation:

```text
Profiler
  -> observed demand
  -> safe required shape
  -> action
  -> canonical RunnerShape

canonical RunnerShape
  -> ProviderCatalog / live capacity
  -> provider offer
  -> price / billing / queue / locality
```

The base recommendation should not present a dollar value as though it were provider-backed evidence when it comes only from an internal canonical model.

Real monetary claims should carry provider identity, pricing provenance/date, billing semantics, and required account/repository context.

Related issue: **#80 CORECOST1**.

Open product question:

Some economics capabilities may eventually be premium/cloud features, but the architecture must not distort evidence semantics merely to create a paywall. Local OSS sizing should remain useful without cloud signup.

## Continuous profiling, not a snapshot tool

Measurement should normally happen **on every CI run**, because workload behavior changes as tests, dependencies, code, assets, build graphs, and data volumes change.

A nightly job is better suited to aggregation/evaluation/notification than to being the only measurement point.

Preferred model:

```text
Every CI run
------------
workload
  -> observer
  -> RunObservation
  -> history

Periodic / every N runs / event driven
--------------------------------------
history
  -> profile
  -> compare with prior profile/state
  -> recommendation
  -> drift/state transition
  -> notify only when actionable
```

This supports the real lifecycle:

```text
today:
  configured      cpu8-mem16
  required        cpu2-mem4
  action          downsize

later:
  configured      cpu2-mem4
  required        cpu2-mem4
  action          keep

later:
  configured      cpu2-mem4
  required        cpu4-mem4
  action          increase_cpu
```

Candidate key: **PROFILE1**.

## Drift is a first-class product capability

The profiler should model not only a current recommendation but whether the workload has **changed materially over time**.

Useful state vocabulary:

- `healthy`
- `overprovisioned`
- `underprovisioned`
- `approaching_limit`
- `insufficient_evidence`
- `pending_confirmation`
- `changed`

State and action are separate concepts.

Example:

```text
state             approaching_limit
action            observe

then, after sustained evidence:

state             underprovisioned
action            increase_cpu
```

### Avoid flapping

Do not alternate recommendations on individual noisy runs.

Candidate mechanisms:

- recent rolling window, for example last 20-50 successful runs;
- longer comparison window, for example 7/30 days;
- percentile-based CPU/memory/duration evidence;
- hysteresis;
- require a direction to persist across several evaluations;
- immediate alert only for hard safety breaches.

Exact window sizes and confirmation counts remain open policy questions.

Reference concept: continuous benchmarking/regression tools such as Bencher demonstrate historical thresholds and alerting against prior metrics.

Candidate key: **DRIFT1**.

## Notify on state transitions, not with noisy daily reports

A daily "everything is fine" email is low-value noise.

Preferred behavior:

```text
observe continuously
  -> evaluate continuously/periodically
  -> notify when state/action changes materially
```

Example notification:

```text
CIShape detected a sizing change

workload        test
current         cpu2-mem4
recommended     cpu4-mem4
action          increase_cpu

CPU p95 increased from 1.1 -> 2.4 cores
Memory remains within target

Evidence: 31 runs / 6 days
```

Potential delivery channels:

- email;
- GitHub check/PR comment/job summary;
- Slack/ChatOps;
- webhook/API;
- cloud UI inbox.

Channel choice and cadence are not yet fixed.

Candidate key: **NOTIFY1**.

## Workload semantics and resource behavior are independent dimensions

CIShape should distinguish:

1. **where the workload executes**;
2. **what the workload is intended to do**;
3. **which tools/runtime perform it**;
4. **which inputs/outputs it touches**;
5. **what resource behavior was actually observed**.

Do not collapse these into one `WorkloadType` enum.

Conceptual model:

```text
Workload
├── execution     where it runs
├── semantics     what it is intended to do
├── toolchain     how it is performed
├── inputs        what it consumes
├── outputs       what it produces
└── observed      resource behavior
```

Examples of orthogonal concepts:

- GitHub Actions / Buildkite: CI control planes;
- Nix: build/environment/reproducibility system;
- BuildKit: build engine;
- GHCR: artifact/container registry;
- S3: object-storage input/output source.

A single job may combine several activities:

```text
build + package + publish
fetch + validate + generate
test + coverage + upload
```

Therefore prefer facets over a single hierarchy.

Candidate key: **SEMANTICS1**.

## Declared/detected semantics must be separate from observed behavior

CIShape may observe:

```text
CPU high
memory low
network high
```

That is evidence for a resource signature. It is **not sufficient evidence** to claim "this is compilation" or "this is S3 parsing".

Use two layers.

### Semantic context

Potential activities:

- test
- lint
- validate
- build
- compile
- package
- publish
- release
- analyze
- render
- convert
- ingest
- deploy
- benchmark

Potential domain/tool/input/output facets may include:

- language/runtime;
- native extension;
- container image;
- CAD;
- PDF;
- image/media;
- object storage;
- package registry;
- container registry.

Semantic data may be:

- explicitly declared by the user/project;
- detected from a trustworthy adapter;
- unknown.

Its provenance should remain visible.

### Observed resource signature

Derived only from telemetry, for example:

- CPU-bound;
- memory-bound;
- disk-read-bound;
- disk-write-bound;
- network-bound;
- wait-bound;
- mixed;
- unknown.

Never infer business/workload semantics from resource shape alone.

## Optional workload declaration

CIShape should continue to work as a black box, but an optional project declaration can add semantics:

```yaml
workloads:
  test:
    activities: [test]
    domain: ruby

  docker-build:
    activities: [build, package]
    artifact: container-image
    builder: buildkit
    destination: ghcr

  cad-check:
    activities: [validate]
    domain: cad

  reports:
    activities: [generate]
    domain: pdf
```

If no semantics are declared:

```text
semantics      unknown
observed       cpu+network mixed
```

The profiler remains useful.

## Resource profile should grow beyond CPU and memory

Long-term workload profiling should treat resources as independent dimensions.

Candidate profile:

```yaml
profile:
  duration:
    p95: 42s

  cpu:
    p95: 2.7 cores

  memory:
    p99: 3.1 GiB

  disk:
    read_p95: 420 MiB
    write_p95: 90 MiB

  network:
    rx_p95: 1.8 GiB
    tx_p95: 12 MiB

  resource_signature:
    primary: network
    secondary: cpu
```

This enables recommendations such as:

```text
action          keep_shape
network         investigate

reason:
  CPU and memory have sufficient headroom
  runtime correlates with sustained network transfer
```

instead of incorrectly recommending more CPU.

Future RunnerCapabilities may extend beyond CPU/RAM:

- disk/IOPS;
- network bandwidth;
- architecture;
- accelerator/GPU;
- cache locality;
- isolation/runtime capabilities.

Candidate key: **RESOURCE1**.

## Network attribution is a future telemetry slice

Current v0.1.2 observation uses Linux process accounting and `/proc` process-tree data for CPU, memory and I/O.

Future Linux observation should investigate stronger workload attribution using:

- cgroup v2;
- PSI where useful;
- cgroup-aware network accounting;
- eBPF/cgroup hooks where allowed;
- lower-level collectors only when they preserve the evidence boundary.

Do not silently count host-wide network traffic as if it belonged to one workload.

cAdvisor is useful prior art for CPU/memory/disk/network container telemetry, but CIShape still needs job/workload attribution and CI-specific history/action semantics.

## Continuous architecture direction

The emerging architecture is:

```text
Observer
   |
   v
History
   |
   v
Profiler
   |
   v
Drift Detector
   |
   v
Advisor
   |
   v
Recommendation
   |
   v
Notifier

Profiler/Advisor
   |
   +--> Economics
   |
   +--> future provider/execution planning
```

The existing CI control plane remains responsible for execution.

Related issue: **#81 EXECUTION1** defines a future provider-neutral advisory ExecutionPlan without making CIShape a control plane.

## Market synchronization - 2026-09-29

The market validates several parts of this direction, but the capabilities are usually bundled into a specific runner/build ecosystem rather than exposed as a vendor-neutral profiler.

### Depot

Depot documents GitHub Actions analytics with CPU/memory utilization trends and automated size-up / size-down recommendations derived from historical job performance.

References:

- https://depot.dev/docs/github-actions/observability/github-actions-metrics
- https://depot.dev/docs/api/github-actions-api

Product lesson:

Right-sizing recommendations based on historical utilization are already a paid-value feature.

### WarpBuild

WarpBuild documents per-runner/job observability including CPU, memory, filesystem/disk I/O, network utilization, logs and right-sizing recommendations.

References:

- https://www.warpbuild.com/docs/ci/features/observability
- https://www.warpbuild.com/solutions/github-actions-observability

Product lesson:

A multi-dimensional resource profile plus explicit right-sizing action is already useful in managed-runner products.

### Bencher

Bencher demonstrates continuous historical performance comparison, thresholds, statistical tests and alerts rather than treating every point as a new action.

References:

- https://bencher.dev/docs/explanation/benchmarking/
- https://bencher.dev/docs/explanation/thresholds/

Product lesson:

Drift detection needs history, thresholds/models and anti-noise behavior.

### OpenTelemetry

OpenTelemetry has CI/CD semantic conventions for pipeline runs, task runs, metrics and logs.

References:

- https://opentelemetry.io/docs/specs/semconv/cicd/
- https://opentelemetry.io/docs/specs/semconv/cicd/cicd-spans/

Architecture lesson:

Reuse common CI/CD vocabulary where it fits instead of inventing equivalent names.

### cAdvisor

cAdvisor is prior art for container resource telemetry including CPU, memory, filesystem/disk and network statistics.

Reference:

- https://github.com/google/cadvisor

Architecture lesson:

The low-level resource metrics are not the moat by themselves. CIShape's differentiated layer is workload attribution, longitudinal profiling, drift, explicit human action, vendor neutrality and separation from provider economics.

### Other reference systems to keep in view

The broader market scan also identified build/test-specific systems worth revisiting during RFC work:

- Gradle Develocity - deep build/task/resource semantics;
- BuildBuddy - execution/action history and resource-aware build scheduling;
- StepSecurity Harden-Runner - black-box process/file/network visibility for CI security;
- Buildkite Test Engine - historical test performance and optimization;
- GitHub/GitLab/CircleCI native analytics - baseline pipeline/job duration and reliability analytics.

These references should be re-verified when used for a concrete RFC or competitive claim.

## Product differentiation hypothesis

CPU/RAM charts alone are not differentiated; managed runner vendors already provide them.

The stronger CIShape hypothesis is the combination:

```text
vendor-neutral
+ local-first OSS
+ arbitrary black-box CI workload
+ optional semantic context
+ multi-dimensional resource profile
+ longitudinal history
+ drift/state transitions
+ explicit human action
+ provider-independent canonical shape
+ provider economics as a separate evidence layer
```

This is the product direction to validate.

## Explicit boundaries

Preserve these boundaries:

- CIShape observes, profiles and advises; it is not a CI control plane.
- Workload semantics are not inferred from job names or resource usage without evidence.
- Provider labels are aliases, not canonical capacity types.
- CPU and memory stay independent dimensions.
- Missing provider capacity/pricing/network evidence stays unknown.
- Core sizing and provider economics remain separate.
- A recommendation should be reproducible from evidence, policy and algorithm version.
- Optional AI/decision providers never widen the deterministic feasible set.
- Continuous notification should surface material state/action changes, not create daily noise.

## Candidate implementation slices

These names are intentionally provisional but provide a stable handoff vocabulary:

- **DISPLAY1** - make lowercase `cpuN-memN` the single visible RunnerShape identity;
- **ACTION1** - add explicit overall and per-resource actions to Recommendation;
- **PROFILE1** - formalize continuous workload profiling from every CI run;
- **DRIFT1** - rolling-window drift/state detection with hysteresis;
- **SEMANTICS1** - versioned workload semantic facets with provenance;
- **RESOURCE1** - extend workload resource profile toward disk/network and resource signatures;
- **NOTIFY1** - state-transition/action-change notification contract;
- **CORECOST1** - separate canonical sizing objective from provider-backed monetary economics (#80);
- **EXECUTION1** - derive an advisory provider-neutral ExecutionPlan without executing CI (#81).

These slices should become RFCs/issues only when we choose to implement them. The insight snapshot is the canonical source for the reasoning that connects them.
