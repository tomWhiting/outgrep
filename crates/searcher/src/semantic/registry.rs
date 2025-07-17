/*!
Model registry for semantic search models.

This module handles loading, parsing, and managing the model registry
that defines available embedding models for semantic search.
*/

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Performance characteristics of a model
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelPerformance {
    /// Speed category: "fast", "medium", "slow"
    pub speed: String,
    /// Quality category: "good", "very-good", "excellent"
    pub quality: String,
    /// Memory usage in MB
    pub memory_mb: u64,
    /// Average inference time in milliseconds
    pub inference_time_ms: u64,
}

/// File information for model components
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelFile {
    /// Download URL
    pub url: String,
    /// Local filename
    pub filename: String,
    /// SHA256 hash for verification
    pub sha256: String,
    /// File size in bytes
    pub size_bytes: u64,
}

/// Model files required for inference
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelFiles {
    /// ONNX model file
    pub model: ModelFile,
    /// Tokenizer configuration file
    pub tokenizer: ModelFile,
}

/// Complete model definition
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelInfo {
    /// Model identifier name
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Embedding vector dimensions
    pub dimensions: usize,
    /// Total model size in MB
    pub size_mb: u64,
    /// License identifier
    pub license: String,
    /// Performance characteristics
    pub performance: ModelPerformance,
    /// Required files for this model
    pub files: ModelFiles,
    /// Use case categories
    pub use_cases: Vec<String>,
    /// Supported languages
    pub supported_languages: Vec<String>,
    /// Recommendation text
    pub recommended_for: String,
}

/// Model source information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelSource {
    /// Base URL for this source
    pub base_url: String,
    /// Description of the source
    pub description: String,
    /// Reliability rating
    pub reliability: String,
    /// Whether this source uses a CDN
    pub cdn: bool,
}

/// Complete model registry
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelRegistry {
    /// Registry format version
    pub version: String,
    /// Last updated timestamp
    pub updated: String,
    /// Registry description
    pub description: String,
    /// Available models
    pub models: HashMap<String, ModelInfo>,
    /// Recommended models for different use cases
    pub recommendations: HashMap<String, String>,
    /// Model categories
    pub categories: HashMap<String, Vec<String>>,
    /// Model sources
    pub sources: HashMap<String, ModelSource>,
}

impl ModelRegistry {
    /// Load registry from a JSON file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read model registry: {}", path.display()))?;
        
        let registry: ModelRegistry = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse model registry: {}", path.display()))?;
        
        Ok(registry)
    }

    /// Load registry from embedded default
    pub fn load_default() -> Result<Self> {
        const DEFAULT_REGISTRY: &str = include_str!("../../../../model-registry.json");
        let registry: ModelRegistry = serde_json::from_str(DEFAULT_REGISTRY)
            .context("Failed to parse embedded model registry")?;
        Ok(registry)
    }

    /// Load registry with fallback priority: user -> project -> default
    pub fn load_with_fallback(model_path: Option<&Path>) -> Result<Self> {
        // Try user-specified path first
        if let Some(path) = model_path {
            if path.exists() {
                return Self::load_from_file(path);
            }
        }

        // Try user config directory
        if let Some(home_dir) = dirs::home_dir() {
            let user_registry = home_dir.join(".config/outgrep/model-registry.json");
            if user_registry.exists() {
                return Self::load_from_file(user_registry);
            }
        }

        // Try project-local registry
        let project_registry = Path::new(".outgrep/model-registry.json");
        if project_registry.exists() {
            return Self::load_from_file(project_registry);
        }

        // Fall back to embedded default
        Self::load_default()
    }

    /// Get model information by name
    pub fn get_model(&self, name: &str) -> Option<&ModelInfo> {
        self.models.get(name)
    }

    /// Get recommended model for a use case
    pub fn get_recommended(&self, use_case: &str) -> Option<&ModelInfo> {
        let model_name = self.recommendations.get(use_case)?;
        self.get_model(model_name)
    }

    /// List all available model names
    pub fn list_models(&self) -> Vec<&String> {
        self.models.keys().collect()
    }

    /// List models by category
    pub fn list_by_category(&self, category: &str) -> Vec<&ModelInfo> {
        self.categories
            .get(category)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| self.get_model(name))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Validate that a model exists and return its info
    pub fn validate_model(&self, name: &str) -> Result<&ModelInfo> {
        self.get_model(name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown model '{}'. Available models: {}",
                    name,
                    self.list_models().iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                )
            })
    }

    /// Get model storage path for a given model
    pub fn get_model_storage_path(&self, model_name: &str, base_path: &Path) -> Option<PathBuf> {
        let _model_info = self.get_model(model_name)?;
        Some(base_path.join(model_name))
    }

    /// Check if model files exist locally
    pub fn model_exists_locally(&self, model_name: &str, base_path: &Path) -> bool {
        if let Some(model_info) = self.get_model(model_name) {
            let model_dir = base_path.join(model_name);
            let model_file = model_dir.join(&model_info.files.model.filename);
            let tokenizer_file = model_dir.join(&model_info.files.tokenizer.filename);
            
            model_file.exists() && tokenizer_file.exists()
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_default_registry() {
        let registry = ModelRegistry::load_default().unwrap();
        assert!(!registry.models.is_empty());
        assert!(registry.models.contains_key("all-MiniLM-L6-v2"));
        assert_eq!(registry.version, "1.0");
    }

    #[test]
    fn test_get_model() {
        let registry = ModelRegistry::load_default().unwrap();
        let model = registry.get_model("all-MiniLM-L6-v2").unwrap();
        assert_eq!(model.name, "all-MiniLM-L6-v2");
        assert_eq!(model.dimensions, 384);
    }

    #[test]
    fn test_get_recommended() {
        let registry = ModelRegistry::load_default().unwrap();
        let default_model = registry.get_recommended("default").unwrap();
        assert_eq!(default_model.name, "all-MiniLM-L6-v2");
    }

    #[test]
    fn test_validate_model() {
        let registry = ModelRegistry::load_default().unwrap();
        
        // Valid model
        assert!(registry.validate_model("all-MiniLM-L6-v2").is_ok());
        
        // Invalid model
        assert!(registry.validate_model("nonexistent-model").is_err());
    }

    #[test]
    fn test_model_storage_path() {
        let registry = ModelRegistry::load_default().unwrap();
        let base_path = Path::new("/tmp/models");
        
        let storage_path = registry.get_model_storage_path("all-MiniLM-L6-v2", base_path).unwrap();
        assert_eq!(storage_path, base_path.join("all-MiniLM-L6-v2"));
    }

    #[test]
    fn test_list_by_category() {
        let registry = ModelRegistry::load_default().unwrap();
        let fast_models = registry.list_by_category("fast");
        assert!(!fast_models.is_empty());
        
        // Verify fast models have expected performance characteristics
        for model in fast_models {
            assert_eq!(model.performance.speed, "fast");
        }
    }
}