# Design And Quality Standards

This document is durable guidance for `tui-tracing`. It is the source of truth for the
maintenance posture expected from implementation, documentation, tests, and automation.

## Library Goal

Build this as a production-quality Rust library, optimized for long-term maintenance, reader
clarity, and idiomatic public API design.

The goal is not just to make the code work. The goal is to make the crate easy to understand,
easy to audit, easy to extend, and hard to misuse.

## General Standards

- Prefer obvious, boring, well-factored code over clever abstractions.
- Optimize for reader locality: a maintainer should be able to understand a module from top to
  bottom without chasing hidden state across the crate.
- Keep concepts coherent. A module should own one recognizable idea, not a bucket of loosely
  related helpers.
- Use abstractions only when they reduce cognitive load, duplication, or misuse.
- Favor small, meaningful types and functions with clear invariants.
- Make side effects explicit in names, call sites, and docs.
- Prefer direct implementation code over generic framework-shaped code unless the abstraction is
  clearly paying for itself.
- Keep public API shape intentional. Do not preserve accidental development-era compatibility if
  the crate is pre-release.
- Re-exports should improve discovery, not hide ownership.
- Avoid vague docs, generic examples, unnecessary wrappers, over-commented trivial code, broad
  modules, and examples that only prove a function can be called.

## Public API Standards

- Every public item should have a clear owning module.
- Every public type should explain what it represents and what invariants it maintains.
- Fallible APIs should document meaningful error behavior with `# Errors`.
- APIs with side effects should document ordering, lifecycle, cleanup, failure, and caller
  responsibility.
- Use `From`, `TryFrom`, `Default`, `Display`, and common derives where they make value types
  easier and safer to use without weakening invariants.
- Public error enums should usually be `#[non_exhaustive]` unless exhaustive matching is
  intentional.
- Avoid giant crate roots. The root should teach the crate, expose the primary path, and point to
  deeper modules.

## Module Standards

- Put the main concept first.
- Organize modules by domain concepts a user or maintainer would recognize.
- Reading order should follow either execution order or conceptual dependency order.
- Keep helpers near the code that uses them unless they are independently important concepts.
- Split files when it lowers the number of facts a reader must hold at once.
- Do not split files merely to create a maze of tiny modules.
- Prefer caller-before-callee ordering when it improves readability.
- Keep tests near the behavior they prove when practical.

## Documentation Standards

- Treat rustdoc as part of the API, not decoration.
- Each public module should answer what concept it owns, when to use it, what to read next, and
  what it intentionally does not own.
- Provide guide-level docs as well as reference docs: tutorials for first success, how-tos for
  common tasks, explanations for core concepts, and reference docs for exact API behavior.
- Write docs for non-linear readers. Someone may land on any module, type, example, or guide
  first.
- Make start-here paths obvious.
- Distinguish user docs, maintainer docs, design notes, release checklists, and historical review
  docs.
- Cross-link docs directionally: start here, source of truth, read next, and related lower-level
  detail.
- Examples should show practical use, not just construction.
- Prefer simple, obvious examples over elaborate mini-frameworks.
- Use realistic examples that teach ownership, lifecycle, errors, or integration shape.

## Testing Standards

- Tests should prove contracts, not implementation trivia.
- Use focused unit tests for pure behavior and invariants.
- Use integration tests for public workflows across modules.
- Use doctests where examples can compile without fragile environment assumptions.
- Add regression tests for bugs and edge cases.
- Test error behavior, invalid inputs, cancellation, drop, cleanup paths, and boundary conditions.
- Prefer deterministic tests over tests that depend on timing or external state.
- Add fuzzing or property tests where parsers, formatters, protocol decoders, state machines, or
  untrusted input are involved.
- Add benchmarks where performance claims, hot paths, or allocation behavior matter.

## Linting, CI, And Automation Standards

- Treat CI as part of the library's public quality bar, not as an afterthought.
- CI should check the same commands maintainers are expected to run locally.
- Prefer fast, deterministic CI jobs with clear failure modes.
- Keep required checks strict enough that warnings, broken docs, stale examples, and formatting
  drift do not accumulate.
- Avoid noisy automation that produces dependency churn or low-signal failures.

## Baseline Local And CI Checks

- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test --doc --workspace --all-features`
- `cargo test --examples --workspace --all-features`
- `cargo clippy --workspace --all-features --all-targets -- -D warnings`
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps`
- `cargo check --workspace --all-targets --all-features`
- `cargo check --workspace --all-targets --no-default-features`
- `cargo check --workspace --all-targets --features <feature-set>`, for important feature
  combinations
- `cargo package --workspace` in CI and release checks
- `cargo package --workspace --allow-dirty` for local pre-commit validation
- `cargo audit`
- `cargo machete`
- `cargo deny check`, once dependency, license, and advisory policy is configured
- `cargo udeps`, periodically if nightly is acceptable
- Markdown linting for README, docs, changelog, and contribution docs
- Spell checking for public docs if the project has enough documentation to justify it
- Fuzz and benchmark compile checks if the crate has fuzz targets or benches

