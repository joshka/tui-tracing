# Release Checklist

This crate is experimental, but releases should still use a repeatable quality gate.

## Before Publishing

1. Confirm the README, crate docs, examples, and known limitations describe the same API.
1. Confirm the version, changelog, license, repository metadata, and docs.rs metadata are ready.
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
1. Publish with a dry run first.
1. Tag only after the release artifact is validated.

## Manual Validation Gaps

- Platform support is not yet validated on Windows, macOS, and Linux CI.
- No MSRV is declared.
- No `cargo-deny` policy is configured yet.
- No fuzz targets or benchmarks exist yet.
- Markdown link checking is not configured because external link checks can be flaky.
