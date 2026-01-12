#!/bin/bash

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

echo -e "${BLUE}Cleaning ZK circuit build artifacts...${NC}"
echo ""

# Remove build directory
if [ -d "build" ]; then
    echo -e "${YELLOW}Removing build directory...${NC}"
    rm -rf build
    echo -e "${GREEN}✓${NC} build/ removed"
fi

# Remove node_modules (optional, uncomment if needed)
# if [ -d "node_modules" ]; then
#     echo -e "${YELLOW}Removing node_modules...${NC}"
#     rm -rf node_modules
#     echo -e "${GREEN}✓${NC} node_modules/ removed"
# fi

# Remove Powers of Tau (optional, keeps by default since it's reusable)
# if [ -d "pot" ]; then
#     echo -e "${YELLOW}Removing Powers of Tau...${NC}"
#     rm -rf pot
#     echo -e "${GREEN}✓${NC} pot/ removed"
# fi

# Remove individual circuit artifacts if they exist
if [ -f "compressed_account_merkle_proof.r1cs" ]; then
    rm compressed_account_merkle_proof.r1cs
    echo -e "${GREEN}✓${NC} Removed .r1cs file"
fi

if [ -f "compressed_account_merkle_proof.sym" ]; then
    rm compressed_account_merkle_proof.sym
    echo -e "${GREEN}✓${NC} Removed .sym file"
fi

if [ -f "compressed_account_merkle_proof.wasm" ]; then
    rm compressed_account_merkle_proof.wasm
    echo -e "${GREEN}✓${NC} Removed .wasm file"
fi

echo ""
echo -e "${GREEN}Cleanup complete!${NC}"
echo ""
echo "To rebuild the circuit, run: ${BLUE}./scripts/setup.sh${NC}"
echo ""