## Rust Lint Posture

- Use `clippy -D warnings` in CI.
- Prefer crate-level lint configuration only when it encodes a durable project rule.
- Do not enable a large pile of pedantic lints blindly.
- Enable stricter lints selectively when they improve correctness, public API quality, or
  maintainability.
- Deny accidental unsafe code with `#![forbid(unsafe_code)]`.
- Consider denying or warning on missing docs, rustdoc broken intra-doc links, rustdoc bare URLs,
  unreachable public items, unused crate dependencies, unexpected cfgs, and missing debug
  implementations for public types where useful.
- Avoid lint suppressions unless they explain why the exception is correct.
- Never use lint allows as cleanup-by-silencing.

Suggested crate attributes for library crates:

```rust
#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::bare_urls)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(unused_crate_dependencies)]
```

Use `#![warn(missing_docs)]` once the API is ready for that pressure. Earlier in exploration, it
may be too noisy.

## CI Structure

- Run formatting and clippy early because they fail fast.
- Run docs as a first-class job, not only as a release task.
- Run tests with all features.
- Run important feature combinations separately.
- Run MSRV checks if the crate declares an MSRV.
- Run platform checks for every supported platform.
- If the crate claims Windows, macOS, and Linux support, CI should at least compile and run
  relevant smoke tests on each.
- If platform behavior cannot be fully tested in CI, document the manual validation gap.
- Keep slow fuzzing, long benchmarks, and exhaustive compatibility tests out of required PR CI
  unless they are fast and deterministic.
- Add scheduled or manual CI for expensive checks.

Recommended GitHub Actions shape:

- `fmt`
- `clippy`
- `test`
- `doc`
- `feature-powerset` or selected feature matrix
- `msrv`, if applicable
- `platform-smoke`
- `markdown`
- `package`
- `dependency-policy`
- Optional scheduled `fuzz-smoke`
- Optional scheduled `bench-no-run`

## Feature And Dependency Automation

- Use `cargo hack` or equivalent to test important feature combinations.
- Make feature flags additive where possible.
- Avoid features that silently change public semantics in surprising ways.
- Keep optional dependencies tied to clearly named features.
- Use the widest honest semver-compatible dependency requirements.
- Do not raise minimum dependency versions unless the crate actually requires the newer API or
  behavior.
- Keep dependency updates separate from behavior changes when possible.
- Use grouped dependency-update automation to reduce PR noise.
- Treat semver-compatible dependency breakage as a real downstream integration concern, not just
  local maintenance.

## Documentation Automation

- Build docs in CI with warnings denied.
- Ensure README examples, rustdoc examples, and examples directory stay aligned.
- Run doctests for public examples.
- Lint markdown tables, headings, links, and fenced code blocks.
- Prefer checked links where feasible, but avoid making CI flaky on external network failures.
- For docs.rs, configure metadata intentionally: all relevant features, documented cfgs when
  needed, and platform docs if platform-specific APIs matter.

## Release Automation

- Have one command or documented checklist that reproduces the release gate.
- Use release-plz to prepare release PRs, tag published versions, create GitHub releases, and
  publish crates.io versions after the first manual release.
- Use crates.io trusted publishing for CI releases. Do not store a long-lived
  `CARGO_REGISTRY_TOKEN` secret in GitHub Actions.
- Keep the trusted publisher bound to a specific workflow filename and GitHub environment.
- Validate package contents before publishing with `cargo package --workspace` or per-crate
  packaging as appropriate.
- Inspect included docs, examples, license files, README, and generated artifacts.
- Ensure changelog, version numbers, crate metadata, docs.rs config, and README support claims
  agree.
- Do a dry-run publish where possible.
- Tag only after the release artifact is validated.

## General Automation Philosophy

- Automate checks that catch real regressions.
- Keep required checks understandable and actionable.
- Prefer fewer high-signal gates over many noisy ones.
- Track known manual validation gaps explicitly.
- CI should make quality cheaper, not bury maintainers in ritual.

## Review Posture

- Review from the perspective of a future maintainer who did not write the code.
- Look for repeated problems across modules, not just isolated defects.
- Prefer concrete findings with file or module references and proposed fixes.
- Prioritize correctness, public API clarity, documentation truthfulness, and maintainability.
- If a module is hard to read, identify whether the problem is concept mixing, poor ordering,
  hidden state, vague names, or too much abstraction.
- If docs are confusing, identify what reader role or entry path is unsupported.

## Implementation Practice

- Read the existing code first.
- Follow the crate's established style where it is good.
- Improve local structure when the existing style is hurting maintainability.
- Keep edits scoped to the owning concept.
- Do not do unrelated refactors.
- Validate with focused checks while working and full checks before declaring completion.
- Call out residual gaps honestly.
