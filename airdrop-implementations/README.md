# Airdrop Implementations

Simple Implementation: simple-claim - Distributes compressed tokens that get decompressed on claim

Advanced Implementation: distributor - Distributes SPL tokens, uses compressed PDAs to track claims

## Quick comparison

|  | simple-claim | distributor |
|--|--------------|-------------|
| Time-lock | Slot-based (all-or-nothing) | Timestamp-based (linear vesting) |
| Partial claims | No | Yes |
| Clawback | No | Yes |
| Admin controls | No | Yes |

## Cost (100k recipients)

|  | simple-claim | distributor | Regular |
|--|-------------:|------------:|--------:|
| Setup | ~0.5 SOL | ~0.002 SOL | ~0.002 SOL |
| Per-claim | ~1 SOL | ~5 SOL | ~200 SOL |
| **Total** | **~1.5 SOL** | **~5 SOL** | **~200 SOL** |