# Anonymous Airdrop

Claim tokens from an airdrop anonymously using ZK proofs. The claimant proves they're eligible without revealing which address from the eligibility snapshot they own.

## Privacy Guarantee

```
Without ZK: "Address 0xABC claimed 500 tokens to 0xXYZ"
            → Links old wallet to new wallet

With ZK:    "Someone claimed 500 tokens to 0xXYZ"  
            → Can't tell which eligible address owns 0xXYZ
```

## How It Works

1. **Eligibility Snapshot**: Authority creates a Merkle tree of `(eligible_address, amount)` pairs
2. **ZK Proof**: Claimant proves they know a private key for an address in the tree
3. **Nullifier**: Derived from `Poseidon(airdrop_id, private_key)` - prevents double claims
4. **Anonymous Claim**: Tokens transferred to any recipient wallet

## Quick Start

```bash
# Install circuit dependencies
npm install

# Run trusted setup
./scripts/setup.sh

# Build Solana program
cargo build-sbf
```

## Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│ Eligibility     │────▶│ ZK Proof         │────▶│ Anonymous Claim │
│ Merkle Tree     │     │ (Groth16)        │     │ to Fresh Wallet │
└─────────────────┘     └──────────────────┘     └─────────────────┘
        │                       │                        │
        ▼                       ▼                        ▼
  Off-chain data          Client-side             On-chain verification
  (address, amount)       proof generation        + nullifier tracking
```

## Instructions

### initialize_airdrop

Creates a new airdrop with eligibility configuration.

```rust
initialize_airdrop(
    airdrop_id: u64,           // Unique ID
    eligibility_root: [u8; 32], // Merkle root
    unlock_slot: u64,           // Time-lock
)
```

### claim

Claims tokens with ZK proof.

```rust
claim(
    validity_proof: ValidityProof,      // Light Protocol proof
    address_tree_info: PackedAddressTreeInfo,
    output_state_tree_index: u8,
    groth16_proof: CompressedProof,     // ZK proof
    nullifier: [u8; 32],                // Prevents double-claim
    amount: u64,                        // Claim amount
)
```

## Circuit

The `airdrop_claim.circom` circuit verifies:

1. Claimant knows `privateKey` such that `Poseidon(privateKey) = eligibleAddress`
2. `(eligibleAddress, amount)` is in the eligibility Merkle tree
3. `nullifier = Poseidon(airdropId, privateKey)`

Public inputs: `eligibilityRoot`, `nullifier`, `recipient`, `airdropId`, `amount`

## Security

| Property | Mechanism |
|----------|-----------|
| Anonymity | ZK proof hides which eligible address is claiming |
| Double-claim | Nullifier uniqueness via compressed account |
| Time-lock | `current_slot >= unlock_slot` check |
| Front-running | Recipient bound to proof |

## Comparison with Simple Claim

| Feature | Simple Claim | Anonymous Airdrop |
|---------|--------------|-------------------|
| Privacy | None (claimant visible) | Full (claimant hidden) |
| Eligibility | PDA ownership | Merkle proof |
| Double-claim | PDA uniqueness | Nullifier uniqueness |
| Complexity | Simple | ZK circuits required |

## Files

```
anonymous-airdrop/
├── circuits/
│   ├── airdrop_claim.circom    # Main ZK circuit
│   └── merkle_proof.circom     # Merkle verification
├── program/src/
│   ├── lib.rs                  # Program logic
│   ├── error.rs                # Error types
│   └── verifying_key.rs        # Generated VK
├── scripts/
│   └── setup.sh                # Circuit setup
├── typescript/
│   └── client.ts               # Example client
└── CLAUDE.md                   # Detailed documentation
```

## License

MIT

