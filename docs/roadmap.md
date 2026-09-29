# Roadmap

CIShape advances through narrow vertical slices.

## VS0 - Synthetic proof

Status: Done.

Goal:

```text
synthetic runs -> profile -> fit -> explain
```

Deliverables:

- Rust CLI
- DuckDB history
- JobShape
- RunnerShape
- deterministic optimizer
- `cishape demo`
- GitHub Actions CI

## VS1 - Real local observation

Status: Done.

Goal:

```text
cishape observe -- <command>
```

Collect:

- wall duration
- process CPU time
- process-tree CPU where available
- peak RSS
- read/write bytes
- exit status
- host/limit RunnerShape

Linux-first implementation is merged.

## VS2 - Dogfood GitHub Actions

Status: Done.

Run CIShape around its own:

- tests
- clippy
- build/demo

Persist normalized run summaries and local DuckDB history as run-scoped workflow artifacts first.

Do not require a SaaS backend.

## VS3 - Portable interchange

Status: Done for JSON/JSONL rolling history. OpenTelemetry and Parquet remain follow-ups.

Add import/export for normalized run history.

Priorities:

- JSON/JSONL
- merge history from multiple workflow runs
- OpenTelemetry mapping
- Parquet
- provider metadata adapters
- rolling GitHub Actions history from main-branch artifacts
- repository-safe historical report

## VS4 - Decision providers

Status: Deterministic provider is the primary path. Jev shadow/live adapters are implemented, and LIVE1 credentialed shadow proof is complete.

Keep deterministic feasibility/ranking.

JEV1:

- expose all deterministic feasible runner candidates
- prepare the official Jev typed-choice request without a network dependency
- reject choices outside the feasible set
- persist DecisionRecord separately from Recommendation/RunObservation
- compare Jev choice with the deterministic baseline in shadow mode

JEV2 adds the explicit live TypeSafe HTTP transport while preserving raw response evidence and the same fail-closed validator.

LIVE1 is complete: the manual GitHub Actions proof consumed 24 real rolling-history runs for `test`, called Jev System One, validated the response, and recorded agreement with the deterministic `cpu8-mem16` baseline at 0.88 confidence. The proof remained shadow-only.

## VS5 - Deterministic advisory

Status: Implemented.

Turn accumulated history into actionable, backend-free suggestions:

- evaluate every workload scope against the deterministic catalog
- classify insufficient_evidence/keep/downsize/upsize/reshape/no_candidate
- emit Markdown/JSON advisory reports
- require minimum evidence before actionable changes
- dogfood the report from rolling GitHub Actions history
- keep execution unchanged

## VS6 - Provider catalogs

Status: CATALOG1 and CATALOG2 implemented for dated offline snapshots, contextual offers, and per-job fitting.

- provider runner aliases
- complete or explicitly incomplete CPU/RAM capacity
- OS/architecture/execution model
- dated price provenance
- billing increments
- fixed-server pricing kept separate from ephemeral per-job pricing

CATALOG2:

- represent repository visibility as explicit offer eligibility when capacity/commercial semantics differ
- model GitHub `ubuntu-latest` public and private standard runners as separate offer IDs
- keep the shared workflow runner label as an alias, not offer identity
- require explicit visibility for `catalog-fit` to include context-specific offers
- encode the current public standard Ubuntu shape and zero runner-usage price without changing private-runner economics

CATALOG3:

- carry optional repository visibility in CapacitySnapshot without breaking generic snapshots
- observe public/private visibility from GitHub repository metadata
- require contextual offers to match the live snapshot context
- fail closed when contextual visibility is missing or mismatched
- preserve repository context in single-job, batch, workflow, and economics-advisory evidence

Next: refresh adapters/API-backed catalogs where providers expose reliable machine/price data.

## VS7 - Policy and outcomes

Status: POLICY1 and OUTCOME1 implemented.

POLICY1:

- versioned optimization policy
- explicit evidence threshold
- CPU/RAM safety factors
- latency penalty and optional p95 guard
- deterministic objective
- policy identity in recommendation/advisory evidence

OUTCOME1:

- DecisionRecord v2 carries baseline/selected RunnerShape and predicted latency
- post-decision observations are matched by repository/job/selected shape
- statuses distinguish not applied, insufficient evidence, within/outside prediction, and mixed failures
- Markdown/JSON outcome evidence remains local and deterministic
- no causal claim is made from temporal correlation alone

Next: surface economics-aware advisory results directly in CI.

## VS8 - Capacity economics

Status: CAPACITY1 through CAPACITY10 implemented.

CAPACITY1:

- keep runtime queue/capacity observations separate from static provider catalog facts
- model queue wait from queue depth, running jobs, parallel slots, and slot turnover
- model warm/cold cache state with an explicit cold-cache penalty
- preserve provider billing increments for managed runners
- allocate persistent fixed-server cost using explicit utilization and parallel-slot capacity
- emit effective runtime, queue wait, billed time, effective cost, and time-to-green
- mark Pareto-optimal offers without collapsing cost and latency into a hidden score
- remain local, deterministic, and read-only

CAPACITY2:

