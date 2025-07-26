
## Requirements
- light-cli install with npm -g install  @lightprotocol/zk-compression-cli

## Build and Test

```bash
# Build the program
cargo build-sbf

# Run tests and see tx
RUST_BACKTRACE=1 cargo test-sbf -- --nocapture
```
