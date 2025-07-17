/*!
Model download and management functionality.

This module handles downloading, verifying, and caching semantic search models
from the model registry.
*/

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::registry::{ModelInfo, ModelRegistry};

/// Progress callback for download operations
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Model downloader and manager
pub struct ModelDownloader {
    /// Registry containing model definitions
    registry: ModelRegistry,
    /// Base directory for model storage
    storage_path: PathBuf,
}

impl ModelDownloader {
    /// Create a new model downloader
    pub fn new(registry: ModelRegistry, storage_path: PathBuf) -> Self {
        Self {
            registry,
            storage_path,
        }
    }

    /// Check if a model is already downloaded and valid
    pub fn is_model_available(&self, model_name: &str) -> Result<bool> {
        let model_info = self.registry.validate_model(model_name)?;
        let model_dir = self.storage_path.join(model_name);
        
        // Check if both required files exist
        let model_file = model_dir.join(&model_info.files.model.filename);
        let tokenizer_file = model_dir.join(&model_info.files.tokenizer.filename);
        
        if !model_file.exists() || !tokenizer_file.exists() {
            return Ok(false);
        }

        // TODO: Add hash verification here
        // For now, just check file existence
        Ok(true)
    }

    /// Download a model if not already available
    pub fn ensure_model_available(&self, model_name: &str) -> Result<PathBuf> {
        if self.is_model_available(model_name)? {
            return Ok(self.storage_path.join(model_name));
        }

        self.download_model(model_name)
    }

    /// Download a specific model
    pub fn download_model(&self, model_name: &str) -> Result<PathBuf> {
        let model_info = self.registry.validate_model(model_name)?;
        let model_dir = self.storage_path.join(model_name);

        // Create model directory
        fs::create_dir_all(&model_dir)
            .with_context(|| format!("Failed to create model directory: {}", model_dir.display()))?;

        println!("Downloading model: {} ({} MB)", model_name, model_info.size_mb);

        // Download model file
        let model_path = model_dir.join(&model_info.files.model.filename);
        self.download_file(&model_info.files.model.url, &model_path)
            .with_context(|| format!("Failed to download model file for {}", model_name))?;

        // Download tokenizer file
        let tokenizer_path = model_dir.join(&model_info.files.tokenizer.filename);
        self.download_file(&model_info.files.tokenizer.url, &tokenizer_path)
            .with_context(|| format!("Failed to download tokenizer file for {}", model_name))?;

        println!("Successfully downloaded model: {}", model_name);
        Ok(model_dir)
    }

    /// Download a file from URL to local path
    fn download_file(&self, url: &str, local_path: &Path) -> Result<()> {
        println!("  Downloading: {} -> {}", url, local_path.display());
        
        // Make HTTP request
        let response = reqwest::blocking::get(url)
            .with_context(|| format!("Failed to make HTTP request to {}", url))?;
        
        // Check if request was successful
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "HTTP request failed with status: {} for URL: {}", 
                response.status(), 
                url
            ));
        }
        
        let total_size = response.content_length().unwrap_or(0);
        println!("  File size: {} bytes", total_size);
        
        // Create the parent directory if it doesn't exist
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create parent directory: {}", parent.display()))?;
        }
        
        // Create the file and download content
        let mut file = fs::File::create(local_path)
            .with_context(|| format!("Failed to create file: {}", local_path.display()))?;
        
        let content = response.bytes()
            .with_context(|| format!("Failed to read response body from {}", url))?;
            
        file.write_all(&content)
            .with_context(|| format!("Failed to write to file: {}", local_path.display()))?;
        
        let downloaded = content.len() as u64;
        println!("  Downloaded: {} bytes", downloaded);
        
        if total_size > 0 && downloaded != total_size {
            return Err(anyhow::anyhow!(
                "Download incomplete: expected {} bytes, got {} bytes", 
                total_size, 
                downloaded
            ));
        }
        
        println!("  Successfully downloaded: {}", local_path.display());
        Ok(())
    }

    /// List all available models in the registry
    pub fn list_available_models(&self) -> Vec<(&str, &ModelInfo)> {
        self.registry
            .models
            .iter()
            .map(|(name, info)| (name.as_str(), info))
            .collect()
    }

    /// List downloaded models
    pub fn list_downloaded_models(&self) -> Result<Vec<String>> {
        let mut downloaded = Vec::new();
        
        if !self.storage_path.exists() {
            return Ok(downloaded);
        }

        for entry in fs::read_dir(&self.storage_path)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                if let Some(model_name) = path.file_name().and_then(|s| s.to_str()) {
                    if self.is_model_available(model_name).unwrap_or(false) {
                        downloaded.push(model_name.to_string());
                    }
                }
            }
        }

        Ok(downloaded)
    }

    /// Remove a downloaded model
    pub fn remove_model(&self, model_name: &str) -> Result<()> {
        let model_dir = self.storage_path.join(model_name);
        
        if model_dir.exists() {
            fs::remove_dir_all(&model_dir)
                .with_context(|| format!("Failed to remove model directory: {}", model_dir.display()))?;
            println!("Removed model: {}", model_name);
        } else {
            println!("Model not found locally: {}", model_name);
        }

        Ok(())
    }

    /// Get the local path for a model's files
    pub fn get_model_paths(&self, model_name: &str) -> Result<(PathBuf, PathBuf)> {
        let model_info = self.registry.validate_model(model_name)?;
        let model_dir = self.storage_path.join(model_name);
        
        let model_path = model_dir.join(&model_info.files.model.filename);
        let tokenizer_path = model_dir.join(&model_info.files.tokenizer.filename);
        
        Ok((model_path, tokenizer_path))
    }

    /// Get model information
    pub fn get_model_info(&self, model_name: &str) -> Result<&ModelInfo> {
        self.registry.validate_model(model_name)
    }

    /// Get storage directory for models
    pub fn storage_path(&self) -> &Path {
        &self.storage_path
    }
}

