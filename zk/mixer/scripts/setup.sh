#!/bin/bash
set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}======================================${NC}"
echo -e "${BLUE}Mixer ZK Circuit Setup Script${NC}"
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
    echo "Downloading Powers of Tau file (this may take a few minutes)..."

    MAX_RETRIES=3
    RETRY_COUNT=0
    DOWNLOAD_SUCCESS=false

    PTAU_URL="https://storage.googleapis.com/zkevm/ptau/powersOfTau28_hez_final_16.ptau"

    while [ $RETRY_COUNT -lt $MAX_RETRIES ] && [ "$DOWNLOAD_SUCCESS" = "false" ]; do
        RETRY_COUNT=$((RETRY_COUNT + 1))

        if [ $RETRY_COUNT -gt 1 ]; then
            echo "Retry attempt $RETRY_COUNT of $MAX_RETRIES..."
        fi

        curl -L --fail --retry 3 --retry-delay 5 \
             --output "$PTAU_FILE" \
             --progress-bar \
             "$PTAU_URL"

        if [ $? -eq 0 ] && [ -f "$PTAU_FILE" ]; then
            FILE_SIZE=$(stat -f%z "$PTAU_FILE" 2>/dev/null || stat -c%s "$PTAU_FILE" 2>/dev/null || echo "0")
            MIN_SIZE=$((70 * 1024 * 1024))
            MAX_SIZE=$((80 * 1024 * 1024))

            if [ "$FILE_SIZE" -ge "$MIN_SIZE" ] && [ "$FILE_SIZE" -le "$MAX_SIZE" ]; then
                echo -e "${GREEN}✓${NC} Powers of Tau downloaded successfully ($(( FILE_SIZE / 1024 / 1024 )) MB)"
                DOWNLOAD_SUCCESS=true
            else
                echo -e "${YELLOW}Warning: Unexpected file size (got $(( FILE_SIZE / 1024 / 1024 )) MB)${NC}"
                rm -f "$PTAU_FILE"

                if [ $RETRY_COUNT -lt $MAX_RETRIES ]; then
                    echo "Retrying download..."
                    sleep 2
                fi
            fi
        else
            echo -e "${YELLOW}Download attempt failed${NC}"
            rm -f "$PTAU_FILE"

            if [ $RETRY_COUNT -lt $MAX_RETRIES ]; then
                sleep 2
            fi
        fi
    done

    if [ "$DOWNLOAD_SUCCESS" = "false" ]; then
        echo -e "${RED}Error: Failed to download Powers of Tau after $MAX_RETRIES attempts${NC}"
        echo ""
        echo "Please download manually from:"
        echo "https://storage.googleapis.com/zkevm/ptau/powersOfTau28_hez_final_16.ptau"
        echo ""
        echo "Then place the file at: $PTAU_FILE"
        exit 1
    fi
fi
echo ""

# Compile the circuit
echo -e "${BLUE}[5/7]${NC} Compiling circom circuit..."
echo "This may take several minutes depending on circuit complexity..."
circom circuits/withdraw.circom \
    --r1cs \
    --wasm \
    --sym \
    --verbose \
    -o build

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓${NC} Circuit compiled successfully"
    echo "  - Generated: build/withdraw.r1cs"
    echo "  - Generated: build/withdraw_js/withdraw.wasm"
    echo "  - Generated: build/withdraw.sym"
else
    echo -e "${RED}Error: Circuit compilation failed${NC}"
    exit 1
fi
echo ""

# Generate the initial zkey
echo -e "${BLUE}[6/7]${NC} Generating proving key (zkey)..."
echo "Running Groth16 setup (this may take several minutes)..."
npx snarkjs groth16 setup \
    build/withdraw.r1cs \
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
RANDOM_ENTROPY=$(head -c 32 /dev/urandom | xxd -p -c 256)
SYSTEM_ENTROPY="${RANDOM}${RANDOM}${RANDOM}$(date +%s%N)$(uname -a | sha256sum | cut -d' ' -f1)"
COMBINED_ENTROPY="${RANDOM_ENTROPY}${SYSTEM_ENTROPY}"

npx snarkjs zkey contribute \
    build/circuit_0000.zkey \
    build/withdraw_final.zkey \
    --name="First contribution" \
    -v \
    -e="$COMBINED_ENTROPY"

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
    build/withdraw_final.zkey \
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
echo "  - build/withdraw.r1cs"
echo "  - build/withdraw_js/withdraw.wasm"
echo "  - build/withdraw.sym"
echo "  - build/withdraw_final.zkey"
echo "  - build/verification_key.json"
echo ""
echo "Next steps:"
echo "  1. Build the program: ${BLUE}cargo build-sbf${NC}"
echo "  2. Run tests: ${BLUE}cargo test-sbf${NC}"
echo ""
echo "To clean up build artifacts, run: ${BLUE}./scripts/clean.sh${NC}"
echo ""