- version economics selection policy as checked data
- support minimize-effective-cost with an optional time-to-green SLA
- support minimize-time-to-green with an optional effective-cost budget
- emit explicit eligibility/exclusion evidence
- preserve deterministic tie-breaking with no hidden weighted score
- keep selection advisory-only with no routing side effects

CAPACITY3:

- preserve the single-job economics path as the default
- add deterministic slot scheduling for N parallel identical jobs
- schedule existing queued work before the new batch
- use observed slot turnover for existing work and predicted runtime for new jobs
- emit first/last job start, batch time-to-green, and total effective cost
- apply EconomicsPolicy constraints and selection to batch totals
- prove parallel job count can change the selected provider offer

CAPACITY4:

- accept a versioned heterogeneous WorkflowDemand DAG
- reject missing dependencies, duplicate IDs, and cycles
- allow each job to carry its own RunnerShape and predicted runtime
- schedule only after dependency completion and slot availability
- account for queue-ahead work before workflow jobs
- emit per-job schedule/cost evidence plus intrinsic critical path and workflow time-to-green
- apply EconomicsPolicy to total workflow cost and time-to-green
- prove provider selection can flip when workflow parallelism is constrained
- remain advisory-only with one provider offer evaluated for the whole workflow

CAPACITY5:

- add a read-only GitHub Actions REST adapter for CapacitySnapshot v1
- enumerate queued and in-progress workflow runs and latest jobs with pagination
- filter jobs by explicit runner label
- observe queue_depth and running_jobs from provider state
- keep parallel_slots, slot_turnover_ms, cache state, and cache penalty explicit when they are not reliably exposed
- fail closed rather than silently truncate the provider's filtered run search cap
- keep tokens out of evidence/output
- preserve CAPACITY1-4 as provider-neutral consumers

CAPACITY6:

- add a versioned multi-pool GitHubCapacityPlan
- observe all configured pools from one active-run/job traversal
- support all-required-label selectors per pool
- keep provider identity independent from the GitHub control plane
- fail closed if an active job matches multiple pools
- emit one time-scoped CapacitySnapshot with all pool states
- preserve the CAPACITY5 single-pool API/CLI

CAPACITY7:

- build a versioned economics advisory over historical workload scopes
- gate provider selection on OptimizationPolicy min_runs
- reuse the deterministic runner recommendation as the canonical target
- feed recommendation predicted p95 into current capacity economics
- apply ProviderCatalog and EconomicsPolicy without provider-specific branches
- distinguish insufficient_evidence, no_runner_candidate, no_eligible_offer, and selected
- preserve full EconomicsReport/EconomicsSelection evidence in JSON
- remain advisory-only

CAPACITY8:

- preserve deterministic advisory as the always-on CI path
- add optional live multi-pool capacity collection to the CI Advisory workflow
- activate economics only from an explicit manual capacity-plan path or the conventional checked-in real-evidence plan
- skip cleanly when no plan exists instead of substituting example/default capacity
- fail closed when a manually requested plan path does not exist
- publish live CapacitySnapshot plus Markdown/JSON economics advisory evidence
- append economics advisory to the GitHub job summary
- preserve read-only permissions and no routing side effects

CAPACITY9:

- add provider-neutral `repository` and `provider_account` concurrency scope kinds
- carry scope kind + concrete key on each CapacityState
- let provider offers require a capacity-scope kind without breaking generic snapshots
- mark the current GitHub scanner as repository-scoped
- require provider-account scope for standard GitHub-hosted runner offers
- skip missing/mismatched scope before queue or workflow scheduling math
- preserve scope evidence in single-job, batch, and workflow reports
- keep account-wide inference out of the repository-only adapter

CAPACITY10:

- add a personal-account GitHub capacity collector
- authenticate the account and bind it to the target repository owner
- prove complete owned-repository visibility before emitting provider-account evidence
- aggregate queued/running GitHub-hosted jobs across every owned repository
- preserve target repository visibility for public/private contextual offers
- emit a concrete `github:user:<login>` provider-account scope
- keep parallel-slot capacity explicit rather than inferring it from GitHub plan defaults
- fail closed on partial inventory or unreadable repository Actions state
- leave organization/enterprise scope for a separate completeness model

CAPACITY11 follow-up is deferred research: determine a least-privilege GitHub authentication model that can prove complete private owned-repository inventory without weakening CAPACITY10 fail-closed semantics. See `docs/research/github-account-capacity-auth.md` and issue #55.

Next: package the v0.1 onboarding/advisory experience. The personal-account capacity proof is no longer a v0.1 blocker.

## V0.1 onboarding

Status: ONBOARD1 complete.

Goal:

```text
clone/install -> observe -> local history -> deterministic advisory
```

The default first-run path must require no SaaS account, Jev credential, or account-wide GitHub token. Optional decision-provider and capacity-economics layers are discovered after the deterministic local workflow is understandable.

ONBOARD1:

- put a concise local-first Quickstart near the top of README
- provide a copy-paste `docs/quickstart.md`
- document source build/install without claiming an unpublished binary channel
- make the 10-run evidence gate explicit
- show the path from observation through report/profile/recommend/explain/advisory
- keep Jev shadow and provider capacity as optional advanced layers

