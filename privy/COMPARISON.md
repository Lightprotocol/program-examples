# SPL vs Compressed Token Code Comparison

### 1. Setup

**SPL:**

```typescript
import { Connection } from "@solana/web3.js";

const connection = new Connection("https://api.mainnet-beta.solana.com");
```

**Compressed:**

```typescript
import { createRpc } from "@lightprotocol/stateless.js";

// Requires ZK Compression RPC (Helius, Triton)
const rpc = createRpc(process.env.HELIUS_RPC_URL!);
```

---

### 2. Get Balance

**SPL:**

```typescript
import { getAssociatedTokenAddress, getAccount } from "@solana/spl-token";

const ata = await getAssociatedTokenAddress(mintPubkey, ownerPubkey);
const account = await getAccount(connection, ata);
console.log(account.amount);
```

**Compressed:**

```typescript
const accounts = await rpc.getCompressedTokenAccountsByOwner(ownerPubkey, {
  mint: mintPubkey,
});
const validItems = (accounts.items || []).filter(
  (item): item is NonNullable<typeof item> => item !== null
);

// Aggregate balance across all compressed accounts
const balance = validItems.reduce(
  (sum, acc) => sum + BigInt(acc.parsed.amount.toString()),
  0n
);
console.log(balance);
```

---

### 3. Transfer

**SPL:**

```typescript
import { getAssociatedTokenAddress, createTransferInstruction } from "@solana/spl-token";
import { Transaction, PublicKey } from "@solana/web3.js";

const fromAta = await getAssociatedTokenAddress(mintPubkey, fromPubkey);
const toAta = await getAssociatedTokenAddress(mintPubkey, toPubkey);

const instruction = createTransferInstruction(
  fromAta,
  toAta,
  fromPubkey,
  amount
);

const transaction = new Transaction().add(instruction);
const { blockhash } = await connection.getLatestBlockhash();
transaction.recentBlockhash = blockhash;
transaction.feePayer = fromPubkey;
```

**Compressed:**

```typescript
import { createRpc, bn } from "@lightprotocol/stateless.js";
import {
  CompressedTokenProgram,
  selectMinCompressedTokenAccountsForTransfer,
} from "@lightprotocol/compressed-token";
import { Transaction, ComputeBudgetProgram } from "@solana/web3.js";

// 1. Get compressed token accounts
const accounts = await rpc.getCompressedTokenAccountsByOwner(fromPubkey, {
  mint: mintPubkey,
});
const validItems = (accounts.items || []).filter(
  (item): item is NonNullable<typeof item> => item !== null
);

// 2. Select minimum accounts needed
const tokenAmount = bn(amount);
const [inputAccounts] = selectMinCompressedTokenAccountsForTransfer(
  validItems,
  tokenAmount
);

// 3. Get validity proof
const proof = await rpc.getValidityProof(
  inputAccounts.map((acc) => bn(acc.compressedAccount.hash))
);

// 4. Build transfer instruction
const instruction = await CompressedTokenProgram.transfer({
  payer: fromPubkey,
  inputCompressedTokenAccounts: inputAccounts,
  toAddress: toPubkey,
  amount: tokenAmount,
  recentInputStateRootIndices: proof.rootIndices,
  recentValidityProof: proof.compressedProof,
});

// 5. Build transaction with compute budget
const transaction = new Transaction();
transaction.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 300_000 }));
transaction.add(instruction);

const { blockhash } = await rpc.getLatestBlockhash();
transaction.recentBlockhash = blockhash;
transaction.feePayer = fromPubkey;
```

---

### 4. Compress

**SPL:**

N/A - SPL tokens are uncompressed by default.

**Compressed:**

