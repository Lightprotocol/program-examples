#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$PROJECT_DIR/build"
CIRCUITS_DIR="$PROJECT_DIR/circuits"
POT_DIR="$PROJECT_DIR/pot"

CIRCUIT_NAME="airdropclaim"

echo "=== Anonymous Airdrop Circuit Setup ==="
echo "Project directory: $PROJECT_DIR"

# Create build directory
mkdir -p "$BUILD_DIR"
mkdir -p "$POT_DIR"

# Check for Powers of Tau file
POT_FILE="$POT_DIR/powersOfTau28_hez_final_16.ptau"
if [ ! -f "$POT_FILE" ]; then
    echo "Downloading Powers of Tau file..."
    curl -L -o "$POT_FILE" "https://storage.googleapis.com/zkevm/ptau/powersOfTau28_hez_final_16.ptau"
fi

# Install npm dependencies
cd "$PROJECT_DIR"
if [ ! -d "node_modules" ]; then
    echo "Installing npm dependencies..."
    npm install
fi

# Compile circuit
echo "Compiling circuit..."
circom "$CIRCUITS_DIR/$CIRCUIT_NAME.circom" \
    --r1cs \
    --wasm \
    --sym \
    -o "$BUILD_DIR"

# Get circuit info
echo "Circuit info:"
npx snarkjs r1cs info "$BUILD_DIR/$CIRCUIT_NAME.r1cs"

# Generate initial zkey
echo "Generating initial zkey..."
npx snarkjs groth16 setup \
    "$BUILD_DIR/$CIRCUIT_NAME.r1cs" \
    "$POT_FILE" \
    "$BUILD_DIR/circuit_0000.zkey"

# Contribute to ceremony (for testing - in production use proper MPC)
echo "Contributing to ceremony..."
echo "anonymous-airdrop-contribution" | npx snarkjs zkey contribute \
    "$BUILD_DIR/circuit_0000.zkey" \
    "$BUILD_DIR/${CIRCUIT_NAME}_final.zkey" \
    --name="Anonymous Airdrop Contributor"

# Export verification key
echo "Exporting verification key..."
npx snarkjs zkey export verificationkey \
    "$BUILD_DIR/${CIRCUIT_NAME}_final.zkey" \
    "$BUILD_DIR/verification_key.json"

echo ""
echo "=== Setup Complete ==="
echo "Build artifacts in: $BUILD_DIR"
echo "  - ${CIRCUIT_NAME}.r1cs"
echo "  - ${CIRCUIT_NAME}_js/ (wasm + witness generator)"
echo "  - ${CIRCUIT_NAME}_final.zkey"
echo "  - verification_key.json"
echo ""
echo "Run 'cargo build' to generate verifying_key.rs"

