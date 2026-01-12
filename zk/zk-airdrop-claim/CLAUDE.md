# Anonymous Airdrop - ZK Claim with Privacy

## Summary

- Anonymous token claims using ZK proofs for eligibility verification
- Claimant identity hidden via Merkle proof + nullifier model (Groth16 proofs)
- Claim amounts and recipients are public; only claimant's eligible address is private
- Double-claim prevention via nullifier-derived compressed account addresses
- Time-lock support for scheduled airdrops

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                      Anonymous Airdrop Flow                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  1. SETUP                                                               │
│     Authority creates AirdropConfig PDA with:                           │
│     - Eligibility Merkle root (hash of all (address, amount) pairs)     │
│     - Token vault with airdrop tokens                                   │
│     - Unlock slot for time-gating                                       │
│                                                                         │
│  2. ELIGIBILITY TREE (off-chain)                                        │
│     Merkle tree of: leaf = Poseidon(eligible_address, amount)           │
│     Where: eligible_address = Poseidon(private_key)                     │
│                                                                         │
│  3. CLAIM                                                               │
│     Claimant generates ZK proof proving:                                │
│     - "I know a private key for an address in the eligibility tree"     │
│     - "My nullifier is correctly derived from airdrop_id + private_key" │
│     Submits: proof + nullifier + recipient + amount                     │
│     Creates: NullifierAccount at nullifier-derived address              │
│     Transfers: tokens from vault to recipient                           │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Privacy Properties

| Property | Value |
|----------|-------|
| Claimant identity | Hidden (ZK proof hides which eligible address is claiming) |
| Claim amount | Public (passed as instruction data) |
| Recipient | Public (tokens transferred to visible address) |
| Double-claim prevention | Nullifier-derived address (cryptographic) |
| Trust model | Trustless (Groth16 proofs, no MPC nodes) |

## Source Structure

```text
zk-airdrop-claim/
├── program/src/
│   ├── lib.rs              # Program entry, accounts, instructions
│   ├── error.rs            # AirdropError variants
│   └── verifying_key.rs    # Groth16 verifying key (generated)
├── circuits/
│   ├── airdrop_claim.circom    # Main circuit (5 public inputs)
│   └── merkle_proof.circom     # 20-level Merkle proof
├── scripts/
│   ├── setup.sh            # Circuit compilation + trusted setup
│   └── clean.sh            # Remove build artifacts
├── typescript/
│   └── client.ts           # Example claim client
└── build.rs                # Generates verifying_key.rs from JSON
```

## Accounts

### AirdropConfig PDA

Seeds: `[b"airdrop", airdrop_id.to_le_bytes()]`

| Field | Type | Size | Description |
|-------|------|------|-------------|
| `airdrop_id` | `u64` | 8 | Unique airdrop identifier |
| `authority` | `Pubkey` | 32 | Can deactivate airdrop |
| `mint` | `Pubkey` | 32 | Token mint |
| `eligibility_root` | `[u8; 32]` | 32 | Merkle root of (address, amount) pairs |
| `token_vault` | `Pubkey` | 32 | Vault holding airdrop tokens |
| `unlock_slot` | `u64` | 8 | Slot when tokens unlock |
| `is_active` | `bool` | 1 | Whether airdrop is active |
| `bump` | `u8` | 1 | PDA bump |

### NullifierAccount

Address: `derive_address([b"nullifier", nullifier])`

| Field | Type | Description |
|-------|------|-------------|
| `nullifier` | `[u8; 32]` | `Poseidon(airdrop_id, private_key)` |

## Instructions

### initialize_airdrop

Creates a new airdrop configuration.

| Field | Value |
|-------|-------|
| **instruction_data** | `airdrop_id: u64`, `eligibility_root: [u8; 32]`, `unlock_slot: u64` |
| **accounts** | `airdrop_config` (init PDA), `mint`, `token_vault`, `authority` (signer), `system_program` |

### claim

