# Research: GitHub personal-account capacity authentication

Status: deferred follow-up after CAPACITY10.

## Trigger

The first real local CAPACITY10 proof used the credential returned by GitHub CLI and failed closed with:

```text
GitHub token did not expose owned_private_repos; cannot prove complete personal-account repository inventory
```

This is the intended safety behavior.

CIShape must not emit `provider_account` capacity evidence unless it can prove that the observed repository inventory covers the whole account scope whose concurrency limit is supplied.

## What CAPACITY10 already guarantees

The personal-account collector:

- authenticates the GitHub user
- requires the target repository owner to match the authenticated user
- requires visibility into the owned private-repository count
- enumerates repositories owned by the authenticated user
- requires the enumerated count to match public + owned private repository counts
- scans active Actions jobs across the complete inventory
- emits `provider_account:github:user:<login>` only after that proof succeeds
- keeps `parallel_slots` explicit
- fails closed on incomplete evidence

## Open authentication question

The remaining problem is not queue math. It is proving account inventory completeness with a least-privilege credential.

Research these GitHub authentication models:

- GitHub CLI OAuth credential
- classic personal access token
- fine-grained personal access token
- GitHub App installation token

For each model determine:

1. whether all owned repositories in the intended scope can be enumerated
2. whether private owned-repository completeness can be proven
3. whether Actions run/job state can be read for every repository in that scope
4. the minimum permissions/scopes
5. whether the model is suitable for CIShape product onboarding

## Product direction

Do not weaken CAPACITY10 to make one credential type pass.

For v0.1, account-wide capacity evidence remains optional and this research is non-blocking.

For a hosted/product path, prefer explicit GitHub App installation scope over asking users for a broad personal token, if the App model can support a well-defined capacity scope.

Tracked by issue #55.
