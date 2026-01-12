# Prompt: Generate ZK Identity Protocol Blog Post

## Context
You are writing a technical blog post about a ZK identity protocol. The post is for Solana developers who are familiar with Solana but new to ZK.

## Style
- Paul Graham style: concise, direct, simple language, short sentences, no fluff
- No filler phrases ("Here's the thing", "Let that sink in", etc.)
- No binary contrasts ("Not X. But Y.")
- Trust the reader
- ~800-1000 words total

## Resource
[INSERT RESOURCE HERE: GitHub repo, documentation, or codebase path]

Analyze this resource to understand:
1. How identities/credentials are stored
2. How nullifiers work (if applicable)
3. The proof system used (Groth16, PLONK, etc.)
4. The instruction/function flow
5. How privacy is achieved

## Required Sections

### Section 1: Building Blocks of Privacy (~250 words)
Structure:
- P1 (2-3 sentences): Hook - why public transactions are a problem
- P2 (3-4 sentences): How state is stored in Merkle trees, why this is private (hashing is one-way)
- P3 (3-4 sentences): Brief ZKP explainer - what proof system is used, public inputs vs private, any setup requirements
- P4 (3-4 sentences): Nullifiers - what they are, how they prevent double-spending, why they don't reveal which leaf was spent
- P5: ASCII diagram showing offchain/onchain flow (user, indexer, program, state tree, nullifier set)
- P6 (2-3 sentences): Flow summary

### Section 2: Tools (~100 words)
Structure:
- One intro sentence
- Grouped list of tools used by this protocol:
  - Circuit tools (circom, noir, etc.)
  - Proof generation tools (snarkjs, etc.)
  - Protocol-specific libraries

### Section 3: How [Protocol Name] Works (~200 words)
Structure:
- P1 (4-5 sentences): The naive/DIY approach without this protocol
- P2 (2-3 sentences): What this protocol provides
- List the key patterns/features the protocol enables
- P3 (1-2 sentences): How the example uses these patterns

### Section 4: [Example Name] Walkthrough (~200 words)
Structure:
- P1 (2 sentences): What the example does
- P2 (3-4 sentences): The main instructions/functions
- P3 (4-5 sentences): Step-by-step verification flow
- P4 (2-3 sentences): Context-specific nullifiers or replay protection mechanism

## Output Format
Produce a single markdown file with:
- Title: `# [Protocol Name] for ZK Applications`
- Four sections with `##` headers
- One ASCII diagram in Section 1
- Tool lists with bold tool names
- No code blocks except the diagram
- No emojis

## Process
1. Read the resource thoroughly
2. Extract the key concepts: storage model, proof system, nullifier scheme, user flow
3. Draft each section
4. Review for Paul Graham style compliance
5. Output final markdown
