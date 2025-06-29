use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use log::{info, error, debug};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub file_path: PathBuf,
    pub original_cid: Option<String>,
    pub storage_cid: Option<String>,
    pub purchase_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub codex_endpoint: Option<String>,
    pub status: FileStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileStatus {
    New,
    Uploading,
    Creating,
    Active,
    Failed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlattenedRecord {
    pub relative_path: String,
    #[serde(flatten)]
    pub record: FileRecord,
}

pub struct StorageManager {
    output_folder: PathBuf,
    output_structure: crate::config::OutputStructure,
}

impl StorageManager {
    pub fn new(output_folder: PathBuf, output_structure: crate::config::OutputStructure) -> Self {
        Self {
            output_folder,
            output_structure,
        }
    }
    
    pub async fn load_existing_records(&self, target_folder: &Path) -> Result<HashMap<PathBuf, FileRecord>> {
        let mut records = HashMap::new();
        
        match self.output_structure {
            crate::config::OutputStructure::Flattened => {
                self.load_flattened_records(&mut records, target_folder).await?;
            }
            crate::config::OutputStructure::Structured => {
                self.load_structured_records(&mut records, target_folder).await?;
            }
        }
        
        info!("Loaded {} existing file records", records.len());
        Ok(records)
    }
    
    async fn load_flattened_records(&self, records: &mut HashMap<PathBuf, FileRecord>, target_folder: &Path) -> Result<()> {
        let flattened_file = self.output_folder.join("files.json");
        
        if !flattened_file.exists() {
            return Ok(());
        }
        
        let content = fs::read_to_string(&flattened_file).await
            .map_err(|e| anyhow!("Failed to read flattened records file: {}", e))?;
        
        let flattened_records: Vec<FlattenedRecord> = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse flattened records: {}", e))?;
        
        for flattened in flattened_records {
            let full_path = target_folder.join(&flattened.relative_path);
            records.insert(full_path, flattened.record);
        }
        
        Ok(())
    }
    
    async fn load_structured_records(&self, records: &mut HashMap<PathBuf, FileRecord>, target_folder: &Path) -> Result<()> {
        let walker = WalkDir::new(&self.output_folder);
        
        for entry in walker {
            let entry = entry.map_err(|e| anyhow!("Failed to read output directory: {}", e))?;
            let path = entry.path();
            
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                let relative_output_path = path.strip_prefix(&self.output_folder)
                    .map_err(|e| anyhow!("Failed to get relative path: {}", e))?;
                
                let original_path = self.output_path_to_original_path(relative_output_path, target_folder)?;
                
                let content = fs::read_to_string(path).await
                    .map_err(|e| anyhow!("Failed to read record file {}: {}", path.display(), e))?;
                
                let record: FileRecord = serde_json::from_str(&content)
                    .map_err(|e| anyhow!("Failed to parse record from {}: {}", path.display(), e))?;
                
                records.insert(original_path, record);
            }
        }
        
        Ok(())
    }
    
    pub async fn save_record(&self, target_folder: &Path, file_path: &Path, record: &FileRecord) -> Result<()> {
        match self.output_structure {
            crate::config::OutputStructure::Flattened => {
                self.save_flattened_record(target_folder, file_path, record).await
            }
            crate::config::OutputStructure::Structured => {
                self.save_structured_record(target_folder, file_path, record).await
            }
        }
    }
    
    async fn save_flattened_record(&self, target_folder: &Path, file_path: &Path, new_record: &FileRecord) -> Result<()> {
        let flattened_file = self.output_folder.join("files.json");
        
        let mut records = if flattened_file.exists() {
            let content = fs::read_to_string(&flattened_file).await
                .map_err(|e| anyhow!("Failed to read existing flattened file: {}", e))?;
            serde_json::from_str::<Vec<FlattenedRecord>>(&content)
                .map_err(|e| anyhow!("Failed to parse existing flattened file: {}", e))?
        } else {
            Vec::new()
        };
        
        let relative_path = file_path.strip_prefix(target_folder)
            .map_err(|e| anyhow!("Failed to get relative path: {}", e))?
            .to_string_lossy()
            .to_string();
        
        let flattened_record = FlattenedRecord {
            relative_path: relative_path.clone(),
            record: new_record.clone(),
        };
        
        let existing_index = records.iter().position(|r| r.relative_path == relative_path);
        
        if let Some(index) = existing_index {
            records[index] = flattened_record;
        } else {
            records.push(flattened_record);
        }
        
        let content = serde_json::to_string_pretty(&records)
            .map_err(|e| anyhow!("Failed to serialize flattened records: {}", e))?;
        
        fs::write(&flattened_file, content).await
            .map_err(|e| anyhow!("Failed to write flattened records: {}", e))?;
        
        debug!("Saved flattened record for {}", file_path.display());
        Ok(())
    }
    
    async fn save_structured_record(&self, target_folder: &Path, file_path: &Path, record: &FileRecord) -> Result<()> {
        let relative_path = file_path.strip_prefix(target_folder)
            .map_err(|e| anyhow!("Failed to get relative path: {}", e))?;
        
        let output_path = self.output_folder.join(relative_path).with_extension("json");
        
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| anyhow!("Failed to create output directory {}: {}", parent.display(), e))?;
        }
        
        let content = serde_json::to_string_pretty(record)
            .map_err(|e| anyhow!("Failed to serialize record: {}", e))?;
        
        fs::write(&output_path, content).await
            .map_err(|e| anyhow!("Failed to write record to {}: {}", output_path.display(), e))?;
        
        debug!("Saved structured record for {} to {}", file_path.display(), output_path.display());
        Ok(())
    }
    
    fn output_path_to_original_path(&self, output_path: &Path, target_folder: &Path) -> Result<PathBuf> {
        let without_extension = output_path.with_extension("");
        Ok(target_folder.join(without_extension))
    }
    
    pub fn create_new_record(&self, file_path: PathBuf) -> FileRecord {
        let now = Utc::now();
        FileRecord {
            file_path,
            original_cid: None,
            storage_cid: None,
            purchase_id: None,
            created_at: now,
            updated_at: now,
            codex_endpoint: None,
            status: FileStatus::New,
            error: None,
        }
    }
    
    pub fn update_record_status(&self, record: &mut FileRecord, status: FileStatus, error: Option<String>) {
        record.status = status;
        record.error = error;
        record.updated_at = Utc::now();
    }
    
    pub fn update_record_upload(&self, record: &mut FileRecord, cid: String, endpoint: String) {
        record.original_cid = Some(cid);
        record.codex_endpoint = Some(endpoint);
        record.status = FileStatus::Uploading;
        record.updated_at = Utc::now();
    }
    
    pub fn update_record_purchase(&self, record: &mut FileRecord, purchase_id: String, storage_cid: String) {
        record.purchase_id = Some(purchase_id);
        record.storage_cid = Some(storage_cid);
        record.status = FileStatus::Creating;
        record.updated_at = Utc::now();
    }
    
    pub fn mark_record_active(&self, record: &mut FileRecord) {
        record.status = FileStatus::Active;
        record.updated_at = Utc::now();
    }
    
    pub fn needs_new_purchase(&self, record: &FileRecord, expiry_buffer: chrono::Duration) -> bool {
        match record.status {
            FileStatus::Failed | FileStatus::Expired => true,
            FileStatus::Active => {
                let time_until_expiry = record.created_at + chrono::Duration::days(6) - Utc::now();
                time_until_expiry < expiry_buffer
            }
            _ => false
        }
    }
}