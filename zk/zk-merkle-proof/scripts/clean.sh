#!/bin/bash

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

echo -e "${BLUE}Cleaning ZK circuit build artifacts...${NC}"

if [ -d "build" ]; then
    rm -rf build
    echo -e "${GREEN}✓${NC} build/ removed"
fi

echo -e "${GREEN}Cleanup complete!${NC}"
echo "To rebuild: ${BLUE}./scripts/setup.sh${NC}"
