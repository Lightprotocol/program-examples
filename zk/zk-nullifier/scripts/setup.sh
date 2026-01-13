#!/bin/bash
set -e

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}======================================${NC}"
echo -e "${BLUE}ZK Nullifier Circuit Setup${NC}"
echo -e "${BLUE}======================================${NC}"
echo ""

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

echo -e "${BLUE}[1/9]${NC} Checking dependencies..."
if ! command -v node &> /dev/null; then
    echo -e "${RED}Error: Node.js is not installed${NC}"
    exit 1
fi

if ! command -v circom &> /dev/null; then
    echo -e "${YELLOW}Warning: circom is not installed${NC}"
    echo "Installing circom..."
    npm install -g circom
fi

echo -e "${GREEN}✓${NC} Dependencies OK"
echo ""

echo -e "${BLUE}[2/9]${NC} Installing npm dependencies..."
npm install
echo -e "${GREEN}✓${NC} Dependencies installed"
echo ""

echo -e "${BLUE}[3/9]${NC} Creating build directories..."
mkdir -p pot
mkdir -p build
echo -e "${GREEN}✓${NC} Directories created"
echo ""

echo -e "${BLUE}[4/9]${NC} Downloading Powers of Tau..."
PTAU_FILE="pot/powersOfTau28_hez_final_14.ptau"

if [ -f "$PTAU_FILE" ]; then
    echo -e "${YELLOW}Powers of Tau file exists, skipping${NC}"
else
    PTAU_URL="https://storage.googleapis.com/zkevm/ptau/powersOfTau28_hez_final_14.ptau"
    curl -L --fail --retry 3 --retry-delay 5 \
         --output "$PTAU_FILE" \
         --progress-bar \
         "$PTAU_URL"
    echo -e "${GREEN}✓${NC} Downloaded"
fi
echo ""

# Single nullifier circuit
echo -e "${BLUE}[5/9]${NC} Compiling nullifier.circom..."
circom circuits/nullifier_main.circom \
    --r1cs --wasm --sym -o build
echo -e "${GREEN}✓${NC} Single circuit compiled"
echo ""

echo -e "${BLUE}[6/9]${NC} Generating nullifier zkey..."
npx snarkjs groth16 setup \
    build/nullifier.r1cs \
    "$PTAU_FILE" \
    build/nullifier_0000.zkey

RANDOM_ENTROPY=$(head -c 32 /dev/urandom | xxd -p -c 256)
npx snarkjs zkey contribute \
    build/nullifier_0000.zkey \
    build/nullifier_final.zkey \
    --name="contribution" -v -e="$RANDOM_ENTROPY"

npx snarkjs zkey export verificationkey \
    build/nullifier_final.zkey \
    build/verification_key.json
echo -e "${GREEN}✓${NC} Single nullifier zkey done"
echo ""

# Batch nullifier circuit (4 nullifiers)
echo -e "${BLUE}[7/9]${NC} Compiling batchnullifier.circom..."
circom circuits/batchnullifier.circom \
    --r1cs --wasm --sym -o build
echo -e "${GREEN}✓${NC} Batch circuit compiled"
echo ""

echo -e "${BLUE}[8/9]${NC} Generating batchnullifier zkey..."
npx snarkjs groth16 setup \
    build/batchnullifier.r1cs \
    "$PTAU_FILE" \
    build/batchnullifier_0000.zkey

RANDOM_ENTROPY=$(head -c 32 /dev/urandom | xxd -p -c 256)
npx snarkjs zkey contribute \
    build/batchnullifier_0000.zkey \
    build/batchnullifier_final.zkey \
    --name="contribution" -v -e="$RANDOM_ENTROPY"

npx snarkjs zkey export verificationkey \
    build/batchnullifier_final.zkey \
    build/batch_verification_key.json
echo -e "${GREEN}✓${NC} Batch nullifier zkey done"
echo ""

echo -e "${BLUE}[9/9]${NC} Cleanup intermediate files..."
rm -f build/nullifier_0000.zkey build/batchnullifier_0000.zkey
echo -e "${GREEN}✓${NC} Cleanup done"
echo ""

echo -e "${GREEN}======================================${NC}"
echo -e "${GREEN}Setup Complete!${NC}"
echo -e "${GREEN}======================================${NC}"
echo ""
echo "Single nullifier:"
echo "  - build/nullifier_final.zkey"
echo "  - build/verification_key.json"
echo ""
echo "Batch nullifier (4x):"
echo "  - build/batchnullifier_final.zkey"
echo "  - build/batch_verification_key.json"
echo ""
echo "Next: cargo build-sbf && cargo test-sbf"
