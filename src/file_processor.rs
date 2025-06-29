use anyhow::{anyhow, Result};
use log::{info, error, debug, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use walkdir::WalkDir;

use crate::codex::Client as CodexClient;
use crate::config::Config;
use crate::error::retry_with_backoff;
use crate::storage::{FileRecord, FileStatus, StorageManager};

pub struct FileProcessor {
    pub config: Arc<Config>,
    pub codex_client: Arc<CodexClient>,
    pub storage_manager: StorageManager,
    pub records: Arc<RwLock<HashMap<PathBuf, FileRecord>>>,
}

impl FileProcessor {
    pub fn new(config: Arc<Config>, codex_client: Arc<CodexClient>) -> Self {
        let storage_manager = StorageManager::new(
            config.output_folder.clone(),
            config.output_structure.clone(),
        );
        
        Self {
            config,
            codex_client,
            storage_manager,
            records: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing file processor...");
        
        let existing_records = self.storage_manager
            .load_existing_records(&self.config.target_folder)
            .await?;
        
        *self.records.write().await = existing_records;
        
        info!("File processor initialized successfully");
        Ok(())
    }
    
    pub async fn scan_target_folder(&self) -> Result<Vec<PathBuf>> {
        info!("Scanning target folder: {}", self.config.target_folder.display());
        
        let mut files = Vec::new();
        
        for entry in WalkDir::new(&self.config.target_folder) {
            let entry = entry.map_err(|e| anyhow!("Failed to read directory entry: {}", e))?;
            let path = entry.path();
            
            if path.is_file() {
                let metadata = path.metadata()
                    .map_err(|e| anyhow!("Failed to get metadata for {}: {}", path.display(), e))?;
                
                let file_size = metadata.len();
                
                if file_size < 1024 * 1024 {
                    warn!("Skipping file {} (too small: {} bytes)", path.display(), file_size);
                    continue;
                }
                
                if file_size > 1024 * 1024 * 1024 {
                    warn!("Skipping file {} (too large: {} bytes)", path.display(), file_size);
                    continue;
                }
                
                files.push(path.to_path_buf());
            }
        }
        
        info!("Found {} eligible files", files.len());
        Ok(files)
    }
    
    pub async fn process_files(&self) -> Result<()> {
        let files = self.scan_target_folder().await?;
        
        for file_path in files {
            if let Err(e) = self.process_file(&file_path).await {
                error!("Failed to process file {}: {}", file_path.display(), e);
                
                let mut records = self.records.write().await;
                let record = records.entry(file_path.clone())
                    .or_insert_with(|| self.storage_manager.create_new_record(file_path.clone()));
                
                self.storage_manager.update_record_status(record, FileStatus::Failed, Some(e.to_string()));
                
                if let Err(save_err) = self.storage_manager
                    .save_record(&self.config.target_folder, &file_path, record)
                    .await
                {
                    error!("Failed to save error record for {}: {}", file_path.display(), save_err);
                }
            }
        }
        
        Ok(())
    }
    
    pub async fn process_file(&self, file_path: &Path) -> Result<()> {
        let mut records = self.records.write().await;
        let record = records.entry(file_path.to_path_buf())
            .or_insert_with(|| self.storage_manager.create_new_record(file_path.to_path_buf()));
        
        if record.status == FileStatus::Active && !self.needs_renewal(record) {
            debug!("File {} already has active storage", file_path.display());
            return Ok(());
        }
        
        drop(records);
        
        info!("Processing file: {}", file_path.display());
        
        let upload_result = {
            let client = self.codex_client.clone();
            let path = file_path.to_path_buf();
            retry_with_backoff(
                || client.upload_file(&path),
                &format!("upload file {}", file_path.display()),
                3,
            ).await
        };
        
        let original_cid = match upload_result {
            Ok(cid) => cid,
            Err(e) => {
                let mut records = self.records.write().await;
                let record = records.get_mut(file_path).unwrap();
                self.storage_manager.update_record_status(record, FileStatus::Failed, Some(e.to_string()));
                self.storage_manager.save_record(&self.config.target_folder, file_path, record).await?;
                return Err(anyhow!("Upload failed: {}", e));
            }
        };
        
        {
            let mut records = self.records.write().await;
            let record = records.get_mut(file_path).unwrap();
            self.storage_manager.update_record_upload(record, original_cid.clone(), "endpoint".to_string());
            self.storage_manager.save_record(&self.config.target_folder, file_path, record).await?;
        }
        
        let purchase_result = {
            let client = self.codex_client.clone();
            let cid = original_cid.clone();
            let params = self.config.storage_params.clone();
            retry_with_backoff(
                || client.create_storage_request(&cid, &params),
                &format!("create storage request for {}", file_path.display()),
                3,
            ).await
        };
        
        let purchase_response = match purchase_result {
            Ok(response) => response,
            Err(e) => {
                let mut records = self.records.write().await;
                let record = records.get_mut(file_path).unwrap();
                self.storage_manager.update_record_status(record, FileStatus::Failed, Some(e.to_string()));
                self.storage_manager.save_record(&self.config.target_folder, file_path, record).await?;
                return Err(anyhow!("Storage request failed: {}", e));
            }
        };
        
        {
            let mut records = self.records.write().await;
            let record = records.get_mut(file_path).unwrap();
            self.storage_manager.update_record_purchase(
                record,
                purchase_response.purchase_id.clone(),
                purchase_response.request.content.cid.clone(),
            );
            self.storage_manager.save_record(&self.config.target_folder, file_path, record).await?;
        }
        
        let timeout_secs = self.config.storage_params.expiry_minutes as u64 * 60;
        let wait_result = self.codex_client
            .wait_for_purchase_start(&purchase_response.purchase_id, timeout_secs)
            .await;
        
        match wait_result {
            Ok(_) => {
                let mut records = self.records.write().await;
                let record = records.get_mut(file_path).unwrap();
                self.storage_manager.mark_record_active(record);
                self.storage_manager.save_record(&self.config.target_folder, file_path, record).await?;
                info!("Successfully stored file: {}", file_path.display());
            }
            Err(e) => {
                let mut records = self.records.write().await;
                let record = records.get_mut(file_path).unwrap();
                self.storage_manager.update_record_status(record, FileStatus::Failed, Some(e.to_string()));
                self.storage_manager.save_record(&self.config.target_folder, file_path, record).await?;
                return Err(anyhow!("Purchase failed to start: {}", e));
            }
        }
        
        Ok(())
    }
    
    fn needs_renewal(&self, record: &FileRecord) -> bool {
        let one_hour = chrono::Duration::hours(1);
        self.storage_manager.needs_new_purchase(record, one_hour)
    }
    
    pub async fn monitor_purchases(&self) -> Result<()> {
        info!("Starting purchase monitoring...");
        
        loop {
            let purchases_to_check: Vec<(PathBuf, String)> = {
                let records = self.records.read().await;
                records.iter()
                    .filter_map(|(path, record)| {
                        if record.status == FileStatus::Active {
                            record.purchase_id.as_ref().map(|id| (path.clone(), id.clone()))
                        } else {
                            None
                        }
                    })
                    .collect()
            };
            
            for (file_path, purchase_id) in purchases_to_check {
                if let Err(e) = self.check_purchase_status(&file_path, &purchase_id).await {
                    error!("Failed to check purchase status for {}: {}", file_path.display(), e);
                }
            }
            
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
        }
    }
    
    async fn check_purchase_status(&self, file_path: &Path, purchase_id: &str) -> Result<()> {
        let status = self.codex_client.get_purchase_status(purchase_id).await?;
        
        match status.state.as_str() {
            "started" => {
                // Still active, check if renewal is needed
                let records = self.records.read().await;
                if let Some(record) = records.get(file_path) {
                    if self.needs_renewal(record) {
                        drop(records);
                        info!("Purchase {} needs renewal for file {}", purchase_id, file_path.display());
                        self.process_file(file_path).await?;
                    }
                }
            }
            "failed" | "cancelled" | "expired" => {
                info!("Purchase {} failed for file {}, creating new purchase", purchase_id, file_path.display());
                self.process_file(file_path).await?;
            }
            _ => {
                debug!("Purchase {} in state: {}", purchase_id, status.state);
            }
        }
        
        Ok(())
    }
}