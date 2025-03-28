use std::sync::Arc;

mod chain;
mod connection;
mod error;
mod generated;
mod mempool;
mod notification;

pub use bitcoin::BlockHash;
pub use chain::{Blockchain, ChainInterface};
pub use connection::{Connection, ConnectionProvider, UnixConnectionProvider};
pub use error::BlockTalkError;
pub use generated::*;
pub use mempool::{Mempool, MempoolInterface, TransactionAncestry};
pub use notification::ChainNotification;
pub use notification::NotificationHandler;

#[derive(Clone)]
pub struct BlockTalk {
    connection: Arc<Connection>,
    chain: Arc<dyn ChainInterface>,
    mempool: Arc<dyn MempoolInterface>,
}

impl BlockTalk {
    pub async fn init(socket_path: &str) -> Result<Self, BlockTalkError> {
        log::info!("Initializing BlockTalk with socket path: {}", socket_path);
        let connection = Connection::connect_default(socket_path).await?;
        let chain = Arc::new(Blockchain::new(connection.clone()));
        let mempool = Arc::new(Mempool::new(
            connection.chain_client().clone(),
            connection.thread().clone(),
        ));
        log::info!("BlockTalk initialized successfully");

        Ok(Self {
            connection,
            chain,
            mempool,
        })
    }

    pub async fn init_with(
        socket_path: &str,
        chain_provider: Box<dyn ConnectionProvider>,
        chain_interface: Arc<dyn ChainInterface>,
        mempool_interface: Arc<dyn MempoolInterface>,
    ) -> Result<Self, BlockTalkError> {
        log::info!(
            "Initializing BlockTalk with socket path: {} and custom provider",
            socket_path
        );
        let connection = Connection::connect(socket_path, chain_provider).await?;
        log::info!("BlockTalk initialized successfully");

        Ok(Self {
            connection,
            chain: chain_interface,
            mempool: mempool_interface,
        })
    }

    pub fn chain(&self) -> &Arc<dyn ChainInterface> {
        &self.chain
    }

    pub fn mempool(&self) -> &Arc<dyn MempoolInterface> {
        &self.mempool
    }

    pub async fn disconnect(self) -> Result<(), BlockTalkError> {
        match Arc::try_unwrap(self.connection) {
            Ok(conn) => conn.disconnect().await,
            Err(_) => Ok(()),
        }
    }
}
