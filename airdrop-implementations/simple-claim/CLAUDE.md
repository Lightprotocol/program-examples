# Compressed Claim

Claims time-locked compressed tokens and decompresses them to an SPL token account.

## [README](README.md)

## Source Structure

```text
program/src/
├── lib.rs           # entrypoint, declare_id!
├── error.rs         # ClaimError (3 variants)
├── instruction.rs   # ClaimIxData, ClaimAccounts, builders
└── processor.rs     # process_claim logic
```

## Accounts

### Airdrop PDA

Seeds: `[claimant.to_bytes(), mint.to_bytes(), unlock_slot.to_le_bytes(), bump]`

Derived via `Pubkey::create_program_address`. Holds compressed tokens until claim.

### ClaimIxData (Instruction Data)

| Field | Type | Description |
|-------|------|-------------|
| proof | ValidityProof | ZK proof for compressed account |
| packed_tree_info | PackedStateTreeInfo | Merkle tree state info |
| amount | u64 | Tokens to claim |
| lamports | Option<u64> | Optional lamports to transfer |
| mint | Pubkey | Token mint address |
| unlock_slot | u64 | Slot when tokens unlock |
| bump_seed | u8 | PDA bump seed |

## Instructions

| Instruction | Path | Accounts | Logic |
|-------------|------|----------|-------|
| Claim | processor.rs | claimant (signer), fee_payer (signer), airdrop_pda, + 13 Light/CToken accounts | Verifies unlock_slot <= current_slot, validates PDA, invokes decompress CPI |

### Claim Accounts (16 total)

0. claimant (signer)
1. fee_payer (signer)
2. associated_airdrop_pda
3. ctoken_cpi_authority_pda
4. light_system_program
5. registered_program_pda
6. noop_program
7. account_compression_authority
8. account_compression_program
9. ctoken_program
10. spl_interface_pda (token_pool)
11. decompress_destination (writable)
12. token_program
13. system_program
14. state_tree (writable)
15. queue (writable)

## Key Concepts

**Time-lock**: Tokens unlock at `unlock_slot`. Claim fails if `current_slot < unlock_slot`.

**PDA Derivation**: `[claimant, mint, unlock_slot, bump]` - ensures only rightful claimant can claim.

**Decompress CPI**: Invokes `light_ctoken_sdk::decompress` to convert compressed tokens to SPL.

## Security

**Signer checks**: Both claimant and fee_payer must sign.

**PDA validation**: Derived PDA must match provided airdrop_pda account.

**Program check**: ctoken_program must equal `light_ctoken_sdk::ctoken::id()`.

## Errors

| Error | Description |
|-------|-------------|
| MissingRequiredSignature | Claimant or fee_payer not signer |
| TokensLocked | current_slot < unlock_slot |
| InvalidPDA | Derived PDA doesn't match provided account |
