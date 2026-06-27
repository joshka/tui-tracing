set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

fmt *args:
    cargo +nightly fmt --all {{ args }}

fmt-check *args:
    cargo +nightly fmt --all -- --check {{ args }}

check *args:
    cargo check --workspace --all-targets --all-features {{ args }}
    cargo check --workspace --all-targets --no-default-features {{ args }}

test *args:
    cargo test --workspace --all-features {{ args }}
    cargo test --doc --workspace --all-features {{ args }}
    cargo test --examples --workspace --all-features {{ args }}

clippy *args:
    cargo +stable clippy --workspace --all-features --all-targets {{ args }} -- -D warnings
    cargo +beta clippy --workspace --all-features --all-targets {{ args }} -- -D warnings

doc *args:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps {{ args }}

docs-rs *args:
    RUSTDOCFLAGS="-D warnings" cargo +nightly docs-rs {{ args }}

markdown *args:
    markdownlint-cli2 README.md "docs/**/*.md" {{ args }}

package *args:
    cargo package --workspace --allow-dirty {{ args }}

audit *args:
    cargo audit {{ args }}

deny *args:
    cargo deny check {{ args }}

machete *args:
    cargo machete {{ args }}

minimal-versions *args:
    cargo minimal-versions check --direct --workspace --all-targets --all-features {{ args }}

workflow-lint *args:
    actionlint -color=false .github/workflows/*.yml {{ args }}
    zizmor .github/workflows {{ args }}

typos *args:
    typos {{ args }}

ci: fmt-check check test clippy doc docs-rs markdown workflow-lint typos package audit deny machete minimal-versions

demo-gif *args:
    mkdir -p target/vhs
    env -u NO_COLOR -u CLICOLOR -u CLICOLOR_FORCE -u FORCE_COLOR vhs tapes/demo.tape {{ args }}
