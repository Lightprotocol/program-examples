#!/bin/bash

set -e

echo "Checking formatting..."

# Check formatting for each crate
echo "Checking create-and-update..."
cd create-and-update && cargo fmt --check && cd ..

echo "Checking counter/anchor..."
cd counter/anchor && cargo fmt --check && cd ../..

echo "Checking counter/native..."
cd counter/native && cargo fmt --check && cd ../..

echo "Checking counter/pinocchio..."
cd counter/pinocchio && cargo fmt --check && cd ../..

echo "Checking account-comparison..."
cd account-comparison && cargo fmt --check && cd ..

echo "Checking zk/nullifier..."
cd zk/nullifier && cargo fmt --check && cd ../..

echo "Checking zk/zk-id..."
cd zk/zk-id && cargo fmt --check && cd ../..

echo "Running clippy..."

# Run clippy for each crate
echo "Running clippy on create-and-update..."
cargo clippy --manifest-path create-and-update/Cargo.toml --all-targets --all-features -- -D warnings

echo "Running clippy on counter/anchor..."
cargo clippy --manifest-path counter/anchor/Cargo.toml --all-targets --all-features -- -D warnings

echo "Running clippy on counter/native..."
cargo clippy --manifest-path counter/native/Cargo.toml --all-targets --all-features -- -D warnings

echo "Running clippy on counter/pinocchio..."
cargo clippy --manifest-path counter/pinocchio/Cargo.toml --all-targets --all-features -- -D warnings

echo "Running clippy on account-comparison..."
cargo clippy --manifest-path account-comparison/Cargo.toml --all-targets --all-features -- -D warnings

echo "Running clippy on zk/nullifier..."
cargo clippy --manifest-path zk/nullifier/Cargo.toml --all-targets --all-features -- -D warnings

echo "Running clippy on zk/zk-id..."
cargo clippy --manifest-path zk/zk-id/Cargo.toml --all-targets --all-features -- -D warnings

echo "Lint checks completed successfully!"