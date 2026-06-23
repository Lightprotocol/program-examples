# Plan: Port all portable example programs to local light-sdk + light-client

**Date:** 2026-06-23

## IMPORTANT - Autonomous Execution Mode

- This plan executes without user intervention (mode-auto active).
- All questions resolved: proceed even though the test's start-time `light test-validator --stop` will kill any user-run prover on :3001 (the test starts its own stack).
- Use subagents for parallel porting (max 5 concurrent), spawned so they never prompt.
- If blocked, find an alternative approach; do not stop. Skip-and-note only as last resort.
- Keep working until ALL examples are ported, compiled, and (Phase 2) tested green.

## IMPORTANT (user instructions & constraints)

- User instruction: "Plan to port all portable ones, use subagents to do it in parallel up to 5 at a time."
- **Batching is explicitly ALLOWED for this plan** (user requested parallel subagents). This overrides the default no-batching rule.
- Split the work into todos; work through them one wave at a time; do not collapse waves.
- Use subagents to do the per-example porting in parallel, max 5 concurrent.
- If a subagent gets stuck or starts doing random things, it must stop and report rather than improvise; escalate research to another subagent.
- Test EARLY: subagents must get each example to compile before reporting done.
- **Hard constraint — validator is a singleton:** the rewritten tests spawn `light test-validator` (ports 8899/8784/3001) and each run calls `--stop` first. Therefore **only one example's test may run at a time.** Subagents do PORT + COMPILE only (parallel-safe). Test EXECUTION is serialized in a separate phase.
- Do not add Claude as a commit co-author. No emojis. Never index slices with `[i]` in any Rust we write.

## Goal

Switch every portable example from released `light-*` crates + `light-program-test` to:
- local `~/dev/light/light-sdks` path deps via `[patch.crates-io]`,
- `light-client` + a real `light test-validator` for integration tests,
- the anchor-1.0.2 / solana-4.0 dependency stack.

Pinocchio examples (`counter/pinocchio`) are OUT until `light-sdk-pinocchio` is added to the local workspace.

## The canonical recipe (proven on counter/anchor)

### A. Workspace-root `Cargo.toml` (the one with `[workspace]`)
Add:
```toml
[patch.crates-io]
light-sdk = { path = "/Users/jorrit/dev/light/light-sdks/sdk" }
light-hasher = { path = "/Users/jorrit/dev/light/light-sdks/hasher" }
light-client = { path = "/Users/jorrit/dev/light/light-sdks/client" }
```
(Patching these three pulls the whole transitive `light-*` graph from the local workspace.)

### B. Program `Cargo.toml`
- `anchor-lang = "1.0.2"` (anchor examples only; was 0.31.1).
- Enable `keccak` on the SDK so address derivation works at runtime:
  `light-sdk = { version = "0.23.0", features = ["anchor", "cpi-context", "keccak"] }`
  (native examples: same but without `"anchor"`).
- `[dev-dependencies]`: remove `light-program-test`; remove the `solana-sdk` umbrella; use split crates matching the local stack:
  ```toml
  light-client = "0.23.0"
  solana-instruction = "3.4"
  solana-keypair = "3.1.2"
  solana-pubkey = { version = "4.2", features = ["curve25519", "sha2"] }
  solana-signature = "3.4"
  solana-signer = "3.0"
  # solana-keypair -> five8 1.0 -> five8_core 0.1.2 gates `impl Error for DecodeError` behind `std`
  five8_core = { version = "0.1.2", features = ["std"] }
  tokio = "1.49.0"
  ```
  Keep any other existing dev-deps the test genuinely uses.

### C. Program `src/lib.rs` (anchor only)
- anchor 1.0 collapsed `Context` to one lifetime:
  `Context<'_, '_, '_, 'info, T>` -> `Context<'info, T>` (replace all).
- Fix any other anchor-1.0 / borsh-1.x breaks the compiler surfaces (iterate `cargo check`).

### D. Test files (`tests/*.rs`, and native `src/test_helpers.rs`)
- Swap imports: drop `light_program_test::*` and the `solana_sdk` umbrella.
  Use `light_client::{rpc::{LightClient, LightClientConfig, Rpc, RpcError}, indexer::{Indexer, AddressWithTree, CompressedAccount, TreeInfo, ...}}`
  and the split solana crates (`solana_instruction::Instruction`, `solana_keypair::Keypair`, `solana_signature::Signature`, `solana_signer::Signer`).
- Replace `LightProgramTest::new(...)` setup with a `start_validator_and_connect()` helper that:
  - builds the `.so` path: `format!("{}/../../target/deploy/<prog>.so", env!("CARGO_MANIFEST_DIR"))` (adjust `../..` to the workspace target dir for that example),
  - runs `light test-validator --stop` (cleanup), then spawns `light test-validator --sbf-program <ID> <so_path>`,
  - connects `LightClient::new(LightClientConfig::local())`, polls `get_slot()` until ready, then sleeps ~10s.
