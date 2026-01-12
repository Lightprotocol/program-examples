#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "Cleaning build artifacts..."
rm -rf "$PROJECT_DIR/build"
rm -rf "$PROJECT_DIR/target"
rm -f "$PROJECT_DIR/program/src/verifying_key.rs"

echo "Clean complete."

