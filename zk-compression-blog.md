# ZK Compression for ZK Applications

Every transaction on Solana is public. This lack of privacy prevents mainstream adoption of many use cases.

Zero knowledge proofs allow to build private applications in Solana programs, such as private transactions, private voting, and private identity verification.

The key building blocks for private Solana programs are: Zero Knowledge Proofs (ZKPs) to prove application logic privately, Poseidon Merkle Trees to store data in a format that can be efficiently proven in a ZK circuit, and Nullifiers to prevent double spending.
Let's dive in.

### Zero Knowledge Proofs
Zero knowledge proofs allow to prove ownership of data and application logic without revealing the data itself.
To start you need to select a proof system. Different proof systems trade off proof size, proving time, and setup requirements. Groth16 produces small proofs (256 bytes, compressed 128 bytes) and verifies cheaply onchain, but requires a trusted setup per circuit. Plonk, Halo2, and Plonky3 skip the trusted setup and prove faster, but produce larger proofs. For Solana, Groth16's small proof size and fast verification (~200k compute units) make it the practical choice.
A ZK proof is generated from a circuit - a program that defines what you're proving. Circuits are written in languages like circom or noir. Every circuit has two types of inputs:
1. Private inputs - the secret data only the prover knows.
2. Public inputs - values visible to the verifier that anchor the proof to onchain state.

For example, in a private KYC program, the private input is your credential, the public input is the Merkle root of the identity registry.

### Poseidon Merkle Tree

A Poseidon Merkle tree is a binary tree where each node is the hash of its children.
Poseidon is designed for ZK circuits - it uses fewer constraints than SHA256, making proofs faster and cheaper to generate.

### Nullifier
A nullifier is a hash derived from your secret and the leaf you're spending.
When you spend, you publish the nullifier. The contract stores it in a set.
If anyone tries to spend the same leaf again, the nullifier would match one already stored, so the transaction fails.
But here's the key: the nullifier reveals nothing about which leaf was spent.
Different secrets produce different nullifiers, so observers can't link a nullifier back to its source leaf.

## Zk Id example

### Creating a Credential

An issuer registers with the system by calling `create_issuer`. This creates a compressed account storing the issuer's public key and a credential counter.

The issuer then calls `add_credential` for each user. The user generates a credential keypair: a private key (random 248-bit value) and a public key (Poseidon hash of the private key). The issuer creates a compressed account containing:
- The issuer's public key
- The user's credential public key

The account uses Poseidon hashing. This stores the credential as a leaf in a 26-level Merkle tree. The tree root lives onchain. The indexer maintains a full copy of the tree.

The credential private key never touches the blockchain. Only the user knows it. The public key is a one-way hash, so even though the credential account is onchain, no one can reverse it to obtain the private key.

**Create Credential:**
```
                    OFFCHAIN                                    ONCHAIN

    +------------------+                           +------------------------+
    |  Issuer          |                           |  Solana Program        |
    |                  |      create_issuer        |                        |
    |  - authority     |  ---------------------->  |  1. validate issuer    |
    |  - signs creds   |                           |  2. create account     |
    +------------------+                           +------------------------+
                                                              |
                                                              v
    +------------------+                           +------------------------+
    |  User            |                           |  Solana Program        |
    |                  |      add_credential       |                        |
    |  - generates     |  ---------------------->  |  1. verify issuer sig  |
    |    keypair       |      (cred_pubkey)        |  2. hash credential    |
    |  - stores        |                           |  3. insert leaf        |
    |    private key   |                           +------------------------+
    +------------------+                                      |
                                                              v
                                                   +------------------------+
    +------------------+                           |  State Tree            |
    |  Indexer         |                           |  (26-level Poseidon)   |
    |                  |      reads state          |                        |
    |  - watches chain |  <----------------------  |  [credential leaves]   |
    |  - builds tree   |                           |                        |
    +------------------+                           +------------------------+

```

The issuer registers once, then creates credentials for users. Each credential is a compressed account containing the issuer's pubkey and the user's credential pubkey. The account is Poseidon-hashed and stored as a leaf in the state tree. The user's private key never touches the chain.

### Verifying a Credential

