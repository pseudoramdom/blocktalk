# BlockTalk

> ⚠️ WARNING: This library is pre-alpha and under active development. APIs may change significantly between versions. Not recommended for production use.

A Rust library for interacting with Bitcoin nodes via IPC.

## Overview
BlockTalk provides a high-level API to connect to `bitcoin-node` process, subscribe to blockchain events, and query information about the blockchain state.

## Development status
- [x] Connect / Disconnect
- [x] Query blockchain data 
- [x] Subscribe to real-time blockchain events
- [ ] Better error handling and logging
- [ ] Testing infrastructure
- [ ] More interfaces

## Setup Guide
### Build Bitcoin Core with Multiprocess Support
First, build Bitcoin Core with multiprocess support enabled using [PR#29409](https://github.com/bitcoin/bitcoin/pull/29409).

```bash
# Clone Bitcoin Core and checkout the PR
git clone https://github.com/bitcoin/bitcoin.git
cd bitcoin
git fetch origin pull/29409/head:pr29409
git checkout pr29409

# Build dependencies with multiprocess support
make -C depends HOST=aarch64-apple-darwin MULTIPROCESS=1 NO_QT=1

# Configure and build Bitcoin Core
export HOST_PLATFORM="aarch64-apple-darwin"
cmake -B multiprocbuild/ --toolchain=depends/$HOST_PLATFORM/toolchain.cmake
cmake --build multiprocbuild/ --parallel $(sysctl -n hw.logicalcpu)
```
For more details on multiprocess Bitcoin, refer to the [documentation](https://github.com/bitcoin/bitcoin/blob/master/doc/multiprocess.md#installation).

### Set Up and Run Bitcoin Node
Create a directory for the node and start the node in regtest mode:

```bash
# Create data directory
mkdir -p datadir_blocktalk

# Start Bitcoin node
./multiprocbuild/src/bitcoin-node \
    -regtest \
    -datadir=$PWD/datadir_blocktalk \
    -server=0 \
    -port=19444 \
    -connect=127.0.0.1:18444 \
    -ipcbind=unix \
    -debug=ipc
```

#### Node Configuration Parameters
- `-regtest`: Use regression test mode (local testing chain)
- `-server=0`: Disable RPC server as we'll use IPC
- `-ipcbind=unix`: Enable Unix domain socket for IPC
- `-debug=ipc`: Enable IPC debugging logs

### Usage

> ⚠️ **Note**: Currently, all BlockTalk code must run inside a `tokio::task::LocalSet`. This is a temporary requirement that will be removed in a future version.

#### Chain queries

```rust
let local = tokio::task::LocalSet::new();
local.run_until(async {
    let blocktalk = BlockTalk::init("/path/to/node.sock").await?;
    let chain = blocktalk.chain();

    // Get current tip
    let (height, hash) = chain.get_tip().await?;
    println!("Current tip: height={}, hash={}", height, hash);

    // Get block at specific height
    let block = chain.get_block(&hash, height - 1).await?;
    println!("Previous block hash: {}", block.block_hash());
}).await
```

#### Chain Monitoring

```rust
use blocktalk::{BlockTalk, NotificationHandler, ChainNotification, BlockTalkError};
use async_trait::async_trait;
use std::sync::Arc;

struct BlockMonitor;

#[async_trait]
impl NotificationHandler for BlockMonitor {
    async fn handle_notification(&self, notification: ChainNotification) -> Result<(), BlockTalkError> {
        match notification {
            ChainNotification::BlockConnected(block) => {
                println!("New block: {}", block.block_hash());
            }
            ChainNotification::TransactionAddedToMempool(tx) => {
                println!("New mempool tx: {}", tx.txid());
            }
            _ => {}
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let local = tokio::task::LocalSet::new();
    
    local.run_until(async {
        let blocktalk = BlockTalk::init("/path/to/node.sock").await?;
        
        // Register handler and subscribe
        blocktalk.chain().register_handler(Arc::new(BlockMonitor)).await?;
        blocktalk.chain().subscribe_to_notifications().await?;

        // Keep running until Ctrl+C
        tokio::signal::ctrl_c().await?;
        Ok(())
    }).await
}
```

#### Block Template Retrieval
```rust
    let local = LocalSet::new();
    local.run_until(async {
        let blocktalk = BlockTalk::init("/path/to/node.sock").await?;

        let block_template_interface = blocktalk.block_template();
        let template = block_template_interface.get_block_template().await?;
    }).await;
```

### Try Out Examples

```bash 
cargo run --example chain_query <NODE_SOCKET_PATH>
```

<details>
<summary> sample output </summary>

```
⏳ Connecting to Bitcoin node...
✅ Connected successfully!

╔════════════════════════════════════════════════════════════════════════════╗
║                              Current Chain Tip                             ║
╠════════════════════════════════════════════════════════════════════════════╣
║ Height │ 267                                                               ║
╟────────┼───────────────────────────────────────────────────────────────────╢
║ Hash   │ 3e6033329b2c77f249afe44b4444b18c133f587684fe84b21071a3653bae051e  ║
╚════════╧═══════════════════════════════════════════════════════════════════╝

╔═════════════════════════════════════════════════════════════════════════════════╗
║                                   Block Details                                 ║
╠═════════════════════════════════════════════════════════════════════════════════╣
║ Hash         │ 3e6033329b2c77f249afe44b4444b18c133f587684fe84b21071a3653bae051e ║
╟──────────────┼──────────────────────────────────────────────────────────────────╢
║ Prev Block   │ 60cda1ced332983c6a399bd22a12852ccd87650f34b51ac3a50384c77c54fdb4 ║
║ Merkle Root  │ 16c58a40955eff72595005a57af39af83450d76c5d932742522198c49b51962f ║
║ Timestamp    │ 1740248760                                                       ║
║ Nonce        │ 0                                                                ║
║ TX Count     │ 1                                                                ║
╟──────────────┴──────────────────────────────────────────────────────────────────╢
║                                 Transactions                                    ║
╠═════════════════════════════════════════════════════════════════════════════════╣
║ TX #1                                                                           ║
║ ├─ TXID      │ 16c58a40955eff72595005a57af39af83450d76c5d932742522198c49b51962f ║
║ ├─ Inputs    │ 1                                                                ║
║ ├─ Outputs   │ 2                                                                ║
║ └─ Sample Out│ 25 BTC satoshis                                                  ║
║     [Coinbase Transaction]                                                      ║
╟─────────────────────────────────────────────────────────────────────────────────╢
║ Block Size   │ 250 bytes                                                        ║
╚═════════════════════════════════════════════════════════════════════════════════╝
```
</details>

```bash 
cargo run --example monitor <NODE_SOCKET_PATH>
```

<details>
<summary> sample output </summary>

```
✅ Connected successfully!
🔍 Monitoring blockchain events. Press Ctrl+C to exit.

╔═════════════════════════════════════════════════════════════════════════════════╗
║                         Transaction Added to Mempool                            ║
╠═════════════════════════════════════════════════════════════════════════════════╣
║ TXID         │ 55c8771b606609f1f6f8d3e15f01bfc1af3c6e43feeb4fd4271adf67a5844115 ║
║ Inputs       │ 1                                                                ║
║ Outputs      │ 1                                                                ║
╚══════════════╧══════════════════════════════════════════════════════════════════╝
```
</details>

```bash 
cargo run --example mempool <NODE_SOCKET_PATH> <TX_ID>
```

<details>
<summary> sample output </summary>

```
✅ Connected successfully!
🔍 Monitoring blockchain events. Press Ctrl+C to exit.

⏳ Connecting to Bitcoin node...
✅ Connected successfully!

╔════════════════════════════════════════════════════════════════════════════╗
║                              Mempool Status                                ║
╠════════════════════════════════════════════════════════════════════════════╣
║ Transaction │ ffcc17d72dec6393e48881bce6c4da4cec4053217016d451cf89bfdb4e5bd3b2  ║
╟────────────┼───────────────────────────────────────────────────────────────────╢
║ Status     │ In Mempool                                                        ║
╚════════════╧═══════════════════════════════════════════════════════════════╝

╔════════════════════════════════════════════════════════════════════════════╗
║                            Transaction Descendants                         ║
╠════════════════════════════════════════════════════════════════════════════╣
║ Transaction │ ffcc17d72dec6393e48881bce6c4da4cec4053217016d451cf89bfdb4e5bd3b2  ║
╟────────────┼───────────────────────────────────────────────────────────────────╢
║ Status     │ No Descendants                                                    ║
╚════════════╧═══════════════════════════════════════════════════════════════╝

╔════════════════════════════════════════════════════════════════════════════╗
║                            Transaction Ancestry                            ║
╠════════════════════════════════════════════════════════════════════════════╣
║ Transaction │ ffcc17d72dec6393e48881bce6c4da4cec4053217016d451cf89bfdb4e5bd3b2  ║
╟────────────┼───────────────────────────────────────────────────────────────────╢
║ Ancestors  │ 1                                                                 ║
║ Descendants│ 1                                                                 ║
║ Size       │ 141 bytes                                                         ║
║ Fees       │ 141 satoshis                                                      ║
╚════════════╧═══════════════════════════════════════════════════════════════╝
```
</details>

The examples expect Bitcoin Core and BlockTalk to be in sibling directories. If you have a different setup, update the `socket_path` in `examples/chain_query.rs`:

## License
MIT License

## Contributing
Contributions are welcome! Please feel free to submit a Pull Request.

## Acknowledgements 
This project is heavily inspired by [@darosior](https://github.com/darosior)'s [prototype](https://github.com/darosior/core_bdk_wallet) which is based on [@ryanofsky](https://github.com/ryanofsky)'s work on [Multiprocess Bitcoin](https://github.com/ryanofsky/bitcoin/blob/pr/ipc/doc/design/multiprocess.md)