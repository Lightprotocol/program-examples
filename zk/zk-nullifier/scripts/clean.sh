#!/bin/bash
set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

echo "Cleaning build artifacts..."
rm -rf build/
rm -rf node_modules/
rm -rf target/
rm -f package-lock.json

echo "Clean complete."

