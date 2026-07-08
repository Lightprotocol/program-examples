# How to use Zk compression in ZK Solana Programs

Every transaction on Solana is public. This lack of privacy prevents mainstream adoption for many use cases.

Zero knowledge proofs enable privacy in Solana programs, such as private transactions, private voting, and private identity verification.

The key building blocks for zk in Solana programs are:
1. Zero Knowledge Proofs (ZKPs) to prove application logic privately.
2. Poseidon Merkle Trees to store data in a format that can be efficiently proven in a ZK circuit.
3. Nullifiers to prevent double spending.

### Zero Knowledge Proofs
Zero knowledge proofs enable proving ownership of data and application logic without revealing the data itself.
First, select a proof system.
Different proof systems trade off proof size, proving time, and setup requirements. Groth16 produces small proofs (256 bytes, compressed 128 bytes) and verifies cheaply onchain, but requires a trusted setup (TODO: add reference) per circuit.
Other established proof systems avoid trusted setups or prove faster, but produce larger proofs (kilobytes instead of bytes).
For Solana, Groth16's small proof size and fast verification (~200k compute units) make it the practical choice.
A ZK proof is generated from a circuit - a program that defines what you're proving.
Circuits are written in languages like circom or noir. Every circuit has two types of inputs:
1. Private inputs - the secret data only the prover knows.
2. Public inputs - values visible to the verifier that anchor the proof to onchain state.

For example, in a private KYC program, the private input is your credential, the public input is the Merkle root of the identity registry.

### Poseidon Merkle Tree

Merkle trees store zk application state.
Specifically, a Poseidon Merkle tree is a binary tree where each node is the hash of its children.
Poseidon is designed for ZK circuits - it uses fewer constraints (TODO: add reference) than SHA256, making proofs faster and cheaper to generate.

**Merkle trees on Solana:**

A **custom sparse Merkle tree** gives you full control. You design the leaf structure and proof format to match your circuit exactly. Create Solana accounts with your program to store a sparse Merkle tree and its roots.
The tradeoff: you build and run your own indexer to track the tree and serve Merkle proofs.

**Zk compression state Merkle trees** Solana RPCs handle indexing for you and serve Merkle proofs. The Light Protocol programs create and maintain Poseidon state Merkle trees for you in Solana accounts. Once a state Merkle tree fills up the protocol creates a new one. 
The tradeoff: your circuit must prove inclusion of your data inside the compressed account structure. Compressed accounts are stored as hashes in Poseidon Merkle trees with Solana RPC support. This adds constraints to your circuit but RPCs index the Merkle tree for you.

Note, for offchain privacy a user client should fetch a complete (sub)tree not Merkle proof from an indexer. If only onchain privacy is sufficient fetching Merkle proofs from an indexer is more efficient.


### Nullifier
Nullifiers prevent double spending.
In detail, a nullifier is a hash derived from your secret and the leaf the transaction is using.
When you use private state (stored in a Merkle tree leaf), you publish the nullifier. The program stores it in a set.
If anyone tries to spend the same leaf again, the nullifier would match one already stored, so the transaction fails.
The nullifier reveals nothing about which leaf was spent.
Different state produces different nullifiers, so observers can't link a nullifier back to its source leaf.

**Nullifiers on Solana:**

**PDAs** are a straightforward choice. Derive an address from the nullifier hash, create an account there. If the account exists, the nullifier was used. The cost is ~899k lamports per nullifier for rent exemption.

**Compressed addresses** work the same way but cost ~10k lamports. The tradeoff: you need an additional ZK proof to create the account and a CPI to the Light system program. If you're already generating a ZK proof for your application logic, the marginal cost of the extra proof is low. If not, PDAs are simpler.


## Zk Id example

[zk-id](https://github.com/Lightprotocol/program-examples/tree/main/zk-id) is a proof of concept credential system built with zk compression and the following tools:

| Component | Implementation |
|-----------|----------------|
| Merkle leaves | [compressed accounts](https://github.com/Lightprotocol/program-examples/blob/main/zk-id/src/lib.rs#L141) (light-sdk) |
| Nullifiers | [compressed addresses](https://github.com/Lightprotocol/program-examples/blob/main/zk-id/src/lib.rs#L192) (light-sdk) |
| Circuit | [circom](https://github.com/Lightprotocol/program-examples/tree/main/zk-id/circuits) |
| Proof generation | [circom-prover](https://github.com/Lightprotocol/program-examples/blob/main/zk-id/tests/test.rs#L575) (Rust) |
| On-chain verification | [groth16-solana](https://github.com/Lightprotocol/program-examples/blob/main/zk-id/src/lib.rs#L269) |

### Creating a Credential

An issuer registers with the system by calling `create_issuer`. This creates a compressed account storing the issuer's public key and a credential counter.

The issuer then calls `add_credential` for each user. The user generates a credential keypair: a private key (random 248-bit value, sized to fit the BN254 field) and a public key (Poseidon hash of the private key). The issuer creates a compressed account containing:
- The issuer's public key
- The user's credential public key

The account uses Poseidon hashing. This stores the credential as a leaf in a 26-level Merkle tree (supporting ~67 million leaves). The tree root lives onchain. An indexer (a server that indexes Solana transactions, in this case leaves) maintains a full copy of the tree.

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

The address derivation is the double-spend check. Compressed addresses can only be created once (the Light system program rejects duplicates). If the nullifier was already used for this verification_id, the address exists, and the transaction fails.

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

## Resources

**Circuits**
- **circom** - Domain-specific language for writing ZK circuits
- **circomlib** - Standard library (Poseidon hash, comparators, binary operations)
- **noir** - Rust-like circuit language
- **ark-works** - Rust cryptography library for circuits

**Proof Generation & Verification**
- **snarkjs** - Generates proofs from circom circuits in JavaScript
- **circomlibjs** - Offchain implementations of circomlib functions
- **groth16-solana** - Verifies Groth16 proofs onchain (~200k compute units)

**Zk compression**
- **light-hasher** - Poseidon/SHA256 implementations matching circuit behavior
- **light-sdk** - Compressed accounts, state trees, address derivation

## Appendix

1. Compressed Account Hashing:
```
Compressed Account Hash:
+----------------------------------------------------------+
|  Poseidon(                                               |
|    owner_hash,                                           |
|    leaf_index,                                           |
|    merkle_tree_pubkey,                                   |
|    address,                                              |
|    discriminator,                                        |
|    data_hash  <-- developer-defined, hash anything here  |
|  )                                                       |
+----------------------------------------------------------+
```

The `data_hash` is entirely yours. Hash whatever structure your application needs. The outer fields are protocol overhead, but they don't limit what you store inside.
