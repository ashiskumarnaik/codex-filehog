use anyhow::{anyhow, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "filehog")]
#[command(about = "A tool for storing files on Codex decentralized storage")]
pub struct Args {
    #[arg(short, long, help = "Path to configuration file")]
    pub config: Option<PathBuf>,
    
    #[arg(short, long, help = "Target folder to store")]
    pub target_folder: Option<PathBuf>,
    
    #[arg(short, long, help = "Output folder for metadata")]
    pub output_folder: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub target_folder: PathBuf,
    pub output_folder: PathBuf,
    pub output_structure: OutputStructure,
    pub codex_endpoints: Vec<String>,
    pub storage_params: StorageParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputStructure {
    Flattened,
    Structured,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageParams {
    pub price: u64,
    pub nodes: u32,
    pub tolerance: u32,
    pub proof_probability: u32,
    pub duration_days: u32,
    pub expiry_minutes: u32,
    pub collateral: u64,
}

impl Default for StorageParams {
    fn default() -> Self {
        Self {
            price: 1000,
            nodes: 10,
            tolerance: 5,
            proof_probability: 100,
            duration_days: 6,
            expiry_minutes: 60,
            collateral: 1,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let args = Args::parse();
        
        let config = if let Some(config_path) = args.config {
            let config_str = std::fs::read_to_string(&config_path)
                .map_err(|e| anyhow!("Failed to read config file {}: {}", config_path.display(), e))?;
            toml::from_str(&config_str)
                .map_err(|e| anyhow!("Failed to parse config file: {}", e))?
        } else {
            Self::default_config()
        };
        
        let mut final_config = config;
        
        if let Some(target) = args.target_folder {
            final_config.target_folder = target;
        }
        
        if let Some(output) = args.output_folder {
            final_config.output_folder = output;
        }
        
        Ok(final_config)
    }
    
    fn default_config() -> Self {
        Self {
            target_folder: PathBuf::from("./target"),
            output_folder: PathBuf::from("./output"),
            output_structure: OutputStructure::Structured,
            codex_endpoints: vec!["http://localhost:8080".to_string()],
            storage_params: StorageParams::default(),
        }
    }
    
    pub fn validate(&self) -> Result<()> {
        if self.target_folder == self.output_folder {
            return Err(anyhow!(
                "Target folder and output folder cannot be the same: {}",
                self.target_folder.display()
            ));
        }
        
        if !self.target_folder.exists() {
            return Err(anyhow!(
                "Target folder does not exist: {}",
                self.target_folder.display()
            ));
        }
        
        if !self.target_folder.is_dir() {
            return Err(anyhow!(
                "Target folder is not a directory: {}",
                self.target_folder.display()
            ));
        }
        
        if self.storage_params.duration_days < 1 {
            return Err(anyhow!(
                "Duration must be at least 1 day, got: {}",
                self.storage_params.duration_days
            ));
        }
        
        if self.storage_params.expiry_minutes < 15 {
            return Err(anyhow!(
                "Expiry must be at least 15 minutes, got: {}",
                self.storage_params.expiry_minutes
            ));
        }
        
        let duration_minutes = self.storage_params.duration_days * 24 * 60;
        if self.storage_params.expiry_minutes > duration_minutes {
            return Err(anyhow!(
                "Expiry ({} minutes) cannot be greater than duration ({} minutes)",
                self.storage_params.expiry_minutes,
                duration_minutes
            ));
        }
        
        if self.codex_endpoints.is_empty() {
            return Err(anyhow!("At least one Codex endpoint must be provided"));
        }
        
        std::fs::create_dir_all(&self.output_folder)
            .map_err(|e| anyhow!(
                "Failed to create output folder {}: {}",
                self.output_folder.display(),
                e
            ))?;
        
        Ok(())
    }
    
    pub fn duration(&self) -> Duration {
        Duration::from_secs(self.storage_params.duration_days as u64 * 24 * 60 * 60)
    }
    
    pub fn expiry(&self) -> Duration {
        Duration::from_secs(self.storage_params.expiry_minutes as u64 * 60)
    }
}