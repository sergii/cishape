# Release candidate

CIShape can build a reproducible, non-publishing Linux x86_64 release candidate from GitHub Actions.

## Boundary

The `Release Candidate` workflow is intentionally manual-only.

It:

- checks out the exact workflow-dispatch commit
- requires the dispatch ref to be `main`
- builds `cishape` with `cargo build --release --locked`
- smoke-tests `cishape --help` and `cishape demo`
- packages the binary with README and the v0.1 quickstart
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
```

The workflow also uploads:

```text
cishape-v0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256
cishape-v0.1.0-x86_64-unknown-linux-gnu.manifest.json
```

The manifest records the version, target, repository, exact commit SHA, workflow run/attempt, archive filename, and archive SHA-256.

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

The repository does not yet declare a redistribution license.

Therefore RELEASE1 is a **build/provenance proof**, not a public distribution release. The workflow artifact is useful for validating packaging and reproducibility, but the project should not create a public GitHub Release or publish to a package registry until the license decision is explicit.

## Follow-up

A later RELEASE2 may add:

- additional supported targets
- signing/attestations
- GitHub Release publication
- package-manager distribution
- crates.io publication if appropriate

Those are separate from proving that the v0.1 binary can be built and packaged deterministically.
