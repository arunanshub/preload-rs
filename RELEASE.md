# Releases

Stable releases are built when a tag such as `v0.1.0` is pushed. The tag must
match the version in `crates/cli/Cargo.toml` and point to a commit merged into
`master`. Bump the binary's Cargo version and update `Cargo.lock` before tagging
a new version. Prerelease tags and tags with leading zeroes are rejected.

GoReleaser builds and packages exactly two Linux x86-64 targets using upstream
cross images. The existing Cargo release profile controls optimization. SQLx
uses the checked-in `.sqlx` metadata; a build database is not required.

| Archive suffix | Use |
| --- | --- |
| `x86_64-unknown-linux-gnu.tar.gz` | Linux with glibc |
| `x86_64-unknown-linux-musl.tar.gz` | Linux with a statically linked libc, including Alpine |

Each archive includes the executable, license, usage guide, release guide, and
example configuration. The workflow smoke-tests both binaries in their pinned
cross images and rejects dynamically linked musl binaries.

The build job has read-only repository access. A separate job downloads the
outputs, verifies their checksums, and generates signed SLSA provenance with
GitHub's `actions/attest`. It verifies that provenance against the repository,
workflow, tag, and commit before uploading the archives, `checksums.txt`, and
`provenance.sigstore.json` to a GitHub Release. The release stays a draft until
all assets have been uploaded. No crates or container images are published.

## Download and verify

Download your archive, `checksums.txt`, and `provenance.sigstore.json` from the
[GitHub Release](https://github.com/arunanshub/preload-rs/releases). For example,
when downloading the glibc archive for version `0.1.0`, verify it before running:

```sh
gh attestation verify preload-rs_0.1.0_x86_64-unknown-linux-gnu.tar.gz \
  --repo arunanshub/preload-rs \
  --signer-workflow arunanshub/preload-rs/.github/workflows/release.yml \
  --source-ref refs/tags/v0.1.0 \
  --deny-self-hosted-runners \
  --bundle provenance.sigstore.json

sha256sum --check --ignore-missing checksums.txt
tar -xzf preload-rs_0.1.0_x86_64-unknown-linux-gnu.tar.gz
./preload-rs --help
```

Use the corresponding musl filename if that is the archive you downloaded.
The signed bundle is also available through GitHub's attestation API; omit
`--bundle` to fetch it there. For additional assurance, provide
`--source-digest <expected-commit-sha>` obtained from a trusted source.
Provenance establishes the build's identity and source; it is not a guarantee
that the program is free of vulnerabilities or that rebuilds are byte-identical.

## Build locally

Install Rust `1.99.0`, cross `0.2.5`, GoReleaser `2.18.3`, and Docker. Then run:

```sh
goreleaser check
goreleaser release --snapshot --clean
```

Outputs go into `release/`. Snapshot builds never publish. Pull requests run the
same builds and smoke tests, without signing or publishing. Do not restore
pull-request build caches into the release workflow.

Keep the Rust and tool versions in `.github/workflows/release.yml` and the image
digests in `Cross.toml` up to date together; let the PR builds verify upgrades.
Protect release tag creation and modification with repository rulesets. If a
release upload fails after its draft was created, inspect that draft before
retrying: the workflow intentionally does not overwrite existing release assets.
