# Airdrop Implementations

Simple Implementation: simple-claim -  Distributes compressed tokens that get decompressed on claim

Advanced Implementation: distributor - Distributes SPL tokens, uses compressed PDAs to track claims


## Quick comparison

|  | simple-claim | distributor |
|--|------------------|-------------|
| Time-lock | Slot-based (all-or-nothing) | Timestamp-based (linear vesting) |
| Partial claims | No | Yes |
| Clawback | No | Yes |
| Admin controls | No | Yes |

## Cost (10,000 recipients)

| | simple-claim | distributor | Regular |
|--|----------------:|------------:|----------------:|
| Setup | ~0.03 SOL | ~0.002 SOL | ~0.002 SOL |
| Claim tracking | 0 | ~0.03 SOL | ~6 SOL |
| **Total** | **~0.03 SOL** | **~0.03 SOL** | **~6 SOL** |

## Getting started

- [simple-claim README](./simple-claim/README.md)
- [distributor README](./distributor/README.md)