/// Model management utilities
pub struct ModelManager;

impl ModelManager {
    /// Create a model downloader with automatic path detection
    pub fn create_downloader(custom_path: Option<&Path>) -> Result<ModelDownloader> {
        let registry = ModelRegistry::load_with_fallback(None)?;
        
        let storage_path = if let Some(path) = custom_path {
            path.to_path_buf()
        } else {
            Self::default_storage_path()?
        };

        Ok(ModelDownloader::new(registry, storage_path))
    }

    /// Get the default storage path for models
    pub fn default_storage_path() -> Result<PathBuf> {
        let home_dir = dirs::home_dir()
            .context("Could not determine home directory")?;
        
        Ok(home_dir.join(".cache/outgrep/models"))
    }

    /// Print model information in a user-friendly format
    pub fn print_model_info(model_info: &ModelInfo) {
        println!("Model: {}", model_info.name);
        println!("  Description: {}", model_info.description);
        println!("  Dimensions: {}", model_info.dimensions);
        println!("  Size: {} MB", model_info.size_mb);
        println!("  License: {}", model_info.license);
        println!("  Performance:");
        println!("    Speed: {}", model_info.performance.speed);
        println!("    Quality: {}", model_info.performance.quality);
        println!("    Memory: {} MB", model_info.performance.memory_mb);
        println!("    Inference: {} ms", model_info.performance.inference_time_ms);
        println!("  Use cases: {}", model_info.use_cases.join(", "));
        println!("  Recommended for: {}", model_info.recommended_for);
    }

    /// Print available models in a table format
    pub fn print_available_models(downloader: &ModelDownloader) {
        println!("{:<25} {:<15} {:<8} {:<12} {:<30}", "Model", "Speed", "Size MB", "Dimensions", "Description");
        println!("{}", "-".repeat(90));
        
        let mut models: Vec<_> = downloader.list_available_models();
        models.sort_by_key(|(_, info)| info.performance.speed.as_str());
        
        for (name, info) in models {
            let downloaded = downloader.is_model_available(name).unwrap_or(false);
            let status = if downloaded { " ✓" } else { "" };
            
            println!(
                "{:<25} {:<15} {:<8} {:<12} {:<30}{}",
                name,
                info.performance.speed,
                info.size_mb,
                info.dimensions,
                truncate_string(&info.description, 30),
                status
            );
        }
    }
}

/// Truncate a string to a maximum length with ellipsis
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_model_downloader_creation() {
        let registry = ModelRegistry::load_default().unwrap();
        let temp_dir = TempDir::new().unwrap();
        
        let downloader = ModelDownloader::new(registry, temp_dir.path().to_path_buf());
        assert_eq!(downloader.storage_path(), temp_dir.path());
    }

    #[test]
    fn test_model_availability_check() {
        let registry = ModelRegistry::load_default().unwrap();
        let temp_dir = TempDir::new().unwrap();
        
        let downloader = ModelDownloader::new(registry, temp_dir.path().to_path_buf());
        
        // Model should not be available initially
        assert!(!downloader.is_model_available("all-MiniLM-L6-v2").unwrap());
    }

    #[test]
    fn test_get_model_paths() {
        let registry = ModelRegistry::load_default().unwrap();
        let temp_dir = TempDir::new().unwrap();
        
        let downloader = ModelDownloader::new(registry, temp_dir.path().to_path_buf());
        let (model_path, tokenizer_path) = downloader.get_model_paths("all-MiniLM-L6-v2").unwrap();
        
        assert!(model_path.to_string_lossy().contains("all-MiniLM-L6-v2"));
        assert!(model_path.to_string_lossy().contains("model.onnx"));
        assert!(tokenizer_path.to_string_lossy().contains("tokenizer.json"));
    }

    #[test]
    fn test_default_storage_path() {
        let path = ModelManager::default_storage_path().unwrap();
        assert!(path.to_string_lossy().contains(".cache/outgrep/models"));
    }
}