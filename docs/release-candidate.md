# Release candidate

CIShape can build a reproducible, non-publishing Linux x86_64 release candidate from GitHub Actions.

## Boundary

The `Release Candidate` workflow is intentionally manual-only.

It:

- checks out the exact workflow-dispatch commit
- requires the dispatch ref to be `main`
- resolves a fresh dependency lock once for the run, records its SHA-256, then builds `cishape` with `cargo build --release --locked`
- smoke-tests `cishape --help` and `cishape demo`
- audits third-party dependency licenses with pinned cargo-about tooling
- packages the binary with README, the v0.1 quickstart, the exact dependency lock, the AGPL license, and generated third-party notices
- emits a SHA-256 checksum
- emits a machine-readable provenance manifest
- uploads the files as a GitHub Actions artifact

It does **not**:

- create a Git tag
- create a GitHub Release
- publish to crates.io
- sign artifacts
- deploy anything
- claim support for platforms that are not packaged

## Artifact layout

The Linux candidate archive is named from Cargo metadata and the Rust host target:

```text
cishape-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

The archive contains:

```text
cishape-v0.1.0-x86_64-unknown-linux-gnu/
  cishape
  README.md
  QUICKSTART.md
  LICENSE
  THIRD_PARTY_LICENSES.html
  Cargo.lock
```

The workflow also uploads:

```text
cishape-v0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256
cishape-v0.1.0-x86_64-unknown-linux-gnu.manifest.json
```

The manifest records the version, target, repository, exact commit SHA, workflow run/attempt, archive filename/checksum, and the SHA-256 of the generated `Cargo.lock`.

The repository does not yet commit `Cargo.lock`. RELEASE1 therefore freezes the dependency graph at the start of each candidate run and preserves that exact lockfile inside the archive. This makes the produced candidate reconstructable, but a future release-hardening slice should decide whether the application lockfile becomes a committed repository input.

## Third-party license audit

The release path uses `cargo-about 0.9.2` from its pinned Linux release artifact and verifies that tool archive with a checked SHA-256 before execution.

The checked-in `about.toml` is fail-closed: dependencies whose detected licenses are not explicitly accepted stop CI/release packaging for review. The generated `THIRD_PARTY_LICENSES.html` records dependency package names/versions and detected full license texts.

This report is distribution/compliance evidence, not legal advice.

## Verify locally

After downloading the workflow artifact:

```bash
sha256sum -c cishape-v0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256

tar -xzf cishape-v0.1.0-x86_64-unknown-linux-gnu.tar.gz

./cishape-v0.1.0-x86_64-unknown-linux-gnu/cishape --help
./cishape-v0.1.0-x86_64-unknown-linux-gnu/cishape demo
```

On macOS, verify the checksum with:

```bash
shasum -a 256 -c cishape-v0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256
```

The Linux binary itself should be executed on a compatible Linux x86_64 system.

## License boundary

CIShape is licensed under `AGPL-3.0-only`, and release-candidate archives include the full `LICENSE` text.

RELEASE1 remains a **build/provenance proof** because the workflow itself intentionally does not publish a Git tag or GitHub Release. The licensing blocker has been removed; RELEASE2 can now define the public publication path.

## Proven release candidate

The first successful proof is Release Candidate #2 from commit:

```text
520426443708b07c6201f5e5fbe7640cf5262672
```

The workflow completed the dependency lock, locked release build, smoke test, packaging, checksum/provenance generation, and artifact upload. The uploaded artifact was:

```text
cishape-v0.1.0-x86_64-unknown-linux-gnu-rc-36510349471-1
```

This proves the non-publishing Linux v0.1 candidate pipeline end to end.

## Follow-up

A later RELEASE2 may add:

- additional supported targets
- signing/attestations
- GitHub Release publication
- package-manager distribution
- crates.io publication if appropriate

Those are separate from proving that the v0.1 binary can be built and packaged deterministically.
