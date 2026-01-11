# Airdrop Implementations

Simple Implementation: [simple-claim](./simple-claim) - Distributes compressed tokens that get decompressed on claim

Advanced Implementation: [distributor](./distributor) - Distributes SPL tokens, uses compressed PDAs to track claims. Based on jito Merkle distributor.

## Quick comparison

|  | simple-claim | distributor |
|--|--------------|-------------|
| Vesting | Cliff at Slot X	 | Linear Vesting |
| Partial claims | No | Yes |
| Clawback | No | Yes |
| Admin controls | No | Yes |

## Cost

|                          |    Per-claim | 100k claims |
|--------------------------|-------------:|------------:|
| simple-claim             | ~0.00001 SOL |      ~1 SOL |
| distributor (compressed) | ~0.00005 SOL |      ~5 SOL |
| distributor (original)   |   ~0.002 SOL |    ~200 SOL |
