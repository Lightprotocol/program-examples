#!/bin/bash

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Cleaning mixer build artifacts...${NC}"

# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

# Remove build directory
if [ -d "build" ]; then
    rm -rf build
    echo "  - Removed build/"
fi

# Remove node_modules
if [ -d "node_modules" ]; then
    rm -rf node_modules
    echo "  - Removed node_modules/"
fi

# Remove pot directory (but keep the ptau file if it exists)
if [ -d "pot" ]; then
    # Only remove the pot directory if you want to re-download ptau
    # Uncomment the next line to also remove the Powers of Tau file
    # rm -rf pot
    echo "  - Keeping pot/ (contains Powers of Tau file)"
fi

# Remove Rust target directory
if [ -d "target" ]; then
    rm -rf target
    echo "  - Removed target/"
fi

# Remove generated verifying key (it will be regenerated)
if [ -f "src/verifying_key.rs" ]; then
    # Keep the placeholder, don't delete
    echo "  - Keeping src/verifying_key.rs (placeholder)"
fi

# Remove package-lock.json
if [ -f "package-lock.json" ]; then
    rm -f package-lock.json
    echo "  - Removed package-lock.json"
fi

echo ""
echo -e "${GREEN}Clean complete!${NC}"
echo ""
echo "To rebuild, run: ./scripts/setup.sh"

