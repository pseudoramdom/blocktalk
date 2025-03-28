use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::chain_capnp::chain::Client as ChainClient;
use crate::init_capnp::init::Client as InitClient;
use crate::proxy_capnp::thread::Client as ThreadClient;
use crate::BlockTalkError;
use crate::mining_capnp::block_template::Client as BlockTemplateClient;

#[async_trait::async_trait(?Send)]
pub trait ConnectionProvider: Send + Sync {
    async fn create_network(
        &self,
        path: &str,
    ) -> Result<Box<dyn capnp_rpc::VatNetwork<twoparty::VatId>>, BlockTalkError>;

    fn create_rpc(
        &self,
        network: Box<dyn capnp_rpc::VatNetwork<twoparty::VatId>>,
    ) -> (
        RpcSystem<capnp_rpc::rpc_twoparty_capnp::Side>,
        InitClient,
        capnp_rpc::Disconnector<twoparty::VatId>,
    );

    fn spawn_rpc(
        &self,
        rpc: RpcSystem<capnp_rpc::rpc_twoparty_capnp::Side>,
    ) -> JoinHandle<Result<(), capnp::Error>> {
        tokio::task::spawn_local(rpc)
    }

    async fn create_clients(
        &self,
        init: &InitClient,
    ) -> Result<(ThreadClient, ChainClient), BlockTalkError>;
}

pub struct UnixConnectionProvider;

#[async_trait::async_trait(?Send)]
impl ConnectionProvider for UnixConnectionProvider {
    async fn create_network(
        &self,
        path: &str,
    ) -> Result<Box<dyn capnp_rpc::VatNetwork<twoparty::VatId>>, BlockTalkError> {
        let stream = tokio::net::UnixStream::connect(path).await.map_err(|e| {
            log::error!("Failed to connect to Unix socket at {}: {}", path, e);
            BlockTalkError::node_error(format!("Failed to connect to Unix socket: {}", e), -1)
        })?;
        log::debug!("Unix stream connected successfully");

        let (reader, writer) = stream.into_split();
        Ok(Box::new(twoparty::VatNetwork::new(
            reader.compat(),
            writer.compat_write(),
            rpc_twoparty_capnp::Side::Client,
            Default::default(),
        )))
    }

    fn create_rpc(
        &self,
        network: Box<dyn capnp_rpc::VatNetwork<twoparty::VatId>>,
    ) -> (
        RpcSystem<capnp_rpc::rpc_twoparty_capnp::Side>,
        InitClient,
        capnp_rpc::Disconnector<twoparty::VatId>,
    ) {
        let mut rpc = RpcSystem::new(network, None);
        let init_interface = rpc.bootstrap(rpc_twoparty_capnp::Side::Server);
        let disconnector = rpc.get_disconnector();
        (rpc, init_interface, disconnector)
    }

    async fn create_clients(
        &self,
        init: &InitClient,
    ) -> Result<(ThreadClient, ChainClient), BlockTalkError> {
        // Create thread client
        let mk_init_req = init.construct_request();
        let response = mk_init_req.send().promise.await.map_err(|e| {
            log::error!("Failed to initialize connection: {}", e);
            BlockTalkError::Connection(e.to_string())
        })?;

        let thread_map = response.get()?.get_thread_map().map_err(|e| {
            log::error!("Failed to get thread map: {}", e);
            BlockTalkError::Connection(e.to_string())
        })?;

        let mk_thread_req = thread_map.make_thread_request();
        let response = mk_thread_req.send().promise.await.map_err(|e| {
            log::error!("Failed to create thread: {}", e);
            BlockTalkError::Connection(e.to_string())
        })?;

        let thread = response.get()?.get_result().map_err(|e| {
            log::error!("Failed to get thread result: {}", e);
            BlockTalkError::Connection(e.to_string())
        })?;
        log::debug!("Thread client established");

        // Create chain client using the thread
        let mut mk_chain_req = init.make_chain_request();
        {
            let mut context = mk_chain_req.get().get_context().map_err(|e| {
                log::error!("Failed to get chain context: {}", e);
                BlockTalkError::Connection(e.to_string())
            })?;
            context.set_thread(thread.clone());
        }

        let response = mk_chain_req.send().promise.await.map_err(|e| {
            log::error!("Failed to initialize chain client: {}", e);
            BlockTalkError::Connection(e.to_string())
        })?;

        let chain_client = response.get()?.get_result().map_err(|e| {
            log::error!("Failed to get chain client result: {}", e);
            BlockTalkError::Connection(e.to_string())
        })?;
        log::debug!("Chain client established");

        Ok((thread, chain_client))
    }
}

