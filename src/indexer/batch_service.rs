use std::sync::{atomic::AtomicBool, Arc};

use eyre::Result;

#[derive(Debug)]
pub struct BatchIndexConfig {
    pub starting_block: i64,
}

// TODO: In construction
pub async fn batch_index(
    #[allow(unused_variables)] config: BatchIndexConfig,
    #[allow(unused_variables)] should_terminate: Arc<AtomicBool>,
) -> Result<()> {
    Ok(())
}
