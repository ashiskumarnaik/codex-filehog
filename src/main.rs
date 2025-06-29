mod config;
mod codex;
mod file_processor;
mod storage;
mod monitor;
mod error;

use anyhow::Result;
use config::Config;
use log::info;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    let config = Config::load()?;
    config.validate()?;
    
    info!("Starting FileHog with config: target={}, output={}", 
          config.target_folder.display(), config.output_folder.display());
    
    let codex_client = Arc::new(codex::Client::new(config.codex_endpoints.clone()));
    
    codex_client.check_connectivity().await?;
    info!("All Codex endpoints are reachable");
    
    let file_processor = file_processor::FileProcessor::new(
        Arc::new(config),
        codex_client.clone()
    );
    
    let monitor = monitor::Monitor::new(file_processor);
    
    monitor.run().await?;
    
    Ok(())
}
