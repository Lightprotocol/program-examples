# ZK Examples

Building a private Solana program requires a Merkle tree to store state, a way to track nullifiers, and an indexer to serve Merkle proofs.

You can use Light to:
- Track and store nullifiers rent-free in indexed address Merkle trees
- Store state rent-free in indexed state Merkle trees as compressed accounts

[Learn more in the documentation](https://www.zkcompression.com/zk/overview)

## Examples

**Full Examples:**

- **[zk-id](./zk/zk-id)** - Identity verification using Groth16 proofs. Issuers create credentials; users prove ownership without revealing the credential.

**Basic Examples:**

- **[zk-nullifier](./zk/zk-nullifier)** - Creates one or four nullifiers. Uses Groth16 proofs and compressed accounts.
- **[zk-merkle-proof](./zk/zk-merkle-proof)** - Creates compressed accounts and verifies with Groth16 proofs (without nullifier).