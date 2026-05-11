set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

fmt:
    cargo +nightly fmt --all

fmt-check:
    cargo +nightly fmt --all -- --check

check:
    cargo check --workspace --all-targets --all-features
    cargo check --workspace --all-targets --no-default-features

test:
    cargo test --workspace --all-features
    cargo test --doc --workspace --all-features
    cargo test --examples --workspace --all-features

clippy:
    cargo +stable clippy --workspace --all-features --all-targets -- -D warnings
    cargo +beta clippy --workspace --all-features --all-targets -- -D warnings

doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps

docs-rs:
    RUSTDOCFLAGS="-D warnings" cargo +nightly docs-rs

markdown:
    markdownlint-cli2 README.md "docs/**/*.md"

package:
    cargo package --workspace --allow-dirty

audit:
    cargo audit

machete:
    cargo machete

minimal-versions:
    cargo minimal-versions check --direct --workspace --all-targets --all-features

ci: fmt-check check test clippy doc docs-rs markdown package audit machete minimal-versions

demo-gif:
    mkdir -p target/vhs
    env -u NO_COLOR -u CLICOLOR -u CLICOLOR_FORCE -u FORCE_COLOR vhs tapes/demo.tape
