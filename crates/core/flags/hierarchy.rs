/*!
Hierarchical configuration system for outgrep.

This module implements a hierarchical configuration system with the following precedence:
1. CLI flags (highest priority)
2. Local/repository config files 
3. Global config files (lowest priority)

Config files are discovered automatically using standard locations and project detection.
*/

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Represents a configuration source with its origin
#[derive(Debug, Clone)]
pub struct ConfigSource {
    pub path: PathBuf,
    pub args: Vec<OsString>,
    pub source_type: ConfigType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigType {
    Global,
    Local,
    Cli,
}

/// Hierarchical configuration manager
pub struct ConfigHierarchy {
    pub global_config: Option<ConfigSource>,
    pub local_config: Option<ConfigSource>,
}

impl ConfigHierarchy {
    /// Load configuration hierarchy from standard locations
    pub fn load() -> Result<Self> {
        let global_config = Self::find_global_config()?;
        let local_config = Self::find_local_config()?;

        Ok(Self {
            global_config,
            local_config,
        })
    }

    /// Merge all configuration sources with proper precedence
    /// Order: CLI args are added last (highest priority)
    pub fn merge_args(&self, cli_args: Vec<OsString>) -> Vec<OsString> {
        let mut merged_args = Vec::new();

        // Add global config args first (lowest priority)
        if let Some(ref global) = self.global_config {
            merged_args.extend(global.args.clone());
        }

        // Add local config args second (medium priority)
        if let Some(ref local) = self.local_config {
            merged_args.extend(local.args.clone());
        }

        // Add CLI args last (highest priority)
        merged_args.extend(cli_args);

        merged_args
    }

    /// Find global configuration file
    fn find_global_config() -> Result<Option<ConfigSource>> {
        let global_paths = Self::global_config_paths();

        for path in global_paths {
            if path.exists() {
                let args = Self::parse_config_file(&path)
                    .with_context(|| format!("Failed to parse global config: {}", path.display()))?;
                
                return Ok(Some(ConfigSource {
                    path,
                    args,
                    source_type: ConfigType::Global,
                }));
            }
        }

        Ok(None)
    }

    /// Find local/repository configuration file
    fn find_local_config() -> Result<Option<ConfigSource>> {
        let current_dir = env::current_dir()
            .context("Failed to get current directory")?;

        if let Some(project_root) = Self::find_project_root(&current_dir) {
            let local_paths = Self::local_config_paths(&project_root);

            for path in local_paths {
                if path.exists() {
                    let args = Self::parse_config_file(&path)
                        .with_context(|| format!("Failed to parse local config: {}", path.display()))?;
                    
                    return Ok(Some(ConfigSource {
                        path,
                        args,
                        source_type: ConfigType::Local,
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Get standard global config file paths in priority order
    pub fn global_config_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Use ~/.config/outgrep/config as the primary location
        if let Some(home_dir) = dirs::home_dir() {
            paths.push(home_dir.join(".config").join("outgrep").join("config"));
            paths.push(home_dir.join(".outgrep"));
        }

        paths
    }

    /// Get local config file paths for a project root
    pub fn local_config_paths(project_root: &Path) -> Vec<PathBuf> {
        vec![
            project_root.join(".outgrep").join("config"),
            project_root.join(".outgrep"),
            project_root.join(".ripgreprc"),
        ]
    }

    /// Find project root by traversing up directory tree
    pub fn find_project_root(start_dir: &Path) -> Option<PathBuf> {
        let mut current = start_dir;

        loop {
            // Check for version control directories
            if current.join(".git").exists() {
                return Some(current.to_path_buf());
            }

            // Check for common project files
            for marker in &["Cargo.toml", "package.json", ".outgrep", "pyproject.toml", "go.mod"] {
                if current.join(marker).exists() {
                    return Some(current.to_path_buf());
                }
            }

            // Move up to parent directory
            match current.parent() {
                Some(parent) => current = parent,
                None => break,
            }
        }

        None
    }

    /// Parse a configuration file into command line arguments
    fn parse_config_file(path: &Path) -> Result<Vec<OsString>> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let mut args = Vec::new();

        for line in contents.lines() {
            let line = line.trim();
            
            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // For now, treat each line as a single argument
            // TODO: Add support for TOML format in the future
            args.push(OsString::from(line));
        }

        Ok(args)
    }

    /// Get the path where a global config file should be created
    pub fn default_global_config_path() -> Result<PathBuf> {
        let home_dir = dirs::home_dir()
            .context("Could not determine home directory")?;

        Ok(home_dir.join(".config").join("outgrep").join("config"))
    }

    /// Get the path where a local config file should be created
    pub fn default_local_config_path() -> Result<PathBuf> {
        let current_dir = env::current_dir()
            .context("Failed to get current directory")?;

        let project_root = Self::find_project_root(&current_dir)
            .unwrap_or(current_dir);

        Ok(project_root.join(".outgrep").join("config"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_project_root_detection() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create a mock project structure
        let project_dir = base_path.join("my-project");
        let src_dir = project_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        
        // Create a Cargo.toml file
        fs::write(project_dir.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        // Test detection from subdirectory
        let detected_root = ConfigHierarchy::find_project_root(&src_dir);
        assert_eq!(detected_root, Some(project_dir));
    }

    #[test]
    fn test_config_file_parsing() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config");

        let config_content = r#"
# This is a comment
--smart-case
--hidden

--context=3
"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ConfigHierarchy::parse_config_file(&config_path).unwrap();
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], "--smart-case");
        assert_eq!(args[1], "--hidden");
        assert_eq!(args[2], "--context=3");
    }

    #[test]
    fn test_config_hierarchy_merge() {
        let global_config = ConfigSource {
            path: PathBuf::from("/global/config"),
            args: vec![
                OsString::from("--smart-case"),
                OsString::from("--hidden"),
            ],
            source_type: ConfigType::Global,
        };

        let local_config = ConfigSource {
            path: PathBuf::from("/local/config"),
            args: vec![
                OsString::from("--context=5"),
                OsString::from("--line-number"),
            ],
            source_type: ConfigType::Local,
        };

        let hierarchy = ConfigHierarchy {
            global_config: Some(global_config),
            local_config: Some(local_config),
        };

        let cli_args = vec![OsString::from("--no-hidden")];
        let merged = hierarchy.merge_args(cli_args);

        // Check order: global, local, cli
        assert_eq!(merged[0], "--smart-case");     // global
        assert_eq!(merged[1], "--hidden");        // global
        assert_eq!(merged[2], "--context=5");     // local
        assert_eq!(merged[3], "--line-number");   // local
        assert_eq!(merged[4], "--no-hidden");     // cli (overrides global --hidden)
    }
}