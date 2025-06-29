use anyhow::{anyhow, Result};
use log::{info, error, debug, warn};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

use crate::file_processor::FileProcessor;
use crate::storage::StorageManager;

pub struct Monitor {
    file_processor: FileProcessor,
}

impl Monitor {
    pub fn new(file_processor: FileProcessor) -> Self {
        Self { file_processor }
    }
    
    pub async fn run(&self) -> Result<()> {
        info!("Starting FileHog monitor...");
        
        self.file_processor.initialize().await?;
        
        info!("Processing existing files...");
        self.file_processor.process_files().await?;
        
        let (tx, mut rx) = mpsc::channel(100);
        
        let target_folder = self.file_processor.config.target_folder.clone();
        let watcher_tx = tx.clone();
        
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        if let Err(e) = watcher_tx.try_send(event) {
                            error!("Failed to send file event: {}", e);
                        }
                    }
                    Err(e) => error!("File watcher error: {}", e),
                }
            },
            notify::Config::default(),
        ).map_err(|e| anyhow!("Failed to create file watcher: {}", e))?;
        
        watcher.watch(&target_folder, RecursiveMode::Recursive)
            .map_err(|e| anyhow!("Failed to watch target folder: {}", e))?;
        
        info!("File watcher started for: {}", target_folder.display());
        
        let file_processor = Arc::new(self.file_processor.clone());
        let monitor_processor = file_processor.clone();
        
        let monitor_handle = tokio::spawn(async move {
            if let Err(e) = monitor_processor.monitor_purchases().await {
                error!("Purchase monitoring failed: {}", e);
            }
        });
        
        let mut file_check_interval = tokio::time::interval(Duration::from_secs(30));
        
        info!("FileHog monitor is running. Press Ctrl+C to stop.");
        
        loop {
            tokio::select! {
                Some(event) = rx.recv() => {
                    if let Err(e) = self.handle_file_event(event).await {
                        error!("Failed to handle file event: {}", e);
                    }
                }
                _ = file_check_interval.tick() => {
                    if let Err(e) = self.periodic_check().await {
                        error!("Periodic check failed: {}", e);
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("Received shutdown signal");
                    break;
                }
            }
        }
        
        monitor_handle.abort();
        info!("FileHog monitor stopped");
        Ok(())
    }
    
    async fn handle_file_event(&self, event: Event) -> Result<()> {
        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in event.paths {
                    if path.is_file() {
                        let metadata = match path.metadata() {
                            Ok(m) => m,
                            Err(e) => {
                                warn!("Failed to get metadata for {}: {}", path.display(), e);
                                continue;
                            }
                        };
                        
                        let file_size = metadata.len();
                        
                        if file_size < 1024 * 1024 {
                            debug!("Ignoring small file: {} ({} bytes)", path.display(), file_size);
                            continue;
                        }
                        
                        if file_size > 1024 * 1024 * 1024 {
                            warn!("Ignoring large file: {} ({} bytes)", path.display(), file_size);
                            continue;
                        }
                        
                        info!("New file detected: {}", path.display());
                        
                        sleep(Duration::from_secs(1)).await;
                        
                        let new_metadata = match path.metadata() {
                            Ok(m) => m,
                            Err(_e) => {
                                warn!("File disappeared before processing: {}", path.display());
                                continue;
                            }
                        };
                        
                        if new_metadata.len() != file_size {
                            debug!("File {} still being written, skipping for now", path.display());
                            continue;
                        }
                        
                        if let Err(e) = self.file_processor.process_file(&path).await {
                            error!("Failed to process new file {}: {}", path.display(), e);
                        }
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    info!("File removed: {}", path.display());
                }
            }
            _ => {}
        }
        
        Ok(())
    }
    
    async fn periodic_check(&self) -> Result<()> {
        debug!("Performing periodic check...");
        
        let files = self.file_processor.scan_target_folder().await?;
        let mut new_files = Vec::new();
        
        {
            let records = self.file_processor.records.read().await;
            for file_path in files {
                if !records.contains_key(&file_path) {
                    new_files.push(file_path);
                }
            }
        }
        
        if !new_files.is_empty() {
            info!("Found {} new files during periodic check", new_files.len());
            for file_path in new_files {
                if let Err(e) = self.file_processor.process_file(&file_path).await {
                    error!("Failed to process file {}: {}", file_path.display(), e);
                }
            }
        }
        
        Ok(())
    }
}

impl Clone for Monitor {
    fn clone(&self) -> Self {
        Self {
            file_processor: self.file_processor.clone(),
        }
    }
}

impl Clone for FileProcessor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            codex_client: self.codex_client.clone(),
            storage_manager: StorageManager::new(
                self.config.output_folder.clone(),
                self.config.output_structure.clone(),
            ),
            records: self.records.clone(),
        }
    }
}