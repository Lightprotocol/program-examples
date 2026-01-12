# ZK Compression for ZK Applications

Every transaction on Solana is public. Anyone can see who sent what to whom. This lack of privacy prevents mainstream adoption of many use cases.

Zero knowledge proofs allow to build private applications in Solana programs, such as private transactions, private voting, and private identity verification.

The key building blocks for private Solana programs are: Zero Knowledge Proofs (ZKPs) to prove application logic privately, Poseidon Merkle Trees to store data in a format that can be efficiently proven in a ZK circuit, and Nullifiers to prevent double spending.
Let's dive in.

### Zero Knowledge Proofs
Zero knowledge proofs allow to prove ownership of data and application logic without revealing the data itself.
First you need to select a proof system. Different proof systems trade off proof size, proving time, and setup requirements. Groth16 produces small proofs (256 bytes, compressed 128 bytes) and verifies cheaply onchain, but requires a trusted setup per circuit. Plonk, Halo2, and Plonky3 skip the trusted setup and prove faster, but produce larger proofs. For Solana, Groth16's small proof size and fast verification (~200k compute units) make it the practical choice.
A ZK proof is generated from a circuit - a program that defines what you're proving. Circuits are written in languages like circom or noir. Every circuit has two types of inputs:
1. Private inputs - the secret data only the prover knows.
2. Public inputs - values visible to the verifier that anchor the proof to onchain state.

For example, in a private vote: the private input is your credential, the public input is the Merkle root of the voter registry.

### Poseidon Merkle Tree

A Poseidon Merkle tree is a binary tree where each node is the hash of its children.
Poseidon is designed for ZK circuits - it uses fewer constraints than SHA256, making proofs faster and cheaper to generate.

### Nullifier
A nullifier is a hash derived from your secret and the leaf you're spending.
When you spend, you publish the nullifier. The contract stores it in a set.
If anyone tries to spend the same leaf again, the nullifier would match one already stored, so the transaction fails.
But here's the key: the nullifier reveals nothing about which leaf was spent.
Different secrets produce different nullifiers, so observers can't link a nullifier back to its source leaf.

```
                    OFFCHAIN                                    ONCHAIN

    +------------------+                           +------------------------+
    |  User            |                           |  Solana Program        |
    |                  |       ZK Proof            |                        |
    |  - private key   |  ---------------------->  |  1. verify proof       |
    |  - generates     |                           |  2. check root matches |
    |    ZK proof      |                           |  3. insert nullifier   |
    +------------------+                           |  4. insert new leaf    |
            ^                                      +------------------------+
            |                                           |           |
            | Merkle proof                              v           v
            | (inclusion path)                  +-----------+  +-----------+
            |                                   | Nullifier |  |  State    |
    +------------------+                        | Set       |  |  Tree     |
    |  Indexer         |                        | (spent)   |  | (leaves)  |
    |                  |                        +-----------+  +-----------+
    |  - reads tree    |                                            ^
    |  - serves        |                                            |
    |    Merkle proofs |  <-----------------------------------------+
    +------------------+              reads state
```

The user generates a ZK proof offchain using their private key and a Merkle inclusion path from the indexer, then submits it onchain where the program verifies the proof, checks the root, records the nullifier, and inserts new state.

The indexer is an offchain service that watches the blockchain, maintains a copy of the Merkle tree, and provides Merkle proofs to users. It can be integrated into the client for maximum privacy.

**User Flow:**
1. User stores data as a leaf in the Merkle tree (hashed with Poseidon)
2. Blockchain stores the Merkle root in a Solana account
3. User fetches Merkle proof from the indexer
4. User generates ZK proof offchain (proves they know a leaf without revealing it)
5. User submits proof + nullifier onchain
6. Program verifies the proof against the stored root
7. Program checks the nullifier was not used before
8. Program stores the nullifier (prevents reuse)
9. Program inserts new state as a fresh leaf
10. Indexer updates local copy of Merkle tree

Your actual data stays on your device. Only the proof and nullifier travel onchain.


## Implementation

Design Choices:
1. How to store Merkle tree data onchain?
  - Sparse Merkle tree in Solana account + indexing instruction data
  - Zk compression state Merkle tree + Solana Rpc indexer
2. How to store Nullifiers onchain?
  - create a PDA derived from the nullifier hash (899,000 lamports)
  - create a compressed address derived from the nullifier hash (10k lamports)
3. Client Proof Generation
  - Snarkjs (typescript)
  - mopro (Mobile proving)
  - ark circom (rust)
  

The DIY approach is straightforward but tedious. You write your own Merkle tree program and store the tree onchain. Users call it to insert leaves. You run an indexer offchain to track these transactions and build a local copy of the tree for generating proofs. For nullifiers, you create a PDA derived from the nullifier hash. PDAs can only be created once, so double-spends fail automatically. This works, but you're now maintaining a custom tree program, running your own indexer, and paying for all that storage.

Building a ZK system from scratch is a lot of work. You need a Merkle tree program, nullifier storage, and an indexer to track state. Light Protocol already has all of this.



**1. Compressed Addresses as Nullifiers**

Derive your compressed account address from a nullifier hash. To check if something is spent, try creating an account at that address. If it already exists, it was already spent.

**2. Compressed Accounts as Merkle Leaves**

Store hashed application in compressed accounts. Compressed accounts are Poseidon hashes stored in Poseidon Merkle trees of height 26.
This way you can request your Merkle proofs from Solana RPCs supporting indexing and don't need to index the tree yourself.
Your circuit proves the Compressed account hash and the Merkle proof of the compressed account.

**3. Store Leaves in Compressed Accounts**

Simpler approach: store your Merkle leaf data in the account's data field. You manage your own tree logic, but get Light's indexing and cheap storage for free.

The zk-id example combines both patterns. Credentials are stored as Poseidon-hashed compressed accounts. When used, the nullifier becomes the new account address.

## Putting It Together: zk-id

zk-id is a credential system built on Solana. Issuers create credentials for users. Users prove they hold a valid credential without revealing which one.

The program has three instructions. `create_issuer` registers an issuer with the system. `add_credential` lets an issuer create a credential for a user—stored as a Poseidon-hashed compressed account. `zk_verify_credential` verifies the ZK proof and creates a nullifier account - users prove they own a credential without exposing it.

Here's what happens when you verify. You have your private key. You fetch your credential's Merkle path from an indexer. Your browser generates a ZK proof: "I know a private key whose corresponding credential is in this tree." You also compute a nullifier—hash of your private key plus a verification context. You submit proof and nullifier to the Solana program. The program verifies the proof and creates an event account at an address derived from the nullifier. Try to verify twice? Same nullifier, same address, account already exists, transaction fails.

The nullifier includes a verification context—an event ID, a vote proposal, a claim period. This means the same credential can verify across different contexts. But within any single context, exactly once.

## Tools & Resources

**Circuits**
- **circom** - Domain-specific language for writing ZK constraints
- **circomlib** - Standard library (Poseidon hash, comparators, binary operations)
- **noir** - Rust-like circuit language
- **ark-works** - Rust cryptography library to write circuits among other things.

**Proof Generation & Verification**
- **snarkjs** - Generates proofs from circom circuits in JavaScript
- **circomlibjs** - Offchain implementations of circomlib functions (hash inputs before proving)
- **groth16-solana** - Verifies Groth16 proofs onchain for ~200k compute units

**Light Protocol**
- **light-hasher** - Poseidon/SHA256 matching circuit implementations exactly
- **light-sdk** - Compressed accounts, state trees, address derivation
