use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::chain_capnp::chain::Client as ChainClient;
use crate::init_capnp::init::Client as InitClient;
use crate::proxy_capnp::thread::Client as ThreadClient;
use crate::BlockTalkError;
use crate::mining_capnp::block_template::Client as BlockTemplateClient;

/// Represents a connection to the Bitcoin node
pub struct Connection {
    rpc_handle: JoinHandle<Result<(), capnp::Error>>,
    disconnector: capnp_rpc::Disconnector<twoparty::VatId>,
    thread: ThreadClient,
    chain_client: ChainClient,
    block_template_client: BlockTemplateClient
}

impl Connection {
    /// Create a new connection to the Bitcoin node
    pub async fn connect(socket_path: &str) -> Result<Arc<Self>, BlockTalkError> {
        log::info!("Connecting to Bitcoin node at {}", socket_path);

        let stream = tokio::net::UnixStream::connect(socket_path).await?;
        log::debug!("Unix stream connected successfully");
        let (reader, writer) = stream.into_split();

        log::debug!("Setting up RPC network");
        let network = Box::new(twoparty::VatNetwork::new(
            reader.compat(),
            writer.compat_write(),
            rpc_twoparty_capnp::Side::Client,
            Default::default(),
        ));
        let mut rpc = RpcSystem::new(network, None);
        let init_interface: InitClient = rpc.bootstrap(rpc_twoparty_capnp::Side::Server);
        let disconnector = rpc.get_disconnector();

        log::debug!("Spawning RPC task");
        let rpc_handle = tokio::task::spawn_local(rpc);

        // Get thread client
        let mk_init_req = init_interface.construct_request();
        let response = mk_init_req.send().promise.await?;

        let thread_map = response.get()?.get_thread_map()?;

        let mk_thread_req = thread_map.make_thread_request();
        let response = mk_thread_req.send().promise.await?;

        let thread = response.get()?.get_result()?;
        log::debug!("Thread client established");

        // Set up chain client with thread context
        let mut mk_chain_req = init_interface.make_chain_request();
        {
            let mut context = mk_chain_req.get().get_context()?;
            context.set_thread(thread.clone());
        }
        let response = mk_chain_req.send().promise.await?;

        let chain_client = response.get()?.get_result()?;
        log::debug!("Chain client established");

        // Set up block template client with thread context
        let mut mk_mining_req = init_interface.make_mining_request();
        {
            let mut context = mk_mining_req.get().get_context()?;
            context.set_thread(thread.clone());
        }
        let response = mk_mining_req.send().promise.await?;

        let mining_client = response.get()?.get_result()?;
        log::debug!("Mining client established");

        // Now create a new block to get the block template client
        let mut create_block_req = mining_client.create_new_block_request();
        {
            // Set up the options for creating a new block
            let mut options = create_block_req.get().init_options();
            options.set_use_mempool(true);
            options.set_block_reserved_weight(4000);
        }
        let response = create_block_req.send().promise.await?;

        let block_template_client = response.get()?.get_result()?;
        log::debug!("Block template client established");

        log::info!("Connection to node established successfully");
        Ok(Arc::new(Self {
            rpc_handle,
            disconnector,
            thread,
            chain_client,
            block_template_client
        }))
    }

    /// Disconnect from the node
    pub async fn disconnect(self) -> Result<(), BlockTalkError> {
        log::info!("Disconnecting from node");
        self.disconnector
            .await
            .map_err(BlockTalkError::ConnectionError)?;
        self.rpc_handle
            .await
            .map_err(|e| BlockTalkError::NodeError(e.to_string()))?
            .map_err(BlockTalkError::ConnectionError)?;
        log::info!("Disconnection completed successfully");
        Ok(())
    }

    /// Get a reference to the chain client
    pub fn chain_client(&self) -> &ChainClient {
        &self.chain_client
    }

    /// Get the mining client
    pub fn block_template_client(&self) -> BlockTemplateClient {
        self.block_template_client.clone()
    }

    /// Get a reference to the thread client
    pub fn thread(&self) -> &ThreadClient {
        &self.thread
    }
}
