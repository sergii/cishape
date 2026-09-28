# Hermetic builds, remote execution, and reusable computation

**Status:** Research note  
**Decision:** None  
**Commitment:** None  
**Trigger:** Discussion around Nix remote builders, Hydra, and the broader family of reproducible / remote build systems.

## Why this note exists

We encountered an architectural idea that is adjacent to CIShape's current problem space:

> Instead of treating CI as a sequence of shell commands executed from scratch, some systems model work as reproducible actions with explicit inputs, environments, dependencies, and outputs.

That model can enable cache reuse, remote execution, build farms, and stronger reproducibility.

CIShape does **not** currently need to adopt any of these systems. This note exists so the idea and the surrounding prior art are not lost.

## The conceptual shift

Traditional CI often looks like:

~~~text
job
  -> checkout
  -> install dependencies
  -> run build commands
  -> run tests
  -> discard machine
~~~

A reproducible-action model looks more like:

~~~text
action
  inputs
  environment
  operation
  declared outputs
        |
        v
  stable identity
        |
   +----+----+
   |         |
 cache hit   cache miss
   |         |
 reuse     execute remotely or locally
~~~

This is a different abstraction from runner sizing.

CIShape today primarily asks:

> What resources did this CI workload actually consume, and what runner shape should execute it?

These systems often ask:

> Can this exact computation be reused, and if not, where should it execute?

The two areas may eventually intersect, but they should not be conflated.

## Nix family

### Nix

Nix is a package and build system based around derivations and a dedicated store.

Interesting properties:

- explicit build inputs;
- reproducible environments;
- binary substitution/cache;
- remote builders;
- multiple versions of dependencies coexisting;
- Linux and macOS support.

Possible CIShape relevance:

- a job may delegate expensive deterministic build work to remote Nix builders;
- measured runner utilization may therefore under-represent total compute consumed by a workflow;
- CIShape may eventually need vocabulary for delegated/off-runner compute;
- Nix build farms are useful prior art for capability-aware worker placement.

### Nixpkgs

Nixpkgs is the large package collection commonly consumed through Nix.

It is not itself a CI system.

### NixOS

NixOS is a Linux distribution configured declaratively through Nix.

NixOS is not required to use Nix or remote builders.

### Hydra

Hydra is a Nix-native continuous build, test, and release system.

It is useful prior art for:

- build farms;
- scheduling Nix builds;
- distributed workers;
- binary artifact publication;
- orchestration around reproducible builds.

No current CIShape decision depends on Hydra.

References:

- https://nix.dev/
- https://nix.dev/manual/nix/stable/advanced-topics/distributed-builds
- https://github.com/NixOS/hydra

## Bazel and REAPI

Bazel models builds and tests as a dependency graph of actions.

The more general concept is the Remote Execution API (REAPI), which separates:

~~~text
Content Addressable Storage
  -> immutable inputs and outputs

Action Cache
  -> action identity to prior result

Execution Service
  -> schedule uncached work on workers
~~~

This is important prior art because it separates:

- action identity;
- artifact identity;
- cache lookup;
- scheduling;
- worker capability;
- execution.

Possible CIShape relevance:

- a "job" can contain many remotely executed actions;
- runner CPU/memory may no longer represent the complete cost of the job;
- runner selection and remote-execution worker selection may become separate optimization problems;
- CIShape's provider-neutral model may eventually need to distinguish orchestration runner resources from delegated compute resources.

References:

- https://bazel.build/remote/rbe
- https://github.com/bazelbuild/remote-apis

## BuildKit

BuildKit is Docker's modern build engine.

Its internal LLB representation is a content-addressable dependency graph and supports:

- precise build caching;
- independent graph execution;
- skipping unused stages;
- external cache import/export;
- remote/persistent builders.

For web projects this may be particularly relevant because teams can get sophisticated reuse without adopting Nix or Bazel if they already use Dockerfiles.

Possible CIShape relevance:

