# Release and supply-chain plan

Status: Design contract. No public binary release exists yet.

## Principles

- release history is deterministic from Git
- release automation is tooling, not CIShape runtime code
- a release must be possible without an AI service
- published binaries must be traceable to repository commits
- supply-chain metadata is part of the release artifact set

## Commit and changelog model

CIShape uses Conventional Commits.

`git-cliff` is the intended canonical changelog generator.

An optional decision/AI layer may later generate release highlights, migration summaries, or audience-specific prose from deterministic release facts. It must not decide version history or silently rewrite canonical changelog facts.

## Planned release flow

```text
main
  |
  v
release preparation
  |
  + version/SemVer checks
  + deterministic CHANGELOG
  |
  v
release PR
  |
  v
tag
  |
  v
cross-platform binary build
  |
  + checksums
  + SBOM
  + provenance/attestation
  |
  v
GitHub Release
```

Candidate tooling to evaluate before the first release:

- release-plz for Rust release PR/version orchestration
- git-cliff for changelog generation
- cargo-semver-checks for public Rust API compatibility
- dist for binary packaging/distribution
- cargo-deny for advisory/license/source policy
- zizmor for GitHub Actions security analysis
- GitHub artifact attestations for build provenance and SBOM attestation

Tool choices remain replaceable until the first release pipeline is implemented.

## Binary targets

Initial targets should be selected only when `cishape observe` is stable enough to ship. Expected priorities are Linux amd64/arm64 and macOS arm64/amd64, followed by Windows where observation semantics are supported.
