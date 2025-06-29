use anyhow::Result;
use log::error;
use std::path::Path;

pub async fn retry_with_backoff<F, Fut, T, E>(
    mut operation: F,
    operation_name: &str,
    max_retries: u32,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_error = None;
    
    for attempt in 0..=max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(err) => {
                error!("Attempt {} of {} failed for {}: {}", 
                       attempt + 1, max_retries + 1, operation_name, err);
                last_error = Some(err);
                
                if attempt < max_retries {
                    let delay = std::time::Duration::from_secs(2_u64.pow(attempt));
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
    
    Err(last_error.unwrap())
}

pub fn crash_with_error(message: &str) -> ! {
    error!("FATAL ERROR: {}", message);
    std::process::exit(1);
}

pub fn write_crash_report(output_folder: &Path, error: &str) -> Result<()> {
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let crash_file = output_folder.join(format!("crash_report_{}.txt", timestamp));
    
    let report = format!(
        "Crash Report - {}\n\n{}\n\nTimestamp: {}\n",
        timestamp,
        error,
        chrono::Utc::now().to_rfc3339()
    );
    
    std::fs::write(&crash_file, report)?;
    error!("Crash report written to: {}", crash_file.display());
    
    Ok(())
}