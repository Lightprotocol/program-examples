import { web3, Program, AnchorProvider, setProvider } from "@coral-xyz/anchor";
import { createRpc, Rpc, sleep, confirmTx } from "@lightprotocol/stateless.js";
import * as assert from "assert";
import * as path from "path";
import * as fs from "fs";

const IDL = JSON.parse(
  fs.readFileSync(path.join(process.cwd(), "target/idl/nullifier.json"), "utf8")
);

const PROGRAM_ID = new web3.PublicKey(IDL.address);


describe("nullifier", () => {
  let rpc: Rpc;
  let signer: web3.Keypair;
  let program: Program;

  before(async () => {
    rpc = createRpc(
      "http://127.0.0.1:8899",
      "http://127.0.0.1:8784",
      "http://127.0.0.1:3001",
      { commitment: "confirmed" }
    );

    signer = web3.Keypair.generate();
    await rpc.requestAirdrop(signer.publicKey, web3.LAMPORTS_PER_SOL);
    await sleep(2000);

    const wallet = {
      publicKey: signer.publicKey,
      signTransaction: async (tx: web3.Transaction) => {
        tx.sign(signer);
        return tx;
      },
      signAllTransactions: async (txs: web3.Transaction[]) => {
        txs.forEach((tx) => tx.sign(signer));
        return txs;
      },
    };
    const provider = new AnchorProvider(rpc, wallet as any, { commitment: "confirmed" });
    setProvider(provider);
    program = new Program(IDL, provider);
  });

  function randomBytes32(): Uint8Array {
    return web3.Keypair.generate().publicKey.toBytes();
  }

  describe("Single nullifier", () => {
    it("should create a nullifier", async () => {
      const nullifier = randomBytes32();

      const { data, remainingAccounts } = await createNullifierInstructionData(
        rpc, PROGRAM_ID, [nullifier]
      );
      const ix = await program.methods
        .createNullifier(data, [Array.from(nullifier)])
        .accounts({ signer: signer.publicKey })
        .remainingAccounts(remainingAccounts)
        .instruction();
      const computeIx = web3.ComputeBudgetProgram.setComputeUnitLimit({ units: 200_000 });

      const tx = new web3.Transaction().add(computeIx, ix);
      tx.recentBlockhash = (await rpc.getLatestBlockhash()).blockhash;
      tx.feePayer = signer.publicKey;
      tx.sign(signer);

      const sig = await rpc.sendTransaction(tx, [signer]);
      await confirmTx(rpc, sig);

      console.log("Tx:", sig);

      const slot = await rpc.getSlot();
      await rpc.confirmTransactionIndexed(slot);

      const accounts = await rpc.getCompressedAccountsByOwner(PROGRAM_ID);
      assert.ok(accounts.items.length > 0, "Nullifier account should be created");
    });
  });

  describe("Multiple nullifiers", () => {
    it("should create multiple nullifiers in one transaction", async () => {
      const nullifiers = [randomBytes32(), randomBytes32()];

      const { data, remainingAccounts } = await createNullifierInstructionData(
        rpc, PROGRAM_ID, nullifiers
      );
      const ix = await program.methods
        .createNullifier(data, nullifiers.map((n) => Array.from(n)))
        .accounts({ signer: signer.publicKey })
        .remainingAccounts(remainingAccounts)
        .instruction();
      const computeIx = web3.ComputeBudgetProgram.setComputeUnitLimit({ units: 300_000 });

      const tx = new web3.Transaction().add(computeIx, ix);
      tx.recentBlockhash = (await rpc.getLatestBlockhash()).blockhash;
      tx.feePayer = signer.publicKey;
      tx.sign(signer);

      const sig = await rpc.sendTransaction(tx, [signer]);
      await confirmTx(rpc, sig);

      console.log("Tx:", sig);

      const slot = await rpc.getSlot();
      await rpc.confirmTransactionIndexed(slot);
    });
  });
});

async function createNullifierInstructionData(
  rpc: Rpc,
  programId: web3.PublicKey,
  nullifiers: Uint8Array[]
) {
  const {
    bn,
    deriveAddressSeedV2,
    deriveAddressV2,
    batchAddressTree,
    PackedAccounts,
    SystemAccountMetaConfig,
    defaultTestStateTreeAccounts,
    featureFlags,
    VERSION,
  } = await import("@lightprotocol/stateless.js");

  (featureFlags as any).version = VERSION.V2;

  const NULLIFIER_PREFIX = Buffer.from("nullifier");
  const addressTree = new web3.PublicKey(batchAddressTree);
  const outputStateTree = defaultTestStateTreeAccounts().merkleTree;

  const addressesWithTree = nullifiers.map((nullifier) => {
    const seed = deriveAddressSeedV2([NULLIFIER_PREFIX, nullifier]);
    const address = deriveAddressV2(seed, addressTree, programId);
    return { tree: addressTree, queue: addressTree, address: bn(address.toBytes()) };
  });

  const proofResult = await rpc.getValidityProofV0([], addressesWithTree);

  const remainingAccounts = new PackedAccounts();
  remainingAccounts.addSystemAccountsV2(SystemAccountMetaConfig.new(programId));

  const addressMerkleTreeIndex = remainingAccounts.insertOrGet(addressTree);
  const outputStateTreeIndex = remainingAccounts.insertOrGet(outputStateTree);

  const { remainingAccounts: accountMetas, systemStart } = remainingAccounts.toAccountMetas();

  const data = {
    proof: { 0: proofResult.compressedProof },
    addressTreeInfo: {
      addressMerkleTreePubkeyIndex: addressMerkleTreeIndex,
      addressQueuePubkeyIndex: addressMerkleTreeIndex,
      rootIndex: proofResult.rootIndices[0],
    },
    outputStateTreeIndex,
    systemAccountsOffset: systemStart,
  };

  return { data, remainingAccounts: accountMetas };
}