pub struct Connection {
    rpc_handle: JoinHandle<Result<(), capnp::Error>>,
    disconnector: capnp_rpc::Disconnector<twoparty::VatId>,
    thread: ThreadClient,
    chain_client: ChainClient,
    block_template_client: BlockTemplateClient
}

impl Connection {
    pub async fn connect(
        socket_path: &str,
        provider: Box<dyn ConnectionProvider>,
    ) -> Result<Arc<Self>, BlockTalkError> {
        log::info!("Connecting to Bitcoin node at {}", socket_path);

        let network = provider.create_network(socket_path).await?;
        let (rpc, init_interface, disconnector) = provider.create_rpc(network);
        let rpc_handle = provider.spawn_rpc(rpc);

        let (thread, chain_client) = provider.create_clients(&init_interface).await?;

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

    pub async fn connect_default(socket_path: &str) -> Result<Arc<Self>, BlockTalkError> {
        Self::connect(socket_path, Box::new(UnixConnectionProvider)).await
    }

    pub async fn disconnect(self) -> Result<(), BlockTalkError> {
        log::info!("Disconnecting from node");
        self.disconnector.await.map_err(|e| {
            log::error!("Failed to disconnect RPC: {}", e);
            BlockTalkError::Connection(e.to_string())
        })?;

        match self.rpc_handle.await {
            Ok(result) => {
                result.map_err(|e| {
                    log::error!("RPC handle error during disconnect: {}", e);
                    BlockTalkError::Connection(e.to_string())
                })?;
            }
            Err(e) => {
                log::error!("Task join error during disconnect: {}", e);
                return Err(BlockTalkError::node_error(e.to_string(), -1));
            }
        }

        log::info!("Disconnection completed successfully");
        Ok(())
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    struct MockConnectionProvider {
        network_error: Option<BlockTalkError>,
        clients_error: Option<BlockTalkError>,
    }

    impl MockConnectionProvider {
        fn new() -> Self {
            Self {
                network_error: None,
                clients_error: None,
            }
        }

        fn with_network_error(error: BlockTalkError) -> Self {
            Self {
                network_error: Some(error),
                clients_error: None,
            }
        }

        fn with_clients_error(error: BlockTalkError) -> Self {
            Self {
                network_error: None,
                clients_error: Some(error),
            }
        }
    }

    struct MockVatNetwork;

    impl capnp_rpc::VatNetwork<twoparty::VatId> for MockVatNetwork {
        fn connect(
            &mut self,
            _host_id: twoparty::VatId,
        ) -> Option<Box<dyn capnp_rpc::Connection<twoparty::VatId>>> {
            None
        }

        fn accept(
            &mut self,
        ) -> capnp::capability::Promise<Box<dyn capnp_rpc::Connection<twoparty::VatId>>, capnp::Error>
        {
            unimplemented!("Mock accept")
        }

        fn drive_until_shutdown(&mut self) -> capnp::capability::Promise<(), capnp::Error> {
            unimplemented!("Mock drive_until_shutdown")
        }
    }

    #[async_trait::async_trait(?Send)]
    impl ConnectionProvider for MockConnectionProvider {
        async fn create_network(
            &self,
            _path: &str,
        ) -> Result<Box<dyn capnp_rpc::VatNetwork<twoparty::VatId>>, BlockTalkError> {
            match &self.network_error {
                Some(error) => Err(error.clone()),
                None => Ok(Box::new(MockVatNetwork)),
            }
        }

        fn create_rpc(
            &self,
            _network: Box<dyn capnp_rpc::VatNetwork<twoparty::VatId>>,
        ) -> (
            RpcSystem<capnp_rpc::rpc_twoparty_capnp::Side>,
            InitClient,
            capnp_rpc::Disconnector<twoparty::VatId>,
        ) {
            unimplemented!("Mock create_rpc")
        }

        async fn create_clients(
            &self,
            _init: &InitClient,
        ) -> Result<(ThreadClient, ChainClient), BlockTalkError> {
            match &self.clients_error {
                Some(error) => Err(error.clone()),
                None => unimplemented!("Mock create_clients"),
            }
        }
    }

    #[tokio::test]
    async fn test_connection_network_failure() {
        let error = BlockTalkError::node_error("Network failure".to_string(), -1);
        let provider = MockConnectionProvider::with_network_error(error.clone());

        let result = Connection::connect("test_path", Box::new(provider)).await;
        assert!(matches!(result, Err(e) if e == error));
    }

    #[tokio::test]
    async fn test_connection_clients_failure() {
        let error = BlockTalkError::Connection("Clients creation failure".to_string());
        let provider = MockConnectionProvider::with_clients_error(error.clone());

        let result = Connection::connect("test_path", Box::new(provider)).await;
        assert!(matches!(result, Err(e) if e == error));
    }
}
