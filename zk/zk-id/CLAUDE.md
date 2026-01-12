# ZK-ID Program

Zero-knowledge identity verification using Groth16 proofs with compressed accounts.

## Summary

- Issuers create credentials for users; users prove credential ownership without revealing the credential
- Credential keypair: private key = `Sha256(sign("CREDENTIAL"))` truncated to 248 bits; public key = `Poseidon(private_key)`
- Nullifier = `Poseidon(verification_id, credential_private_key)` - prevents double-use per verification context
- ZK circuit verifies 26-level Merkle proof of credential account inclusion

## [README](README.md)

## Source Structure

```
src/
├── lib.rs           # Program entry, instructions, account structs, error codes
└── verifying_key.rs # Groth16 verifying key constants (8 public inputs)

circuits/
├── compressed_account_merkle_proof.circom  # Main circuit (26-level Merkle proof)
├── compressed_account.circom               # CompressedAccountHash template
├── credential.circom                       # Keypair and CredentialOwnership templates
└── merkle_proof.circom                     # MerkleProof template
```

## Accounts

### Compressed Accounts (Light Protocol)

| Account | Seeds | Fields | Hashing |
|---------|-------|--------|---------|
| `IssuerAccount` | `[b"issuer", signer_pubkey]` | `issuer_pubkey: Pubkey`, `num_credentials_issued: u64` | SHA256 |
| `CredentialAccount` | `[b"credential", credential_pubkey]` | `issuer: Pubkey` (#[hash]), `credential_pubkey: CredentialPubkey` | Poseidon |
| `EncryptedEventAccount` | `[b"ZK_ID_CHECK", nullifier, verification_id]` | `data: Vec<u8>` | SHA256 |

### Anchor Accounts

| Struct | Fields |
|--------|--------|
| `GenericAnchorAccounts` | `signer: Signer` (mut) |
| `VerifyAccounts` | `signer: Signer` (mut), `input_merkle_tree: UncheckedAccount` |

### Address Derivation

All addresses derive using `derive_address()` with `ADDRESS_TREE_V2`:

```rust
derive_address(&[seed_prefix, identifier], &address_tree_pubkey, &program_id)
```

## Instructions

| # | Instruction | Accounts | Parameters | Logic |
|---|-------------|----------|------------|-------|
| 0 | `create_issuer` | `GenericAnchorAccounts` + CPI accounts | `proof`, `address_tree_info`, `output_state_tree_index` | Derives address from `[ISSUER, signer]`, creates `IssuerAccount` with `num_credentials_issued = 0` |
| 1 | `add_credential` | `GenericAnchorAccounts` + CPI accounts | `proof`, `address_tree_info`, `output_state_tree_index`, `issuer_account_meta`, `credential_pubkey`, `num_credentials_issued` | Mutates issuer (increments counter), derives address from `[CREDENTIAL, credential_pubkey]`, creates `CredentialAccount` |
| 2 | `zk_verify_credential` | `VerifyAccounts` + CPI accounts | `proof`, `address_tree_info`, `output_state_tree_index`, `input_root_index`, `public_data`, `credential_proof`, `issuer`, `nullifier`, `verification_id` | Reads Merkle root, constructs 8 public inputs, decompresses G1/G2 points, verifies Groth16 proof, creates `EncryptedEventAccount` |

## ZK Circuit (CompressedAccountMerkleProof)

**Public inputs** (8 signals):
1. `owner_hashed` - Program ID hashed to BN254 field
2. `merkle_tree_hashed` - State tree pubkey hashed to BN254 field
3. `discriminator` - 8-byte account discriminator
4. `issuer_hashed` - Issuer pubkey hashed to BN254 field
5. `expectedRoot` - Merkle tree root
6. `verification_id` - 31-byte external context
7. `public_encrypted_data_hash` - SHA256 of encrypted data (first byte zeroed)
8. `nullifier` - Prevents double-spending

**Private inputs**:
- `credentialPrivateKey` - User's credential secret
- `leaf_index`, `account_leaf_index`, `address` - Account position
- `pathElements[26]` - Merkle proof
- `encrypted_data_hash` - Must match public input

**Circuit flow**:
1. Derive `credential_pubkey = Poseidon(privateKey)` via `Keypair` template
2. Verify `nullifier = Poseidon(verification_id, privateKey)`
3. Compute `data_hash = Poseidon(issuer_hashed, credential_pubkey)`
4. Compute account hash via `CompressedAccountHash` (adds discriminator domain `+36893488147419103232`)
5. Verify 26-level Merkle proof against `expectedRoot`
6. Verify `public_encrypted_data_hash === encrypted_data_hash`

### Compressed Account Hash

The circuit computes:
```
Poseidon(owner_hashed, leaf_index, merkle_tree_hashed, address, discriminator + 0x2000000000000000, data_hash)
```

The `+0x2000000000000000` (36893488147419103232) sets byte 23 to `0x02` for domain separation.

## Security

| Check | Location | Description |
|-------|----------|-------------|
| Address tree validation | `create_issuer:60-63`, `add_credential:130-133`, `zk_verify_credential:187-190` | Rejects if `address_tree_pubkey != ADDRESS_TREE_V2` |
| Issuer authorization | `add_credential:111-118` | Reconstructs `IssuerAccount` with signer as `issuer_pubkey`; CPI fails if hash mismatch |
| Counter overflow | `add_credential:121-124` | Uses `checked_add()` for `num_credentials_issued` |
| Groth16 verification | `zk_verify_credential:269-284` | Decompresses G1/G2 points, creates `Groth16Verifier`, calls `verify()` |
| Merkle tree owner/discriminator | `zk_verify_credential:203-207` | Reads root via `read_state_merkle_tree_root()` which validates account owner and discriminator |

### Privacy Properties

- Credential verification is private (credential not exposed during proof verification)
- Transaction payer is visible; use a relayer or fresh keypair for full privacy
- Each credential can only be used once per `verification_id` (event account address acts as nullifier)
- Only credential owner can produce a valid proof (requires `credentialPrivateKey`)

## Errors

| Code | Name | Message |
|------|------|---------|
| `InvalidIssuer` | 6000 | Invalid issuer: signer is not the issuer of this account |
| `AccountNotEnoughKeys` | 6001 | Not enough keys in remaining accounts |

Additional errors from `groth16-solana` (returned as `ProgramError::Custom(code)`):
- G1/G2 decompression failures
- Proof verification failures
