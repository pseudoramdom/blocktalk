use std::sync::Arc;

mod chain;
mod connection;
mod error;
mod generated;
mod notification;
mod block_template;

pub use block_template::BlockTemplateInterface;
pub use chain::ChainInterface;
pub use connection::Connection;
pub use error::BlockTalkError;
pub use generated::*;
pub use notification::ChainNotification;
pub use notification::NotificationHandler;

#[derive(Clone)]
/// Main entry point for blockchain interaction
pub struct BlockTalk {
    connection: Arc<Connection>,
    chain: Arc<ChainInterface>,
    block_template_interface: BlockTemplateInterface
}

impl BlockTalk {
    /// Initialize BlockTalk by connecting to a Bitcoin node through the specified socket
    pub async fn init(socket_path: &str) -> Result<Self, BlockTalkError> {
        log::info!("Initializing BlockTalk with socket path: {}", socket_path);
        let connection = Connection::connect(socket_path).await?;
        let chain = Arc::new(ChainInterface::new(connection.clone()));

        let block_template_client = connection.block_template_client();
        let thread_client = connection.thread().clone();
        let block_template_interface = BlockTemplateInterface::new(block_template_client, thread_client);
        log::info!("BlockTalk initialized successfully");

        Ok(Self { connection, chain, block_template_interface })
    }

    /// Get a reference to the chain interface
    pub fn chain(&self) -> &Arc<ChainInterface> {
        &self.chain
    }

    /// Get a reference to the block template interface
    pub fn block_template(&self) -> &BlockTemplateInterface {
        &self.block_template_interface
    }

    /// Disconnect from the node
    pub async fn disconnect(self) -> Result<(), BlockTalkError> {
        // Check if we're the last owner of the connection
        match Arc::try_unwrap(self.connection) {
            Ok(conn) => conn.disconnect().await,
            Err(_) => {
                // There are other references to the connection still alive
                Ok(())
            }
        }
    }
}
