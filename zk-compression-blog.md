# ZK Compression for ZK Applications

Every transaction on Solana is public. Anyone can see who sent what to whom. For many real-world applications, this is a dealbreaker. (TODO: replace dealbreaker with a more appropriate term)

Zero knowledge proofs allow to build private applications in Solana programs, such as private transactions, private voting, and private identity verification.

The key building blocks for private Solana programs are usually Zero Knowledge Proofs (ZKPs),
to prove application logic privately,
Poseidon Merkle Trees to store data so that it is accessible in a Zero Knowledge Proof,
and Nullifiers to prevent double spending.
Let's dive in.

### Zero Knowledge Proofs
Zero knowledge proofs allow to prove ownership of data and application logic without revealing the data itself.
(TODO: add how zk proofs work and what very highlevel tradeoffs exist, we use Groth16, small proof size (128 bytes) it requires a trusted setup other proof systems don't require a trusted setup and or feature faster proof generation at the expense of larger proof sizes (eg Plonk, Plonky3, Halo2, ..))
Every zk proof has 2 parts, circuit to program what to prove, circuit has  public and private inputs.
1. Private inputs, all data the proof is computed over.
2. Public inputs, tie the private inputs to existing application state.

### Poseidon Merkle Tree

A Poseidon Merkle tree is a binary tree where each node is the hash of its children.
Poseidon hash is zk friendly so that we can efficiently prove membership of a leaf in the tree in a zero knowledge proof.

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

You prove you know a valid leaf in the tree without revealing which one. You publish a nullifier that prevents reuse. The contract inserts your new state as a fresh leaf. Throughout this flow, observers see a valid proof and a nullifier—nothing about which leaf you spent or what data it contained.
Privacy is achieved by storing state differently and using a ZKP to prove transaction correctness over private data.
Instead of storing data in a Solana account, you hash it and store only the hash as a leaf in a Poseidon Merkle tree. 
Once stored you can use a ZKP to prove ownership of the data and execute application logic without revealing the data itself.
To ensure that a transaction can only be executed once, you use a nullifier.
The nullifier is a deterministically derived hash of the data hash but unlinkable to the data itself.
The blockchain sees just the tree's root, nullifier and the ZKP.
The actual data stays with the user.


## How ZK Compression Fits

The DIY approach is straightforward but tedious. You write your own Merkle tree program and store the tree onchain. Users call it to insert leaves. You run an indexer offchain to track these transactions and build a local copy of the tree for generating proofs. For nullifiers, you create a PDA derived from the nullifier hash. PDAs can only be created once, so double-spends fail automatically. This works, but you're now maintaining a custom tree program, running your own indexer, and paying for all that storage.

Building a ZK system from scratch is a lot of work. You need a Merkle tree program, nullifier storage, and an indexer to track state. Light Protocol already has all of this.

**Pattern 1: Addresses as Nullifiers**

Derive your compressed account address from a nullifier hash. To check if something is spent, try creating an account at that address. If it already exists, it was already spent.

**Pattern 2: Compressed Accounts as Merkle Leaves**

Light computes each compressed account's hash from six fields: owner, leaf index, tree address, account address, discriminator, and data hash. This hash becomes the leaf in the state Merkle tree. To use this directly, your circuit must compute the identical hash structure using Poseidon. More setup work upfront. But you get native integration with Light's proof system and indexing infrastructure.

**Pattern 3: Store Leaves in Compressed Accounts**

Simpler approach: store your Merkle leaf data in the account's data field. You manage your own tree logic, but get Light's indexing and cheap storage for free.

The zk-id example combines both patterns. Credentials are stored as Poseidon-hashed compressed accounts. When used, the nullifier becomes the new account address.

## Putting It Together: zk-id

zk-id is a credential system built on Solana. Issuers create credentials for users. Users prove they hold a valid credential without revealing which one.

The program has three instructions. `create_issuer` registers an issuer with the system. `add_credential` lets an issuer create a credential for a user—stored as a Poseidon-hashed compressed account. `zk_verify_credential` is where the magic happens: users submit a ZK proof showing they own a credential, without exposing the credential itself.

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
