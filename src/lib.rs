pub mod config;
pub mod codex;
pub mod file_processor;
pub mod storage;
pub mod monitor;
pub mod error;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_storage_params_default() {
        let params = config::StorageParams::default();
        assert_eq!(params.price, 1000);
        assert_eq!(params.nodes, 10);
        assert_eq!(params.tolerance, 5);
        assert_eq!(params.proof_probability, 100);
        assert_eq!(params.duration_days, 6);
        assert_eq!(params.expiry_minutes, 60);
        assert_eq!(params.collateral, 1);
    }

    #[test]
    fn test_config_validation_same_folders() {
        let config = config::Config {
            target_folder: PathBuf::from("/tmp/test"),
            output_folder: PathBuf::from("/tmp/test"),
            output_structure: config::OutputStructure::Structured,
            codex_endpoints: vec!["http://localhost:8080".to_string()],
            storage_params: config::StorageParams::default(),
        };
        
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_duration() {
        let mut config = config::Config {
            target_folder: PathBuf::from("/tmp/target"),
            output_folder: PathBuf::from("/tmp/output"),  
            output_structure: config::OutputStructure::Structured,
            codex_endpoints: vec!["http://localhost:8080".to_string()],
            storage_params: config::StorageParams::default(),
        };
        
        config.storage_params.duration_days = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_expiry() {
        let mut config = config::Config {
            target_folder: PathBuf::from("/tmp/target"),
            output_folder: PathBuf::from("/tmp/output"),
            output_structure: config::OutputStructure::Structured,
            codex_endpoints: vec!["http://localhost:8080".to_string()],
            storage_params: config::StorageParams::default(),
        };
        
        config.storage_params.expiry_minutes = 10;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_file_record_creation() {
        let storage_manager = storage::StorageManager::new(
            PathBuf::from("/tmp/output"),
            config::OutputStructure::Structured,
        );
        
        let record = storage_manager.create_new_record(PathBuf::from("/tmp/test.txt"));
        assert_eq!(record.status, storage::FileStatus::New);
        assert!(record.original_cid.is_none());
        assert!(record.purchase_id.is_none());
    }
}