Verification proves two things: the user knows a credential private key corresponding to a leaf in the tree, and they haven't used that credential in this context before.

The user fetches their credential's Merkle path from the indexer. Their browser computes a nullifier: `Poseidon(verification_id, credential_private_key)`. The verification_id is context-specific, an event ID, a vote proposal, or a claim period.

The ZK circuit takes the private key as private input. Public inputs include the Merkle root, the verification_id, and the nullifier. The circuit verifies:
1. The credential public key derives correctly from the private key
2. The credential exists in the tree (Merkle proof)
3. The nullifier derives correctly from the private key and verification_id

The user submits the proof and nullifier to `zk_verify_credential`. The program verifies the Groth16 proof against the onchain root. It creates an event account at an address derived from the nullifier and verification_id.

The address derivation is the double-spend check. Compressed addresses can only be created once. If the nullifier was already used for this verification_id, the address exists, and the transaction fails.

Same credential, different verification_id means different nullifier, different address. A credential can verify across multiple contexts. Within any single context, exactly once.

**Verify a Credential:**
```
                    OFFCHAIN                                    ONCHAIN

    +------------------+                           +------------------------+
    |  User            |                           |  Solana Program        |
    |                  |       ZK Proof            |                        |
    |  - private key   |  ---------------------->  |  1. verify proof       |
    |  - generates     |                           |  2. check root matches |
    |    ZK proof      |                           |  3. derive address     |
    +------------------+                           |  4. create event acct  |
            ^                                      +------------------------+
            |                                           |           |
            | Merkle proof                              v           v
            | (inclusion path)                  +-----------+  +-----------+
            |                                   | Event     |  |  State    |
    +------------------+                        | Account   |  |  Tree     |
    |  Indexer         |                        | (address= |  | (creds)   |
    |                  |                        |  nullifier)|  +-----------+
    |  - reads tree    |                        +-----------+       ^
    |  - serves        |                                            |
    |    Merkle proofs |  <-----------------------------------------+
    +------------------+              reads state

```

The user fetches a Merkle proof from the indexer, computes a nullifier from their private key and the verification context, and generates a ZK proof. The program verifies the proof and creates an event account at an address derived from the nullifier. If the address already exists, the transaction fails. The credential itself is never revealed.

The indexer watches the blockchain and maintains a local copy of the Merkle tree. Users query it for Merkle proofs. The indexer sees which addresses exist but cannot link them to specific credentials.

## Implementation Design Choices

A zk Solana program similar to zk-id requires a Merkle tree, nullifier storage, and an indexer for the Merkle tree.

**Compressed accounts as credentials**: Store credential data in Poseidon-hashed compressed accounts. The account data becomes a leaf in Zk compression's 26-level Merkle tree. Standard Solana RPCs serve Merkle proofs.

**Compressed addresses as nullifiers**: Derive the event account address from the nullifier hash. Addresses can only be created once. Attempting to create a duplicate fails automatically.

The zk-id program combines both patterns. `add_credential` stores credentials as Poseidon-hashed compressed accounts. `zk_verify_credential` creates event accounts at nullifier-derived addresses.

Design choices for your own ZK Solana Program:
1. Merkle tree storage:
  Light's state trees with RPC indexing, or a custom sparse Merkle tree
2. Nullifier storage: compressed addresses (10k lamports) or PDAs (899k lamports)
3. Proof generation: snarkjs (TypeScript), mopro (mobile), or ark-circom (Rust)

## Tools & Resources

**Circuits**
- **circom** - Domain-specific language for writing ZK constraints
- **circomlib** - Standard library (Poseidon hash, comparators, binary operations)
- **noir** - Rust-like circuit language
- **ark-works** - Rust cryptography library for circuits

**Proof Generation & Verification**
- **snarkjs** - Generates proofs from circom circuits in JavaScript
- **circomlibjs** - Offchain implementations of circomlib functions
- **groth16-solana** - Verifies Groth16 proofs onchain (~200k compute units)

**Light Protocol**
- **light-hasher** - Poseidon/SHA256 implementations matching circuit behavior
- **light-sdk** - Compressed accounts, state trees, address derivation
