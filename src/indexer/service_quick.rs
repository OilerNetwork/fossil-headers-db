use std::sync::{atomic::AtomicBool, Arc};

use eyre::{Context, Result};

#[derive(Debug)]
pub struct QuickIndexConfig {
    pub starting_block: i64,
}

pub async fn quick_index(
    config: QuickIndexConfig,
    should_terminate: Arc<AtomicBool>,
) -> Result<()> {
    Ok(())
}