- Fund a fresh payer: `rpc.airdrop_lamports(&payer.pubkey(), 10_000_000_000).await`.
- Mark every test `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]` (LightClient wraps the blocking RPC client).
- Do NOT add a `--stop` at the END of the test (it kills the test's own process group -> SIGKILL). Rely on the next run's start-time `--stop`.
- Helper fns generic over `R: Rpc + Indexer` need no body changes.

### Reference diff
`counter/anchor` is the worked example. Subagents should read these files as the template:
- `counter/anchor/Cargo.toml` (patch block)
- `counter/anchor/programs/counter/Cargo.toml` (deps)
- `counter/anchor/programs/counter/src/lib.rs` (Context fix)
- `counter/anchor/programs/counter/tests/test.rs` (validator+client test)

## Examples to port

### Anchor (10)
1. `basic-operations/anchor/create`
2. `basic-operations/anchor/update`
3. `basic-operations/anchor/burn`
4. `basic-operations/anchor/close`
5. `basic-operations/anchor/reinit`
6. `read-only`
7. `account-comparison`
8. `create-and-update`
9. `zk/nullifier`
10. `zk/zk-id`  (also has a Noir circuit test `tests/circuit.rs` — leave circuit alone, only port the light test)

### Native (7)
11. `counter/native`
12. `airdrop-implementations/simple-claim`
13. `basic-operations/native/create`
14. `basic-operations/native/update`
15. `basic-operations/native/burn`
16. `basic-operations/native/close`
17. `basic-operations/native/reinit`

Native specifics: no anchor migration; bump `solana-program` to the local 4.0-compatible set; tests build instructions manually (no anchor `InstructionData`/`ToAccountMetas`); a `src/test_helpers.rs` also imports `light_program_test` and must be ported too.

## Execution phases

### Phase 0 — Finalize & prove the recipe on counter (me, sequential) [PREREQUISITE]
- Apply the `keccak` feature fix to `counter/anchor/programs/counter/Cargo.toml`.
- Run `cargo test-sbf` for counter against the validator; confirm `test_counter ... ok`.
- Lock the recipe text above against whatever the run reveals. Only after green do we fan out.

### Phase 1 — Port + compile in parallel (subagents, max 5 concurrent)
Each subagent ports ONE example per the recipe and must achieve BOTH:
- `cargo check --manifest-path <program Cargo.toml> --tests --features test-sbf` green (anchor) / appropriate features (native),
- `cargo build-sbf --manifest-path <program Cargo.toml>` green.
Subagents MUST NOT run `cargo test-sbf` / spawn a validator (port singleton). Report deviations from the recipe.

- Wave A (5): basic-operations/anchor/{create, update, burn, close, reinit}
- Wave B (5): read-only, account-comparison, create-and-update, zk/nullifier, counter/native
- Wave C (5): airdrop-implementations/simple-claim, basic-operations/native/{create, update, burn, close}
- Wave D (2): basic-operations/native/reinit, zk/zk-id

### Phase 2 — Run tests serially (me, one at a time)
For each ported example, run `cargo test-sbf ... -- --nocapture`, confirm the test passes, fix runtime issues (most likely the same keccak/feature class). Never two validators at once.

## Acceptance criteria
- Every listed example: program lib + tests compile against local path deps; `cargo build-sbf` produces a `.so`.
- Each example's integration test passes against `light test-validator` (Phase 2).
- No example still references `light-program-test` or the `solana-sdk` umbrella.
- `[patch.crates-io]` points only at local paths; no released `light-*` crates cross an API boundary.
- Pinocchio examples untouched (documented as blocked).

## STATUS (2026-06-23)

Phase 0: DONE — counter/anchor ported + `test_counter` PASSED with keccak fix. Recipe locked.
Phase 1 (port + compile): DONE for all portable examples. `cargo check --tests` + `cargo build-sbf` green for:
- counter/anchor (also test-passed)
- basic-operations/anchor/{create,update,burn,close,reinit}
- account-comparison (test_solana_account.rs kept on LiteSVM, bumped to 0.12), create-and-update
- read-only, zk/nullifier, zk/zk-id (vendored `read_state_merkle_tree_root`; circuit.rs imports redirected)
- counter/native, basic-operations/native/{create,update,close,reinit,burn}

BLOCKED / not ported:
- airdrop-implementations/simple-claim — depends on the compressed-TOKEN stack (light-token, light-compressed-token-sdk, light-token-types, light-token-interface) which is NOT in the local workspace, and local light-client dropped `get_compressed_token_accounts_by_owner`. REVERTED to released deps (left working).
- counter/pinocchio — needs light-sdk-pinocchio (not local). Untouched.

Phase 2 (run tests): DONE — user authorized; prover killed first. 16 integration-test crates PASS against `light test-validator`:
- counter/anchor (test_counter)
- basic-operations/anchor/{create,update,burn,close,reinit}
- read-only, account-comparison (LightClient + LiteSVM), create-and-update (both files)
- zk/nullifier (2 tests — after adapting the duplicate-rejection check: the real indexer rejects a non-inclusion proof for an existing address at proof-build time)
- counter/native, basic-operations/native/{create,update,close,reinit,burn}

Phase 2 fixes applied:
- native update/close/reinit/burn: `test-sbf -> test-helpers` dragged host-only `light-client`/`solana-keypair` (getrandom 0.2, no solana target) into the SBF build. Moved the 4 test-helpers optional deps under `[target.'cfg(not(target_os = "solana"))'.dependencies]` and gated `pub mod test_helpers` with `not(target_os = "solana")`.
- zk/nullifier: adapted duplicate-rejection assertion (see above).

zk/zk-id: Rust port migrated and `cargo check --tests` GREEN. Its integration test cannot LINK without the native `libcircuit` produced by the example's own `scripts/setup.sh` (node/npm/circom ZK toolchain) — a pre-existing requirement independent of the SDK migration (the released-deps version needs it too). Not run.

## Dependencies / risks
- Requires the local `~/dev/light/light-sdks` workspace to stay buildable (it is).
- Requires `light` CLI + the locally-built `photon` (0.51.2 w/ `--prover-url`) + a prover. User is managing a prover server; Phase 2 must coordinate with whatever prover/validator is running (the test's `--stop` will kill a user-run prover on :3001).
- Each example is its own cargo workspace (separate Cargo.lock/target) — parallel edits touch disjoint paths, so no git/worktree isolation needed.
- `zk/zk-id` is highest-risk (circuit deps); scheduled last/alone.
