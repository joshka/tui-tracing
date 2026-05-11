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
1. Regenerate the demo GIF when the viewer, demo, or README screenshots change:

   ```sh
   just demo-gif
   ```

   The generated GIF is written under `target/vhs/`, which is ignored by version control. Do not
   commit generated GIFs. For pull request review, attach the generated GIF directly to a pull
   request comment. Use GitHub release assets only for actual release artifacts that should become
   stable README or docs links.

   VHS tapes should follow these project rules:

   - Use the `Aardvark Blue` theme for Ratatui-aligned screenshots and GIFs.
   - Keep GIF width at or below 1200 pixels unless a specific target requires otherwise.
   - Give dense log screens enough dwell time to be readable, but watch generated GIF size.
   - Keep generated binaries out of the repository; the tape is the durable source artifact.
   - Put inherited-environment cleanup in `just demo-gif`, not in the tape. In particular, unset
     `NO_COLOR` and related color variables before running VHS.
   - Hide setup, build, and quit commands. Show the terminal only for the demo content.
   - Keep one README-oriented GIF per pull request. The demo should stream briefly, jump to the
     oldest visible rows to stop tailing, select a row, and dwell on the detail pane long enough to
     inspect.
   - Keep tapes simple. Use sleeps for fixed demo pacing unless a startup/build wait is genuinely
     needed.

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
