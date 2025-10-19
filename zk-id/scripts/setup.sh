#!/bin/bash
set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}======================================${NC}"
echo -e "${BLUE}ZK Circuit Setup Script${NC}"
echo -e "${BLUE}======================================${NC}"
echo ""

# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

# Check if node and npm are installed
echo -e "${BLUE}[1/7]${NC} Checking dependencies..."
if ! command -v node &> /dev/null; then
    echo -e "${RED}Error: Node.js is not installed${NC}"
    echo "Please install Node.js from https://nodejs.org/"
    exit 1
fi

if ! command -v npm &> /dev/null; then
    echo -e "${RED}Error: npm is not installed${NC}"
    exit 1
fi

if ! command -v circom &> /dev/null; then
    echo -e "${YELLOW}Warning: circom is not installed${NC}"
    echo "Installing circom..."
    # Try to install circom via npm
    npm install -g circom
    if ! command -v circom &> /dev/null; then
        echo -e "${RED}Error: Failed to install circom${NC}"
        echo "Please install manually from https://docs.circom.io/getting-started/installation/"
        exit 1
    fi
fi

echo -e "${GREEN}✓${NC} All required tools are installed"
echo ""

# Install npm dependencies
echo -e "${BLUE}[2/7]${NC} Installing npm dependencies..."
npm install
echo -e "${GREEN}✓${NC} Dependencies installed"
echo ""

# Create necessary directories
echo -e "${BLUE}[3/7]${NC} Creating build directories..."
mkdir -p pot
mkdir -p build
echo -e "${GREEN}✓${NC} Directories created"
echo ""

# Download Powers of Tau
echo -e "${BLUE}[4/7]${NC} Downloading Powers of Tau ceremony file..."
PTAU_FILE="pot/powersOfTau28_hez_final_16.ptau"

if [ -f "$PTAU_FILE" ]; then
    echo -e "${YELLOW}Powers of Tau file already exists, skipping download${NC}"
else
    echo "Downloading from Hermez ceremony (this may take a few minutes)..."
    curl -L -o "$PTAU_FILE" \
        https://hermez.s3-eu-west-1.amazonaws.com/powersOfTau28_hez_final_16.ptau

    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓${NC} Powers of Tau downloaded successfully"
    else
        echo -e "${RED}Error: Failed to download Powers of Tau${NC}"
        echo "Please download manually from:"
        echo "https://hermez.s3-eu-west-1.amazonaws.com/powersOfTau28_hez_final_16.ptau"
        exit 1
    fi
fi
echo ""

# Compile the circuit
echo -e "${BLUE}[5/7]${NC} Compiling circom circuit..."
echo "This may take several minutes depending on circuit complexity..."
circom circuits/compressed_account_merkle_proof.circom \
    --r1cs \
    --wasm \
    --sym \
    --verbose \
    -o build

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓${NC} Circuit compiled successfully"
    echo "  - Generated: build/compressed_account_merkle_proof.r1cs"
    echo "  - Generated: build/compressed_account_merkle_proof.wasm"
    echo "  - Generated: build/compressed_account_merkle_proof.sym"
else
    echo -e "${RED}Error: Circuit compilation failed${NC}"
    exit 1
fi
echo ""

# Generate the initial zkey
echo -e "${BLUE}[6/7]${NC} Generating proving key (zkey)..."
echo "Running Groth16 setup (this may take several minutes)..."
npx snarkjs groth16 setup \
    build/compressed_account_merkle_proof.r1cs \
    "$PTAU_FILE" \
    build/circuit_0000.zkey

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓${NC} Initial zkey generated"
else
    echo -e "${RED}Error: zkey generation failed${NC}"
    exit 1
fi

# Contribute to the ceremony
echo ""
echo "Contributing to the ceremony..."
npx snarkjs zkey contribute \
    build/circuit_0000.zkey \
    build/compressed_account_merkle_proof_final.zkey \
    --name="First contribution" \
    -v \
    -e="$(date +%s)"

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓${NC} Contribution complete"
else
    echo -e "${RED}Error: Contribution failed${NC}"
    exit 1
fi
echo ""

# Export verification key
echo -e "${BLUE}[7/7]${NC} Exporting verification key..."
npx snarkjs zkey export verificationkey \
    build/compressed_account_merkle_proof_final.zkey \
    build/verification_key.json

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓${NC} Verification key exported to build/verification_key.json"
else
    echo -e "${RED}Error: Verification key export failed${NC}"
    exit 1
fi
echo ""

# Print summary
echo -e "${GREEN}======================================${NC}"
echo -e "${GREEN}Setup Complete!${NC}"
echo -e "${GREEN}======================================${NC}"
echo ""
echo "Generated files:"
echo "  - build/compressed_account_merkle_proof.r1cs"
echo "  - build/compressed_account_merkle_proof.wasm"
echo "  - build/compressed_account_merkle_proof.sym"
echo "  - build/compressed_account_merkle_proof_final.zkey"
echo "  - build/verification_key.json"
echo ""
echo "Next steps:"
echo "  1. Run Rust tests: ${BLUE}cargo test test_compressed_account_merkle_proof_circuit${NC}"
echo "  2. Generate a proof: Use the circuit with mopro"
echo ""
echo "To clean up build artifacts, run: ${BLUE}./scripts/clean.sh${NC}"
echo ""
