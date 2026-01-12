# ZK Mixer

Private SOL mixer using Groth16 proofs with Light Protocol compressed accounts.

## Summary

- Fixed-denomination SOL mixer enabling private deposits and withdrawals
- Deposit: User generates `secret` + `nullifier`, computes `commitment = Poseidon(nullifier, secret)`, creates compressed account
- Withdraw: User proves knowledge of valid commitment in Light Protocol state tree; nullifier compressed account prevents double-spend
- 26-level Poseidon Merkle tree (Light Protocol state trees)
- On-chain Groth16 verification via `groth16-solana` syscalls
- Circuit verifies commitment account existence using Light Protocol's account hashing scheme

## [README](README.md)

## Source Structure

```
src/
├── lib.rs           # Program entry, instructions (initialize, deposit, withdraw)
└── verifying_key.rs # Groth16 verifying key constants (auto-generated)

circuits/
├── withdraw.circom            # Main circuit: commitment + account hash + Merkle proof
├── compressed_account.circom  # Light Protocol account hash computation
└── merkletree.circom          # Poseidon-based Merkle proof verification
```

## Accounts

### MixerConfig PDA

| Field | Type | Description |
|-------|------|-------------|
| `authority` | `Pubkey` | Pool creator |
| `denomination` | `u64` | Fixed deposit amount in lamports |

**Size**: 8 + 32 + 8 = 48 bytes

### Vault PDA

| Field | Value |
|-------|-------|
| **Seeds** | `[b"vault", mixer_config.key()]` |
| **Purpose** | Holds deposited SOL |

### Compressed Accounts

| Account | Seeds | Fields | Hashing |
|---------|-------|--------|---------|
| `CommitmentAccount` | `[b"commitment", commitment]` | `commitment: [u8; 32]` | SHA256 |
| `NullifierAccount` | `[b"nullifier", nullifier_hash]` | `nullifier_hash: [u8; 32]` | SHA256 |

### Address Derivation

All compressed account addresses derive using `derive_address()` with `ADDRESS_TREE_V2`:

```rust
derive_address(&[seed_prefix, identifier], &address_tree_pubkey, &program_id)
```

## Instructions

| # | Instruction | Accounts | Parameters | Logic |
|---|-------------|----------|------------|-------|
| 0 | `initialize` | `mixer_config` (init), `authority` (signer), `system_program` | `denomination: u64` | Creates MixerConfig PDA |
| 1 | `deposit` | `mixer_config`, `vault` (mut), `depositor` (signer), `system_program`, + Light CPI accounts | `proof`, `address_tree_info`, `output_state_tree_index`, `commitment: [u8; 32]` | Transfers `denomination` SOL to vault, creates CommitmentAccount |
| 2 | `withdraw` | `mixer_config`, `vault` (mut), `recipient` (mut), `payer` (signer), `input_merkle_tree`, `system_program`, + Light CPI accounts | `proof`, `address_tree_info`, `output_state_tree_index`, `input_root_index`, `groth16_proof`, `public_inputs` | Verifies Groth16 proof, creates NullifierAccount, transfers SOL to recipient |

## ZK Circuit (Withdraw)

The circuit uses Light Protocol's account hashing scheme to verify commitment existence in the state tree.

### Account Hash Computation

Light Protocol stores compressed accounts as leaves in a Merkle tree. Each leaf is computed as:

```
leaf = Poseidon(owner_hashed, leaf_index, merkle_tree_hashed, address, discriminator + domain, data_hash)
```

Where:
- `owner_hashed` = `hash_to_bn254_field_size_be(program_id)`
- `merkle_tree_hashed` = `hash_to_bn254_field_size_be(state_tree_pubkey)`
- `discriminator` = 8-byte account type identifier (padded to 32 bytes)
- `domain` = `36893488147419103232` (0x2000000000000000)
- `data_hash` = `Poseidon(commitment)` where `commitment = Poseidon(nullifier, secret)`

### Public Inputs (6)

| # | Signal | Description |
|---|--------|-------------|
| 1 | `owner_hashed` | Hash of program ID (BN254 field) |
| 2 | `merkle_tree_hashed` | Hash of state tree pubkey (BN254 field) |
| 3 | `discriminator` | 8-byte account discriminator (padded to 32 bytes) |
| 4 | `expectedRoot` | Merkle root from state tree |
| 5 | `nullifierHash` | `Poseidon(nullifier)` - prevents double-spend |
| 6 | `recipient_hashed` | Hash of withdrawal address (BN254 field) |

### Private Inputs

| Signal | Description |
|--------|-------------|
| `nullifier` | BN254 field element (248 bits) |
| `secret` | BN254 field element (248 bits) |
| `leaf_index` | Raw leaf index for Merkle path calculation |
| `account_leaf_index` | SDK-formatted leaf index for account hash (32-byte encoded) |
| `address` | Compressed account address |
| `pathElements[26]` | Merkle proof sibling hashes |