ONBOARD1 is complete. The first-run path is documented and local-first.

## V0.1 release candidate

Status: RELEASE1 complete.

Goal:

```text
main -> locked release build -> smoke -> archive -> checksum -> provenance
```

RELEASE1:

- manual-only Linux x86_64 release-candidate workflow
- derive version from Cargo metadata
- resolve the dependency graph once per candidate run, preserve the generated `Cargo.lock`, and build with `--locked`
- smoke the release binary
- package binary + README + quickstart
- generate SHA-256 checksum and provenance manifest
- upload workflow artifact only
- no tag, GitHub Release, crates.io publish, signing, or deployment
- package the selected project license with the candidate

RELEASE1 proof:

- Release Candidate #2 completed successfully from main commit `520426443708b07c6201f5e5fbe7640cf5262672`
- Linux x86_64 locked release build passed
- release binary smoke passed
- archive/checksum/provenance packaging passed
- workflow artifact uploaded as `cishape-v0.1.0-x86_64-unknown-linux-gnu-rc-36510349471-1`
- artifact digest recorded by GitHub

## V0.1 license

Status: LICENSE1 complete.

Decision:

- license: GNU Affero General Public License v3.0 only
- SPDX identifier: `AGPL-3.0-only`
- Cargo package metadata declares the same identifier
- release archives include the full license text
- contributions are expected under the same project license unless explicitly agreed otherwise

The public-distribution license blocker is removed.

Next after LICENSE1: RELEASE2 - rebuild a licensed release candidate, verify its artifact contents/provenance, then create the `v0.1.0` tag and GitHub Release.



## V0.1 public release

Status: RELEASE2 complete.

Goal:

```text
AGPL-licensed main
-> third-party dependency license audit
-> licensed RC proof
-> v0.1.0 tag
-> GitHub Release
```

RELEASE2:

- pin and checksum-verify cargo-about release tooling
- fail CI on unresolved/unaccepted dependency licenses
- generate and package `THIRD_PARTY_LICENSES.html`
- preserve project `LICENSE`, generated `Cargo.lock`, archive checksum, and provenance
- prove one licensed Linux x86_64 release candidate
- publish only `v0.1.0` from an exact main commit
- attach archive/checksum/provenance to GitHub Release
- do not publish crates.io or claim additional platform support in this slice


RELEASE2 proof:

- public tag: `v0.1.0`
- source commit: `3cdb07420fa1060835b7fd1553cf1b3e57a7e3cc`
- Publish v0.1.0 workflow: success
- dependency-license audit: success
- release payload verification: success
- GitHub Release: published
- attached Linux x86_64 archive, SHA-256 checksum, and provenance manifest
- archive includes `LICENSE`, `THIRD_PARTY_LICENSES.html`, `README.md`, `QUICKSTART.md`, and exact generated `Cargo.lock`
- crates.io publication: intentionally not part of v0.1.0

CIShape v0.1.0 is the completed OSS/local-first milestone. Further cloud ingest, wider provider coverage, platform packaging, and controlled routing are post-v0.1 work.

## V0.1.1 packaging patch

Status: RELEASE3 complete.

Manual acceptance of the public v0.1.0 artifact found two distribution bugs:

- checksum files referenced the build-time `dist/` path instead of the downloaded archive basename
- standalone archives omitted the default `policies/` and `catalogs/` runtime data required by default CLI paths

RELEASE3:

- bump package version to `0.1.1`
- include complete `policies/` and `catalogs/` directories in the Linux archive
- generate portable checksum files using only the archive basename
- add an extracted-package acceptance gate for observe/report/profile/recommend/explain/advisory/catalog
- publish a new `v0.1.1` tag/release without mutating v0.1.0

## V0.1.2 cost-evidence patch

Status: RELEASE4 complete.

Manual acceptance of v0.1.1 exposed a fail-open cost-reporting path when the observed current runner shape was not present in the internal candidate catalog.

RELEASE4:

- keep current estimated cost unknown when no current runner rate is known
- keep estimated savings unknown in the same case
- preserve known recommended candidate cost
- regression-test both known-current and unknown-current pricing paths
- remove the stale duplicate README license warning
- publish a new `v0.1.2` tag/release without mutating earlier releases


RELEASE4 proof:

- public tag: `v0.1.2`
- source commit: `8d71422910b8d9cd90ca6f04dfdd20b41526d135`
- targeted unknown-current-cost regression test: success
- dependency-license audit: success
- extracted standalone package acceptance: success
- payload re-verification: success
- GitHub Release publication: success
- v0.1.0 and v0.1.1 remain unchanged

## VS9 - Cloud ingest

Only after local dogfood proves useful:

- HTTP ingestion
- first normative OpenAPI
- authentication
- multi-repository history
- cloud UI

Cloudflare is a possible hosting platform, not part of the core product identity.

## VS10 - Advisory CI integration

Emit suggestions for existing CI systems without changing workflows automatically.

## VS11 - Controlled routing

Only after advisory recommendations prove reliable, allow opt-in automation through existing CI control planes.