```typescript
import { createRpc, bn, selectStateTreeInfo } from "@lightprotocol/stateless.js";
import {
  CompressedTokenProgram,
  getTokenPoolInfos,
  selectTokenPoolInfo,
} from "@lightprotocol/compressed-token";
import { getAssociatedTokenAddressSync, getAccount } from "@solana/spl-token";
import { Transaction, ComputeBudgetProgram } from "@solana/web3.js";

// 1. Get source token account and verify balance
const ownerAta = getAssociatedTokenAddressSync(mintPubkey, fromPubkey);
const ataAccount = await getAccount(rpc, ownerAta);
const tokenAmount = bn(amount);

if (ataAccount.amount < BigInt(tokenAmount.toString())) {
  throw new Error("Insufficient SPL balance");
}

// 2. Get state tree and token pool info
const stateTreeInfos = await rpc.getStateTreeInfos();
const selectedTreeInfo = selectStateTreeInfo(stateTreeInfos);
const tokenPoolInfos = await getTokenPoolInfos(rpc, mintPubkey);
const tokenPoolInfo = selectTokenPoolInfo(tokenPoolInfos);

// 3. Build compress instruction
const instruction = await CompressedTokenProgram.compress({
  payer: fromPubkey,
  owner: fromPubkey,
  source: ownerAta,
  toAddress: toPubkey,
  mint: mintPubkey,
  amount: tokenAmount,
  outputStateTreeInfo: selectedTreeInfo,
  tokenPoolInfo,
});

// 4. Build transaction with compute budget
const transaction = new Transaction();
transaction.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 300_000 }));
transaction.add(instruction);

const { blockhash } = await rpc.getLatestBlockhash();
transaction.recentBlockhash = blockhash;
transaction.feePayer = fromPubkey;
```

---

### 5. Decompress

**SPL:**

N/A - SPL tokens are already uncompressed.

**Compressed:**

```typescript
import { createRpc } from "@lightprotocol/stateless.js";
import { decompress } from "@lightprotocol/compressed-token";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";

// 1. Get destination ATA
const ownerAta = getAssociatedTokenAddressSync(mintPubkey, fromPubkey);

// 2. Create dummy payer (only publicKey is used)
const dummyPayer = {
  publicKey: fromPubkey,
  secretKey: new Uint8Array(64),
} as any;

// 3. Intercept sendAndConfirmTransaction to use Privy signing
const originalSendAndConfirm = (rpc as any).sendAndConfirmTransaction;
(rpc as any).sendAndConfirmTransaction = async (tx: Transaction, signers: any[]) => {
  const signResult = await privy.wallets().solana().signTransaction(
    process.env.TREASURY_WALLET_ID!,
    {
      transaction: tx.serialize({ requireAllSignatures: false }),
      authorization_context: {
        authorization_private_keys: [process.env.TREASURY_AUTHORIZATION_KEY!],
      },
    }
  );

  const signedTx = signResult.signed_transaction || signResult.signedTransaction;
  const signedTransaction = Buffer.from(signedTx, "base64");
  const signature = await rpc.sendRawTransaction(signedTransaction, {
    skipPreflight: false,
    preflightCommitment: "confirmed",
  });
  await rpc.confirmTransaction(signature, "confirmed");
  return signature;
};

try {
  // 4. Use high-level decompress action
  const signature = await decompress(
    rpc,
    dummyPayer,
    mintPubkey,
    amount,
    dummyPayer,
    ownerAta
  );
} finally {
  // Restore original function
  (rpc as any).sendAndConfirmTransaction = originalSendAndConfirm;
}
```

---

### 6. Sign with Privy

Privy signing works identically for both SPL and compressed:

```typescript
import { PrivyClient } from "@privy-io/node";

const privy = new PrivyClient({
  appId: process.env.PRIVY_APP_ID!,
  appSecret: process.env.PRIVY_APP_SECRET!,
});

const signResult = await privy.wallets().solana().signTransaction(
  process.env.TREASURY_WALLET_ID!,
  {
    transaction: transaction.serialize({ requireAllSignatures: false }),
    authorization_context: {
      authorization_private_keys: [process.env.TREASURY_AUTHORIZATION_KEY!],
    },
  }
);

const signedTx = signResult.signed_transaction || signResult.signedTransaction;
const signedTransaction = Buffer.from(signedTx, "base64");

// Send transaction
const signature = await rpc.sendRawTransaction(signedTransaction, {
  skipPreflight: false,
  preflightCommitment: "confirmed",
});
await rpc.confirmTransaction(signature, "confirmed");
```

---

### 7. Get Transaction History

**SPL:**

```typescript
const signatures = await connection.getSignaturesForAddress(ownerPubkey, {
  limit: 10,
});
```

**Compressed:**

```typescript
const signatures = await rpc.getCompressionSignaturesForTokenOwner(ownerPubkey);

const transactions = await Promise.all(
  signatures.items.slice(0, 10).map(async (sig) => {
    const txInfo = await rpc.getTransactionWithCompressionInfo(sig.signature);
    return {
      signature: sig.signature,
      slot: sig.slot,
      compressionInfo: txInfo?.compressionInfo,
    };
  })
);
```