use anyhow::{anyhow, Result};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use log::{info, error, debug};
use tokio::fs;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageRequest {
    pub duration: u64,
    #[serde(rename = "pricePerBytePerSecond")]
    pub reward: String,
    #[serde(rename = "proofProbability")]
    pub proof_probability: String,
    pub nodes: u32,
    pub tolerance: u32,
    pub expiry: u64,
    #[serde(rename = "collateralPerByte")]
    pub collateral: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseResponse {
    #[serde(rename = "purchaseId")]
    pub purchase_id: String,
    pub request: StorageRequestInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageRequestInfo {
    pub content: ContentInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentInfo {
    pub cid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseStatus {
    pub state: String,
    pub request: StorageRequestInfo,
}

#[derive(Debug, Clone)]
pub struct Client {
    endpoints: Vec<String>,
    http_client: HttpClient,
    current_endpoint: Arc<AtomicUsize>,
}

impl Client {
    pub fn new(endpoints: Vec<String>) -> Self {
        Self {
            endpoints,
            http_client: HttpClient::new(),
            current_endpoint: Arc::new(AtomicUsize::new(0)),
        }
    }
    
    fn get_endpoint(&self) -> &str {
        let index = self.current_endpoint.fetch_add(1, Ordering::Relaxed) % self.endpoints.len();
        &self.endpoints[index]
    }
    
    pub async fn check_connectivity(&self) -> Result<()> {
        for endpoint in &self.endpoints {
            let url = format!("{}/api/codex/v1/debug/info", endpoint);
            match self.http_client.get(&url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        info!("Endpoint {} is reachable", endpoint);
                    } else {
                        return Err(anyhow!("Endpoint {} returned status: {}", endpoint, response.status()));
                    }
                }
                Err(e) => {
                    return Err(anyhow!("Failed to connect to endpoint {}: {}", endpoint, e));
                }
            }
        }
        Ok(())
    }
    
    pub async fn upload_file(&self, file_path: &Path) -> Result<String> {
        let endpoint = self.get_endpoint();
        let url = format!("{}/api/codex/v1/data", endpoint);
        
        debug!("Uploading file {} to endpoint {}", file_path.display(), endpoint);
        
        let file_content = fs::read(file_path).await
            .map_err(|e| anyhow!("Failed to read file {}: {}", file_path.display(), e))?;
        
        let file_size = file_content.len();
        if file_size < 1024 * 1024 {
            return Err(anyhow!("File {} is too small ({} bytes). Minimum size is 1MB", 
                             file_path.display(), file_size));
        }
        
        if file_size > 1024 * 1024 * 1024 {
            return Err(anyhow!("File {} is too large ({} bytes). Maximum size is 1GB", 
                             file_path.display(), file_size));
        }
        
        let response = self.http_client
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .body(file_content)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to upload file to {}: {}", endpoint, e))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("Upload failed with status {}: {}", status, error_text));
        }
        
        let cid = response.text().await
            .map_err(|e| anyhow!("Failed to parse upload response: {}", e))?;
        
        let cid = cid.trim();
        info!("Successfully uploaded file {} with CID: {}", file_path.display(), cid);
        Ok(cid.to_string())
    }
    
    pub async fn create_storage_request(&self, cid: &str, storage_params: &crate::config::StorageParams) -> Result<PurchaseResponse> {
        let endpoint = self.get_endpoint();
        let url = format!("{}/api/codex/v1/storage/request/{}", endpoint, cid);
        
        debug!("Creating storage request for CID {} at endpoint {}", cid, endpoint);
        
        let duration_seconds = storage_params.duration_days as u64 * 24 * 60 * 60;
        let expiry_seconds = storage_params.expiry_minutes as u64 * 60;
        
        let request = StorageRequest {
            duration: duration_seconds,
            reward: storage_params.price.to_string(),
            proof_probability: storage_params.proof_probability.to_string(),
            nodes: storage_params.nodes,
            tolerance: storage_params.tolerance,
            expiry: expiry_seconds,
            collateral: storage_params.collateral.to_string(),
        };
        
        let response = self.http_client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to create storage request: {}", e))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            
            if status.as_u16() == 402 {
                return Err(anyhow!("Insufficient tokens to create storage request"));
            }
            
            return Err(anyhow!("Storage request failed with status {}: {}", status, error_text));
        }
        
        let purchase_id = response.text().await
            .map_err(|e| anyhow!("Failed to get purchase ID: {}", e))?;
        
        let purchase_id = purchase_id.trim().to_string();
        
        // Create a purchase response with the ID we got and the CID we requested
        let purchase_response = PurchaseResponse {
            purchase_id: purchase_id.clone(),
            request: StorageRequestInfo {
                content: ContentInfo {
                    cid: cid.to_string(),
                }
            }
        };
        
        info!("Created storage request for CID {} with purchase ID: {}", cid, purchase_response.purchase_id);
        Ok(purchase_response)
    }
    
    pub async fn get_purchase_status(&self, purchase_id: &str) -> Result<PurchaseStatus> {
        let endpoint = self.get_endpoint();
        let url = format!("{}/api/codex/v1/storage/purchases/{}", endpoint, purchase_id);
        
        let response = self.http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to get purchase status: {}", e))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to get purchase status with status {}: {}", 
                             status, error_text));
        }
        
        let status: PurchaseStatus = response.json().await
            .map_err(|e| anyhow!("Failed to parse purchase status: {}", e))?;
        
        debug!("Purchase {} status: {}", purchase_id, status.state);
        Ok(status)
    }
    
    pub async fn wait_for_purchase_start(&self, purchase_id: &str, timeout_secs: u64) -> Result<PurchaseStatus> {
        let start_time = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);
        
        loop {
            let status = self.get_purchase_status(purchase_id).await?;
            
            match status.state.as_str() {
                "started" => {
                    info!("Purchase {} started successfully", purchase_id);
                    return Ok(status);
                }
                "cancelled" | "expired" | "failed" => {
                    return Err(anyhow!("Purchase {} reached final state: {}", purchase_id, status.state));
                }
                _ => {
                    if start_time.elapsed() > timeout {
                        return Err(anyhow!("Timeout waiting for purchase {} to start", purchase_id));
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    }
}