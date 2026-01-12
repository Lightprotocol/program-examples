#!/bin/bash
set -e

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}======================================${NC}"
echo -e "${BLUE}ZK Merkle Proof Circuit Setup${NC}"
echo -e "${BLUE}======================================${NC}"
echo ""

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

# Check dependencies
echo -e "${BLUE}[1/7]${NC} Checking dependencies..."
if ! command -v node &> /dev/null; then
    echo -e "${RED}Error: Node.js is not installed${NC}"
    exit 1
fi

if ! command -v circom &> /dev/null; then
    echo -e "${YELLOW}Warning: circom is not installed${NC}"
    npm install -g circom
fi

echo -e "${GREEN}✓${NC} Dependencies OK"
echo ""

# Install npm dependencies
echo -e "${BLUE}[2/7]${NC} Installing npm dependencies..."
npm install
echo -e "${GREEN}✓${NC} Dependencies installed"
echo ""

# Create directories
echo -e "${BLUE}[3/7]${NC} Creating build directories..."
mkdir -p pot
mkdir -p build
echo -e "${GREEN}✓${NC} Directories created"
echo ""

# Download Powers of Tau
echo -e "${BLUE}[4/7]${NC} Downloading Powers of Tau..."
PTAU_FILE="pot/powersOfTau28_hez_final_16.ptau"

if [ -f "$PTAU_FILE" ]; then
    echo -e "${YELLOW}Powers of Tau already exists, skipping${NC}"
else
    PTAU_URL="https://storage.googleapis.com/zkevm/ptau/powersOfTau28_hez_final_16.ptau"
    curl -L --fail --retry 3 --output "$PTAU_FILE" --progress-bar "$PTAU_URL"
    echo -e "${GREEN}✓${NC} Powers of Tau downloaded"
fi
echo ""

# Compile circuit
echo -e "${BLUE}[5/7]${NC} Compiling circuit..."
circom circuits/merkle_proof.circom \
    --r1cs \
    --wasm \
    --sym \
    --verbose \
    -o build

echo -e "${GREEN}✓${NC} Circuit compiled"
echo ""

# Generate zkey
echo -e "${BLUE}[6/7]${NC} Generating proving key..."
npx snarkjs groth16 setup \
    build/merkle_proof.r1cs \
    "$PTAU_FILE" \
    build/circuit_0000.zkey

# Contribute to ceremony
RANDOM_ENTROPY=$(head -c 32 /dev/urandom | xxd -p -c 256)
npx snarkjs zkey contribute \
    build/circuit_0000.zkey \
    build/merkle_proof_final.zkey \
    --name="First contribution" \
    -v \
    -e="$RANDOM_ENTROPY"

echo -e "${GREEN}✓${NC} Proving key generated"
echo ""

# Export verification key
echo -e "${BLUE}[7/7]${NC} Exporting verification key..."
npx snarkjs zkey export verificationkey \
    build/merkle_proof_final.zkey \
    build/verification_key.json

echo -e "${GREEN}✓${NC} Verification key exported"
echo ""

echo -e "${GREEN}======================================${NC}"
echo -e "${GREEN}Setup Complete!${NC}"
echo -e "${GREEN}======================================${NC}"
echo ""
echo "Generated files:"
echo "  - build/merkle_proof.r1cs"
echo "  - build/merkle_proof_js/merkle_proof.wasm"
echo "  - build/merkle_proof_final.zkey"
echo "  - build/verification_key.json"
echo ""
echo "Next: cargo build-sbf && cargo test-sbf"