### Circuit Flow

1. Compute `commitment = Poseidon(nullifier, secret)`
2. Compute `nullifierHash = Poseidon(nullifier)`
3. Constrain: computed `nullifierHash` equals public input
4. Compute `data_hash = Poseidon(commitment)` (matches LightHasher derive)
5. Compute account hash using `CompressedAccountHash` template with `data_hash`
6. Verify 26-level Merkle proof: computed account hash is leaf, `expectedRoot` is root
7. Bind `recipient_hashed` via quadratic constraint

## Security

| Check | Location | Description |
|-------|----------|-------------|
| Address tree validation | `deposit:66-68`, `withdraw:110-112` | Rejects if `address_tree_pubkey != ADDRESS_TREE_V2` |
| Recipient validation | `withdraw:115-118` | Verifies recipient matches public input |
| Merkle root validation | `withdraw:121-127` | Reads root from on-chain state tree account |
| Groth16 verification | `withdraw:140-167` | Decompresses G1/G2 points, verifies proof |
| Double-spend prevention | `withdraw:170-182` | Creating NullifierAccount at same address fails |
| SOL transfer | `withdraw:184-198` | PDA-signed transfer from vault to recipient |

### Privacy Properties

- Withdrawal is private (ZK proof hides which deposit is being claimed)
- Transaction payer is visible; use a relayer or fresh keypair for full privacy
- Each deposit can only be withdrawn once (nullifier-derived address uniqueness)
- Only the depositor can withdraw (requires knowledge of `nullifier` and `secret`)

## Errors

| Code | Name | Message |
|------|------|---------|
| 6000 | `InvalidRoot` | Invalid merkle root |
| 6001 | `RecipientMismatch` | Recipient mismatch |
| 6002 | `InvalidProof` | Invalid ZK proof |
| 6003 | `InvalidAddressTree` | Invalid address tree |
| 6004 | `AccountNotEnoughKeys` | Not enough keys in remaining accounts |

Additional errors from `groth16-solana`:
- G1/G2 decompression failures
- Proof verification failures

## Nullifier Pattern

The mixer uses nullifiers to prevent double-spending:

1. **Deposit**: User stores `(nullifier, secret)` locally
2. **Withdraw**: User computes `nullifierHash = Poseidon(nullifier)` and proves knowledge in ZK
3. **On-chain**: NullifierAccount created at derived address
4. **Double-spend prevention**: Second withdrawal with same nullifier fails - address already occupied

This leverages Light Protocol's address uniqueness guarantee for compressed accounts.

## Client-Side Workflow

```typescript
// 1. DEPOSIT
const nullifier = randomBytes(31);  // 248 bits
const secret = randomBytes(31);      // 248 bits
const commitment = poseidon([nullifier, secret]);

await program.methods.deposit(
    proof, addressTreeInfo, outputStateTreeIndex, commitment
).rpc();

// Store locally: { nullifier, secret }

// 2. WITHDRAW
const nullifierHash = poseidon([nullifier]);

// Get commitment account and merkle proof
const commitmentAccount = await getCompressedAccount(commitmentAddress);
const merkleProof = await getMerkleProof(commitmentAccount.hash);

// Compute hashed values for circuit
const ownerHashed = hashToBn254(programId);
const merkleTreeHashed = hashToBn254(stateTreePubkey);

const recipientHashed = hashToBn254(recipientAddress);

const { proof } = await snarkjs.groth16.fullProve(
    {
        // Public inputs
        owner_hashed: ownerHashed,
        merkle_tree_hashed: merkleTreeHashed,
        discriminator: commitmentAccount.discriminator,
        expectedRoot: merkleProof.root,
        nullifierHash,
        recipient_hashed: recipientHashed,
        // Private inputs
        nullifier,
        secret,
        leaf_index: commitmentAccount.leafIndex,  // Raw index for Merkle path
        account_leaf_index: commitmentAccount.leafIndexSdkFormat,  // 32-byte encoded
        address: commitmentAccount.address,
        pathElements: merkleProof.siblings,
    },
    "build/withdraw_js/withdraw.wasm",
    "build/withdraw_final.zkey"
);

await program.methods.withdraw(
    validityProof, addressTreeInfo, outputStateTreeIndex,
    inputRootIndex, compressProof(proof), publicInputs
).remainingAccounts([inputMerkleTree]).rpc();
```

## Setup

```bash
# Install dependencies and compile circuit
./scripts/setup.sh

# Build Solana program (generates verifying_key.rs)
cargo build-sbf

# Run tests
cargo test-sbf
```

## Dependencies

- Light Protocol SDK (compression, Merkle trees, CPIs)
- groth16-solana (on-chain proof verification)
- circomlib (Poseidon hash)
- snarkjs (circuit compilation, trusted setup)
