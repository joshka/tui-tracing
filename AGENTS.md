# Repository Guidelines

## Project Structure & Module Organization

This crate is a Rust library for viewing `tracing` output in Ratatui apps. Core source lives in
`src/`: capture in `layer.rs`, storage in `store.rs`, filtering in `filter.rs`, rendering in
`viewer.rs`, records in `record.rs`, fields in `field.rs`, and formatting in `format.rs`.
Integration tests live in `tests/public_workflow.rs`. The runnable demo is `examples/demo.rs`.
Maintainer docs are in `docs/`, and VHS source tapes live in `tapes/`. Generated artifacts belong
under `target/` and must not be committed.

## Build, Test, and Development Commands

- `cargo run --example demo`: run the interactive tracing viewer demo.
- `just fmt`: format Rust code with nightly rustfmt.
- `just check`: run all-feature and no-default-feature cargo checks.
- `just test`: run workspace tests, doctests, and example tests.
- `just clippy`: run clippy with warnings denied.
- `just doc`: build rustdoc with warnings denied.
- `just demo-gif`: regenerate the VHS demo GIF into `target/vhs/`.
- `just ci`: run the full local gate expected before PRs.

## Coding Style & Naming Conventions

Use idiomatic Rust 2024, rustfmt, and small concept-owned modules. Public types use descriptive
`Trace*` names such as `TraceStore`, `TraceViewer`, and `TraceFilter`. Keep APIs boring and
explicit: storage owns retained data, filters own display-time matching, and widgets own rendering
state. Do not introduce unsafe code.

## Testing Guidelines

Use focused unit tests for internal invariants and integration tests for public workflows. Add
tests for new public behavior, especially filtering, scrolling, storage counters, and rendering
state. Prefer deterministic tests over timing-sensitive checks. Run `just ci` before considering a
change ready.

## Commit & Pull Request Guidelines

Keep commits and PRs focused on one coherent change. PRs should include a summary, linked issues
such as `Closes #10`, and validation commands. Open the GitHub PR page for review after creating a
PR. For visual changes, attach GIFs directly to PR comments; use GitHub release assets only for
actual releases.

## VHS Demo Artifacts

VHS tapes are text source and may be committed; generated GIFs must stay out of the repository.
Use one calm README-oriented GIF per PR, generated from `tapes/demo.tape`. Use `Aardvark Blue`,
keep captures at or below 1200px wide, hide setup/build/quit commands, and put environment cleanup
such as unsetting `NO_COLOR` in `just demo-gif`, not in the tape.
