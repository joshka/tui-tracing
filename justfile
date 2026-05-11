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
    cargo clippy --workspace --all-features --all-targets -- -D warnings

doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps

markdown:
    markdownlint-cli2 README.md "docs/**/*.md"

package:
    cargo package --workspace --allow-dirty

audit:
    cargo audit

machete:
    cargo machete

ci: fmt-check check test clippy doc markdown package audit machete

demo-gif:
    mkdir -p target/vhs
    vhs tapes/demo.tape
