# Anonymous Airdrop Circuits

## Overview

These circuits enable anonymous token claims from an airdrop. A claimant can prove they're entitled to tokens from a historical snapshot without revealing which eligible address they own.

## Circuit: `airdrop_claim.circom`

### Public Inputs (5)

| # | Signal | Description |
|---|--------|-------------|
| 1 | `eligibilityRoot` | Merkle root of (address, amount) pairs |
| 2 | `nullifier` | `Poseidon(airdropId, privateKey)` - prevents double-claim |
| 3 | `recipient` | Address receiving tokens (can differ from eligible address) |
| 4 | `airdropId` | Unique identifier for this airdrop |
| 5 | `amount` | Token amount being claimed |

### Private Inputs

| Signal | Description |
|--------|-------------|
| `privateKey` | Claimant's secret (32 bytes, BN254-compatible) |
| `pathElements[20]` | Merkle proof siblings |
| `leafIndex` | Position of leaf in tree |

### Constraints

```text
1. eligibleAddress = Poseidon(privateKey)
2. leaf = Poseidon(eligibleAddress, amount)
3. nullifier = Poseidon(airdropId, privateKey)
4. MerkleProof(leaf, pathElements, leafIndex) == eligibilityRoot
5. recipientSquare = recipient * recipient  (binds recipient to proof)
```

### Merkle Tree Structure

The eligibility Merkle tree is built from (address, amount) pairs:

```
                    Root
                   /    \
                 H01     H23
                /   \   /   \
               L0   L1  L2   L3

Where each leaf Li = Poseidon(eligible_address_i, amount_i)
And each address = Poseidon(private_key_i)
```

## Building

```bash
# Install dependencies
npm install

# Run trusted setup
./scripts/setup.sh
```

## Security Properties

| Property | Mechanism |
|----------|-----------|
| Anonymity | ZK proof hides which eligible address is claiming |
| Double-claim prevention | Nullifier uniqueness (on-chain account at nullifier-derived address) |
| Airdrop binding | Nullifier includes `airdropId` |
| Front-running prevention | Recipient bound to proof |
| Amount integrity | Amount is part of Merkle leaf |

