use std::sync::{atomic::AtomicBool, Arc};

use eyre::{Context, Result};

#[derive(Debug)]
pub struct BatchIndexConfig {
    pub starting_block: i64,
}

pub async fn batch_index(
    config: BatchIndexConfig,
    should_terminate: Arc<AtomicBool>,
) -> Result<()> {
    Ok(())
}
