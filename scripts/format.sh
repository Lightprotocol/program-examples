#!/usr/bin/env bash

set -e

# Format each project individually
cargo +nightly fmt --manifest-path create-and-update/Cargo.toml
cargo +nightly fmt --manifest-path counter/anchor/Cargo.toml
cargo +nightly fmt --manifest-path counter/native/Cargo.toml
cargo +nightly fmt --manifest-path counter/pinocchio/Cargo.toml
cargo +nightly fmt --manifest-path account-comparison/Cargo.toml

# Run clippy on each project individually
cargo clippy --manifest-path create-and-update/Cargo.toml \
        --no-deps \
        --all-features \
        -- -A clippy::result_large_err \
           -A clippy::empty-docs \
           -A clippy::to-string-trait-impl \
           -A unexpected-cfgs \
           -A clippy::doc_lazy_continuation \
        -D warnings

cargo clippy --manifest-path counter/anchor/Cargo.toml \
        --no-deps \
        --all-features \
        -- -A clippy::result_large_err \
           -A clippy::empty-docs \
           -A clippy::to-string-trait-impl \
           -A unexpected-cfgs \
           -A clippy::doc_lazy_continuation \
        -D warnings

cargo clippy --manifest-path counter/native/Cargo.toml \
        --no-deps \
        --all-features \
        -- -A clippy::result_large_err \
           -A clippy::empty-docs \
           -A clippy::to-string-trait-impl \
           -A unexpected-cfgs \
           -A clippy::doc_lazy_continuation \
        -D warnings

cargo clippy --manifest-path counter/pinocchio/Cargo.toml \
        --no-deps \
        --all-features \
        -- -A clippy::result_large_err \
           -A clippy::empty-docs \
           -A clippy::to-string-trait-impl \
           -A unexpected-cfgs \
           -A clippy::doc_lazy_continuation \
        -D warnings

cargo clippy --manifest-path account-comparison/Cargo.toml \
        --no-deps \
        --all-features \
        -- -A clippy::result_large_err \
           -A clippy::empty-docs \
           -A clippy::to-string-trait-impl \
           -A unexpected-cfgs \
           -A clippy::doc_lazy_continuation \
        -D warnings
