# Capacity adapters

CAPACITY5 adds the first live provider adapter while keeping `CapacitySnapshot` as the stable provider-neutral boundary.

## GitHub Actions

The GitHub adapter is read-only. It first reads repository metadata to record whether the repository is public or private, then queries active workflow runs, fetches their latest jobs, filters jobs by an exact requested runner label, and records:

- `queue_depth` from matching jobs whose status is `queued`
- `running_jobs` from matching jobs whose status is `in_progress`
- top-level `repository_visibility` from GitHub repository metadata

The GitHub API is the observation source, not necessarily the runner provider. The provider remains an explicit input so the same control plane can observe GitHub-hosted, Depot, or self-hosted runner labels without rewriting provider identity.

Every state emitted by the current GitHub adapter is explicitly scoped to `repository:<owner/name>`, because the adapter traverses active runs for exactly one repository. It does not relabel this as account-wide evidence even when a configured `parallel_slots` value came from an account plan.

This matters for standard GitHub-hosted runners: their total concurrency is plan/account scoped. Those catalog offers therefore require `provider_account` evidence and will not be selected from a repository-scoped snapshot. A later account/org observer must collect the broader active-job set before CIShape can make that comparison safely.

The adapter does not infer facts that the selected GitHub REST endpoints do not expose reliably for the target capacity pool.

Therefore these remain explicit inputs:

- `parallel_slots`
- `slot_turnover_ms`
- `cache_state`
- `cache_penalty_ms`

That is deliberate. A missing concurrency limit is not equivalent to unlimited capacity, and an unknown cache penalty is not zero.

## CLI

Inside GitHub Actions, `GITHUB_REPOSITORY`, `GITHUB_API_URL`, and `GITHUB_TOKEN` can supply repository/API/token context.

Example:

```bash
cargo run -- capacity-github \
  --provider github-actions \
  --offer-id ubuntu-latest-private-x64 \
  --runner-label ubuntu-latest \
  --parallel-slots 20 \
  --slot-turnover-ms 12000 \
  --cache-state warm \
  --cache-penalty-ms 0 \
  --output .cishape/capacity/github-actions.json
```

Outside Actions, pass `--repository owner/repo` and set the token environment variable.

The token variable name defaults to `GITHUB_TOKEN`. The token value is never emitted.

## Multi-pool observation plan

CAPACITY6 can observe several runner pools in one control-plane scan and emit one comparable snapshot.

The plan is versioned data:

```json
{
  "schema_version": 1,
  "pools": [
    {
      "provider": "github-actions",
      "offer_id": "ubuntu-latest-private-x64",
      "required_labels": ["ubuntu-latest"],
      "parallel_slots": 4,
      "slot_turnover_ms": 12000,
      "cache_state": "warm",
      "cache_penalty_ms": 0
    }
  ]
}
```

Every value that describes capacity remains explicit evidence. The numbers in the repository example are illustrative, not claims about provider limits.

A selector matches only when the job contains every label in `required_labels`. If an active job matches more than one configured pool, CIShape fails closed because counting it in both pools would corrupt economics evidence.

```bash
cargo run -- capacity-github-plan \
  --config examples/github-capacity-plan-v1.json \
  --repository owner/repo \
  --output .cishape/capacity/live.json
```

That single output can be passed directly to `economics` or `workflow-economics`. The GitHub API traversal happens once for the observation, regardless of the number of configured pools.

## Evidence semantics

The adapter paginates both workflow-run and workflow-job reads. It requests only `queued` and `in_progress` workflow runs and uses the latest execution attempt's jobs.

GitHub's filtered workflow-run search has a finite search result cap. CIShape fails closed if the reported active-run result set exceeds that cap rather than silently treating a truncated search as complete.

The REST reads are not an atomic provider snapshot. Queue state can move while pages are being collected. `observed_at` records when collection started, and downstream economics should treat every CapacitySnapshot as time-scoped evidence rather than a durable provider fact.

Repository visibility is evidence, not an inference from a runner label or offer ID. Downstream economics compares it with any contextual ProviderCatalog offer and fails closed on missing or mismatched context.

## Boundary

Adapters only observe and normalize evidence.

They do not:

- dispatch or cancel workflow runs
- change runner groups
- change concurrency settings
- route jobs
- invent concurrency limits, turnover, or cache behavior

CAPACITY1-4 consume the resulting CapacitySnapshot without provider-specific logic.
