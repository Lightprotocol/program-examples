# Compressed Counter Program

This is a counter program with instructions to create a compressed account, increment the accounts counter field and delete the account.

## Build

``
$ anchor build
``

## Test

Requirements:
- light cli version 0.24.0
- solana cli version 2.1.16
- anchor version 0.31.1

1. `$ light test-validator --sbf-program {{program-id}} ./target/deploy/{{rust-name-snake-case}}.so`
2. `$ npm test`

`$ light test-validator` spawns the following background processes:
1. solana test validator `http://127.0.0.1:8899`
2. prover server `http://127.0.0.1:8784`
3. photon indexer `http://127.0.0.1:3001`

You can kill these background processes with `lsof -i:<port>` and `kill <pid>`.


## Disclaimer

Light Protocol programs are audited and deployed on Solana devnet and mainnet.
