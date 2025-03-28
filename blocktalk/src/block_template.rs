use crate::mining_capnp::block_template::Client as BlockTemplateClient;
use crate::proxy_capnp::thread::Client as ThreadClient;

#[derive(Clone)]
pub struct BlockTemplateInterface {
    client: BlockTemplateClient,
    thread: ThreadClient,
}

impl BlockTemplateInterface{
    pub fn new(client: BlockTemplateClient, thread: ThreadClient) -> Self {
        Self { client, thread }
    }

    pub async fn get_block_template(&self) -> Result<Vec<u8>, capnp::Error> {
        log::info!("Retrieving new block template");
        let mut request = self.client.get_block_request();

        // Set the thread context
        request
            .get()
            .get_context()?
            .set_thread(self.thread.clone());

        let response = request.send().promise.await?;
        let results = response.get()?;
        
        // Extract the block data and convert to Vec<u8>
        let block_data = results.get_result()?;
        
        // Convert to Vec<u8>
        let block_bytes = block_data.to_vec();
        
        log::info!("Retrieved new block template");
        Ok(block_bytes)
    }
}
