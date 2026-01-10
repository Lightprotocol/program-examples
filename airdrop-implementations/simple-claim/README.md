# simple-claim

A program to distribute time-locked compressed tokens that get decompressed on claim.

Tokens are minted as compressed tokens to PDAs derived from `[claimant, mint, unlock_slot, bump]`. Recipients claim by decompressing to their SPL token account after `unlock_slot`.

Notes:
- the claimant must be signer
- the unlock_slot must be >= slot
- the PDA must have previously received compressed-tokens, to be able to claim them.

For simple client side distribution visit this example: https://github.com/Lightprotocol/example-token-distribution.

## Documentation

- [AI Assistance Reference](CLAUDE.md)
- [Documentation](https://www.zkcompression.com)

## Build and Test

```bash
cargo build-sbf
```

```bash
cargo test-sbf
```

## Disclaimer

This is a reference implementation, not audited and not ready for production use.