# Release Checklist

This crate is experimental, but releases should still use a repeatable quality gate.

## Before Publishing

1. Confirm the README, crate docs, examples, and known limitations describe the same API.
1. Confirm the version, changelog, license, repository metadata, and docs.rs metadata are ready.
1. Confirm the release workflow and crates.io trusted publisher agree on:

   - repository owner: `joshka`
   - repository name: `tui-tracing`
   - workflow filename: `release.yml`
   - GitHub environment: `crates-io`

1. Run the local release gate:

   ```sh
   just ci
   ```

1. Run the pending dependency-policy check once `cargo-deny` is configured:

   ```sh
   cargo deny check
   ```

1. Validate package contents if you need to inspect the archive before publishing:

   ```sh
   cargo package --workspace
   ```

   During pre-commit development, `just ci` uses `cargo package --workspace --allow-dirty` so the
   package can still be verified before the working copy is clean. Use the stricter command above
   for an actual release candidate.

1. Inspect the packaged README, examples, license files, and included source files.
1. Regenerate demo GIFs when the viewer, demo, or README screenshots change:

   ```sh
   just demo-gif
   ```

   Generated GIFs are written under `target/vhs/`, which is ignored by version control. Do not
   commit generated GIFs. Upload them to the relevant pull request for review and to the GitHub
   release assets when they should be stable README or docs links.

1. Publish with a dry run first.
1. For the first crates.io release, publish manually with a scoped token. crates.io requires the
   first release before trusted publishing can be configured.
1. After the first manual release, configure crates.io trusted publishing for the `release.yml`
   workflow and revoke the temporary token.
1. For later releases, merge the release-plz release PR. The `Release` workflow publishes through
   trusted publishing and creates the GitHub release/tag.

## Manual Validation Gaps

- Platform support is not yet validated on Windows, macOS, and Linux CI.
- No `cargo-deny` policy is configured yet.
- No fuzz targets or benchmarks exist yet.
- Markdown link checking is not configured because external link checks can be flaky.