Claims tokens with ZK proof of eligibility.

| Field | Value |
|-------|-------|
| **instruction_data** | `validity_proof`, `address_tree_info`, `output_state_tree_index`, `groth16_proof`, `nullifier`, `amount` |
| **accounts** | `airdrop_config`, `token_vault`, `recipient_token_account`, `payer` (signer), `token_program`, `system_program`, + Light Protocol remaining accounts |
| **constraints** | `is_active`, `current_slot >= unlock_slot`, valid Groth16 proof |

### deactivate_airdrop

Deactivates the airdrop (authority only).

## Circuit: airdrop_claim.circom

### Public Inputs (5)

| # | Signal | Description |
|---|--------|-------------|
| 1 | `eligibilityRoot` | Merkle root of (address, amount) leaves |
| 2 | `nullifier` | `Poseidon(airdropId, privateKey)` |
| 3 | `recipient` | Recipient address (hashed to BN254) |
| 4 | `airdropId` | Airdrop identifier (hashed to BN254) |
| 5 | `amount` | Token amount (as 32-byte BE) |

### Constraints

```text
1. eligibleAddress = Poseidon(privateKey)
2. leaf = Poseidon(eligibleAddress, amount)
3. nullifier = Poseidon(airdropId, privateKey)
4. MerkleProof(leaf, pathElements, leafIndex) == eligibilityRoot
5. recipientSquare = recipient * recipient  (binds recipient to proof)
```

## Client-Side Proof Generation

```typescript
// 1. Load private key and get Merkle proof from eligibility data
const eligibleAddress = poseidon([privateKey]);
const leaf = poseidon([eligibleAddress, amount]);
const { pathElements, leafIndex } = getMerkleProof(eligibilityTree, leaf);

// 2. Generate nullifier
const nullifier = poseidon([airdropId, privateKey]);

// 3. Generate ZK proof
const { proof } = await snarkjs.groth16.fullProve({
    eligibilityRoot: airdropConfig.eligibilityRoot,
    nullifier,
    recipient: recipientAddress,  // hashed
    airdropId,  // hashed
    amount,
    // Private
    privateKey,
    pathElements,
    leafIndex,
}, wasmPath, zkeyPath);

// 4. Submit claim (recipient can be any address - no link to eligible address)
await program.methods.claim(
    validityProof,
    addressTreeInfo,
    outputStateTreeIndex,
    compressProof(proof),
    nullifier,
    amount
).rpc();
```

## Security

| Property | Mechanism |
|----------|-----------|
| Claimant anonymity | ZK proof hides which eligible address is claiming |
| Eligibility | Address exists in eligibility Merkle tree with correct amount |
| Double-claim prevention | Nullifier-derived compressed account address uniqueness |
| Airdrop binding | Nullifier includes `airdropId` |
| Front-running prevention | Recipient bound to proof |
| Time-lock | `current_slot >= unlock_slot` check |

## Errors

| Code | Name | Cause |
|------|------|-------|
| 6000 | `TokensLocked` | `current_slot < unlock_slot` |
| 6001 | `InvalidProof` | Groth16 verification failed |
| 6002 | `AirdropNotActive` | Airdrop has been deactivated |
| 6003 | `InvalidEligibilityRoot` | Root mismatch |
| 6004 | `InvalidNullifier` | Nullifier computation error |
| 6005 | `InvalidAddressTree` | Wrong Light Protocol address tree |
| 6006 | `AccountNotEnoughKeys` | Missing Light Protocol accounts |

## Setup

```bash
# Install circuit dependencies
npm install

# Run trusted setup (generates verification_key.json)
./scripts/setup.sh

# Build Solana program (generates verifying_key.rs)
cargo build-sbf
```

## Dependencies

- Light Protocol SDK (compression, address derivation, CPIs)
- groth16-solana (on-chain proof verification)
- circomlib (Poseidon, Switcher)
- snarkjs (circuit compilation, trusted setup)