- Docker build CPU/memory may run on the CI runner or on a remote BuildKit builder;
- the same workflow YAML can therefore represent very different actual compute topologies;
- "runner shape" alone may be insufficient to explain cost or bottlenecks.

Reference:

- https://docs.docker.com/build/buildkit/

## Pants and Buck2

Pants and Buck2 are further examples of fine-grained build graphs with caching and remote execution support.

They are useful as evidence that this is a broader architectural family, not a Nix-only or Bazel-only pattern.

References:

- https://www.pantsbuild.org/dev/docs/using-pants/remote-caching-and-execution
- https://buck2.build/docs/users/remote_execution/

## Earthly and Dagger

Earthly and Dagger sit closer to portable CI/build workflow definitions around container execution.

Interesting ideas include:

- local/CI parity;
- persistent remote builders;
- keeping cache close to compute;
- portable pipeline definitions;
- reducing dependence on a specific CI vendor.

References:

- https://docs.earthly.dev/
- https://docs.dagger.io/

## Where this sits for Rails, Go, and web development

### Rails

Rails commonly has multiple dependency layers:

~~~text
system/toolchain
  -> Ruby, Node, libpq, libvips, compiler, OpenSSL

language dependencies
  -> Bundler gems

frontend dependencies
  -> npm/pnpm/yarn packages

runtime dependencies
  -> PostgreSQL, Redis, browser, services
~~~

Nix or BuildKit may stabilize and cache the lower layers, while Bundler and JavaScript package managers remain responsible for their own ecosystems.

### Go

Simple Go services already have strong module and build caching, so a heavyweight external build system may add little.

The value increases for repositories involving:

- protobuf/code generators;
- C/C++ libraries;
- eBPF;
- native databases;
- frontend assets;
- multi-language builds.

### Containerized web applications

BuildKit is likely the most natural nearby prior art because the Dockerfile may already describe much of the build environment.

## Why this matters specifically to CIShape

CIShape's current product boundary is still correct:

~~~text
observe actual job consumption
  -> normalize telemetry
  -> build JobShape history
  -> evaluate RunnerShape candidates
  -> recommend a compatible runner shape
~~~

The research raises a future modeling question:

~~~text
CI workflow
   |
   +--> orchestration runner
   |      CPU / memory / disk / time
   |
   +--> remote build service
   |      separate CPU / memory / cache / time
   |
   +--> remote execution workers
          separate CPU / memory / cost
~~~

If this topology becomes common in CIShape users, a single runner-utilization record may not represent the actual workload.

Potential future concepts, **not accepted design**:

- ExecutionTopology;
- DelegatedCompute;
- BuildBackend;
- RemoteExecutionBackend;
- CacheHitRate;
- RemoteWorkerShape;
- orchestration-vs-execution resource attribution.

These names are placeholders only.

## Questions worth preserving

1. Should CIShape optimize only the visible CI runner, or eventually account for delegated build/remote-execution compute?
2. Can provider telemetry tell us when BuildKit, Nix, Bazel, or another tool moved work away from the runner?
3. If a job has a remote cache hit, should that execution be included in the same JobShape history as a cold build?
4. Should cache state become a JobShape facet?
5. How should CIShape compare a larger local runner with a smaller runner plus remote execution?
6. Does runner recommendation remain deterministic when external build farms introduce independent queueing and resource limits?
7. Can OpenTelemetry CI/CD semantics represent this topology cleanly, or would CIShape need additional domain concepts?
8. Should remote worker sizing ever become part of CIShape, or is that a separate product boundary?

## Decision status

No implementation is proposed.

Do not add Nix, Hydra, Bazel, REAPI, BuildKit integrations, or new CIShape domain entities based on this note alone.

The note should be revisited only when:

- real CI telemetry shows delegated compute materially affecting recommendations;
- CIShape starts profiling workflows that use remote build farms;
- runner-only optimization produces misleading cost/performance conclusions; or
- a concrete integration request creates a product decision.

Until then this remains prior art and an open research thread.
