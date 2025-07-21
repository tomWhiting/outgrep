/*!
Configuration management functionality for outgrep.

This module provides functionality to initialize and open configuration files,
including global and local configurations with proper templates.
*/

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::flags::hierarchy::ConfigHierarchy;

/// Configuration file templates
pub struct ConfigTemplates;

impl ConfigTemplates {
    /// Global configuration template with examples and documentation
    pub const GLOBAL_TEMPLATE: &'static str = r#"# Global outgrep configuration
# This file contains default settings for outgrep across all projects
# Uncomment and modify any setting to customize your search behavior
# Priority: CLI flags > Local config > Global config

# ===== SEARCH OPTIONS =====
# Basic search behavior
# --smart-case                    # Search case insensitively if pattern is lowercase
# --case-sensitive              # Execute the search case sensitively
# --ignore-case                 # Search case insensitively
# --fixed-strings               # Treat all patterns as literals instead of regex
# --word-regexp                 # Only show matches surrounded by word boundaries
# --line-regexp                 # Only show matches surrounded by line boundaries
# --invert-match                # Invert matching (print non-matching lines)

# Advanced regex options
# --pcre2                       # Use the PCRE2 regex engine
# --multiline                   # Enable searching across multiple lines
# --multiline-dotall            # Enable "dot all" mode in multiline regex patterns
# --no-unicode                  # Disable Unicode mode for all patterns
# --crlf                        # Treat CRLF (\r\n) as a line terminator
# --null-data                   # Use NUL as a line terminator instead of \n

# Engine configuration
# --engine=default              # Specify regex engine (default, pcre2, auto)
# --dfa-size-limit=10M          # The upper size limit of the regex DFA
# --regex-size-limit=10M        # The size limit of the compiled regex

# Search limits
# --max-count=100               # Limit the number of matching lines per file
# --stop-on-nonmatch            # Stop reading a file once a non-matching line is encountered

# ===== SEMANTIC SEARCH =====
# AI-powered semantic code search (experimental)
# --semantic                    # Enable semantic code search using vector embeddings

# Model configuration
# --semantic-model-path=~/.cache/outgrep/models  # Directory where embedding models are stored
# --semantic-model=all-MiniLM-L6-v2              # Embedding model to use (see model registry for available options)
# --semantic-dimensions=384                      # Number of embedding dimensions (defaults to model's native dimensions)

# ===== OUTPUT OPTIONS =====
# Line display
# --line-number                   # Show line numbers
# --column                        # Show column numbers
# --heading                       # Print file path above clusters of matches
# --no-line-number              # Suppress line numbers
# --with-filename               # Print file path for each matching line
# --no-filename                 # Never print the file path

# Context and formatting
# --context=2                     # Show NUM lines before and after each match
# --before-context=2            # Show NUM lines before each match
# --after-context=2             # Show NUM lines after each match
# --context-separator=--        # String to separate non-contiguous context lines

# Output details
# --byte-offset                 # Print the 0-based byte offset before each line
# --only-matching               # Print only the matched parts of a line
# --trim                        # Remove ASCII whitespace at the beginning of lines
# --vimgrep                     # Print results with every match on its own line
# --passthru                    # Print both matching and non-matching lines

# Colors and highlighting
# --color=auto                    # Control when to use colors (never, auto, always, ansi)
# --colors=match:fg:red         # Configure color settings
# --no-syntax-highlight         # Disable syntax highlighting in AST context mode

# Special output modes
# --count                       # Show count of matching lines for each file
# --count-matches               # Show count of individual matches for each file
# --files-with-matches          # Print only paths with at least one match
# --files-without-match         # Print paths that contain zero matches
# --files                       # Print each file that would be searched
# --json                        # Enable JSON Lines format output

# Advanced output formatting
# --pretty                      # Alias for --color=always --heading --line-number
# --field-context-separator=:   # Set the field context separator
# --field-match-separator=:     # Set the field match separator
# --path-separator=/            # Set the path separator
# --null                        # Follow file paths with NUL byte
# --include-zero                # Print number of matches even if zero

# Buffer control
# --line-buffered               # Use line buffering
# --block-buffered              # Use block buffering

# Column limits
# --max-columns=150             # Omit lines longer than this limit
# --max-columns-preview         # Print a preview for lines exceeding max column limit

# Hyperlinks and hostnames
# --hyperlink-format=default    # Set the format of hyperlinks
# --hostname-bin=hostname       # Control how ripgrep determines hostname

# ===== FILTER OPTIONS =====
# File and directory filtering
# --hidden                        # Search hidden files and directories
# --follow                      # Follow symbolic links while traversing directories
# --max-depth=10                # Limit directory traversal depth
# --one-file-system             # Don't cross file system boundaries

# File size and type filtering
# --max-filesize=10M              # Ignore files larger than NUM in size
# --binary                      # Search binary files
# --text                        # Search binary files as if they were text

# Ignore patterns and files
# --no-ignore                   # Don't respect ignore files
# --no-ignore-dot               # Don't respect .ignore or .rgignore files
# --no-ignore-exclude           # Don't respect repository exclude files
# --no-ignore-files             # Ignore any --ignore-file flags
# --no-ignore-global            # Don't respect global ignore files
# --no-ignore-parent            # Don't respect ignore files in parent directories
# --no-ignore-vcs               # Don't respect VCS ignore files
# --no-require-git              # Respect .gitignore files even outside git repositories
# --ignore-file=/path/to/ignore # Specify gitignore formatted rules files
# --ignore-file-case-insensitive # Process ignore files case insensitively

# Glob patterns
# --glob=!node_modules/*          # Exclude node_modules directories
# --glob=!.git/*                  # Exclude git directories
# --glob=!target/*                # Exclude Rust target directories
# --glob=!build/*                 # Exclude build directories
# --glob=!dist/*                  # Exclude distribution directories
# --glob=!*.min.js              # Exclude minified JavaScript files
# --iglob=!*.LOG                # Include or exclude files (case insensitive)
# --glob-case-insensitive       # Process glob patterns case insensitively

# File type associations
# --type-add=web:*.{html,css,js,ts,jsx,tsx,vue,svelte,astro}
# --type-add=config:*.{yaml,yml,toml,json,ini,conf,env}
# --type-add=docs:*.{md,rst,txt,adoc,tex}
# --type-add=data:*.{csv,tsv,xml,parquet}
# --type-add=scripts:*.{sh,bash,zsh,fish,ps1,bat,cmd}
# --type=rust                   # Only search files matching TYPE
# --type-not=binary             # Do not search files matching TYPE
# --type-clear=config           # Clear the file type globs for TYPE

# Unrestricted search levels
# --unrestricted                # -u: search hidden files
                                # -uu: search hidden files and binary files
                                # -uuu: search everything (no ignore files)

# ===== PERFORMANCE OPTIONS =====
# Threading and memory
# --threads=4                     # Set the approximate number of threads to use
# --mmap                        # Search using memory maps when possible

# Preprocessing
# --pre=command                 # For each input PATH, search the standard output of COMMAND PATH
# --pre-glob=*.pdf              # Limit --pre to files matching GLOB

# Compression support
# --search-zip                  # Search in compressed files (gzip, bzip2, xz, LZ4, LZMA, Brotli, Zstd)

# ===== INPUT OPTIONS =====
# Multiple patterns (can be used multiple times)
# --regexp=pattern1             # A pattern to search for
# --file=/path/to/patterns      # Search for patterns from the given file

# Encoding
# --encoding=utf-8              # Specify the text encoding for all files searched

# ===== LOGGING OPTIONS =====
# Debug and trace information
# --debug                       # Show debug messages
# --trace                       # Show trace messages
# --stats                       # Print aggregate statistics

# Error message control
# --no-messages                 # Suppress some error messages
# --no-ignore-messages          # Suppress ignore file parsing error messages
# --quiet                       # Do not print anything to stdout

# ===== REPLACEMENT OPTIONS =====
# Text replacement
# --replace='replacement text'  # Replace matches with given text

# ===== SORTING OPTIONS =====
# Result ordering
# --sort=path                   # Sort results in ascending order (none, path, modified, accessed, created)
# --sortr=modified              # Sort results in descending order

# ===== AST AND SEMANTIC FEATURES =====
# Advanced code understanding
# --enclosing-symbol            # Show the entire enclosing symbol around each match

# ===== COMPATIBILITY OPTIONS =====
# Legacy and compatibility
# --no-config                   # Never read configuration files

# ===== EXAMPLES =====
# Common configuration patterns:

# For fast searching in large codebases:
# --threads=8
# --max-filesize=50M
# --mmap

# For detailed output with context:
# --context=5
# --line-number
# --column
# --heading

# For semantic code search:
# --semantic
# --enclosing-symbol

# For case-insensitive search with highlighting:
# --ignore-case
# --color=always
# --heading

# For searching compressed files:
# --search-zip
# --text
"#;

    /// Local/project configuration template
    pub const LOCAL_TEMPLATE: &'static str = r#"# Project-specific outgrep configuration
# These settings override global defaults for this project

# Project-specific search settings
# --context=5
# --semantic
# --semantic-similarity-threshold=0.7

# Semantic search model configuration for this project
# --semantic-model-path=./project-models     # Local model directory
# --semantic-model=all-mpnet-base-v2         # Use higher-quality model for this project
# --semantic-dimensions=768                  # Match model dimensions

# Project-specific file types
# --type-add=myproject:*.{custom,ext}

# Project-specific ignore patterns
# --glob=!build/*
# --glob=!dist/*
# --glob=!vendor/*
# --glob=!*.generated.*

# Language-specific settings for this project
# --type-add=proto:*.proto
# --type-add=schema:*.graphql

# Performance tuning for large codebases
# --threads=8
# --max-filesize=50M
"#;
}

/// Configuration management operations
pub struct ConfigManager;

impl ConfigManager {
    /// Initialize global configuration file
    pub fn init_global_config(force: bool) -> Result<PathBuf> {
        let config_path = ConfigHierarchy::default_global_config_path()?;

        // Check if config already exists
        if config_path.exists() && !force {
            anyhow::bail!(
                "Global config file already exists at: {}\nUse --force to overwrite",
                config_path.display()
            );
        }

        // Create parent directories
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create config directory: {}",
                    parent.display()
                )
            })?;
        }

        // Write template content
        fs::write(&config_path, ConfigTemplates::GLOBAL_TEMPLATE)
            .with_context(|| {
                format!(
                    "Failed to write config file: {}",
                    config_path.display()
                )
            })?;

        Ok(config_path)
    }

    /// Initialize local configuration file
    pub fn init_local_config(force: bool) -> Result<PathBuf> {
        let config_path = ConfigHierarchy::default_local_config_path()?;

        // Check if config already exists
        if config_path.exists() && !force {
            anyhow::bail!(
                "Local config file already exists at: {}\nUse --force to overwrite",
                config_path.display()
            );
        }

        // Create parent directories
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create config directory: {}",
                    parent.display()
                )
            })?;
        }

        // Write template content
        fs::write(&config_path, ConfigTemplates::LOCAL_TEMPLATE)
            .with_context(|| {
                format!(
                    "Failed to write config file: {}",
                    config_path.display()
                )
            })?;

        Ok(config_path)
    }

    /// Open global configuration file in editor
    pub fn open_global_config() -> Result<()> {
        let config_paths = ConfigHierarchy::global_config_paths();

        // Find existing config file
        let config_path = config_paths
            .iter()
            .find(|path| path.exists())
            .cloned()
            .unwrap_or_else(|| {
                // If no config exists, use the default path
                ConfigHierarchy::default_global_config_path()
                    .unwrap_or_else(|_| config_paths[0].clone())
            });

        // If config doesn't exist, offer to create it
        if !config_path.exists() {
            anyhow::bail!(
                "Global config file doesn't exist at: {}\nRun 'outgrep --init-global-config' to create it first",
                config_path.display()
            );
        }

        Self::open_file_in_editor(&config_path)
    }

    /// Open local configuration file in editor
    pub fn open_local_config() -> Result<()> {
        let config_path = ConfigHierarchy::default_local_config_path()?;

        // If config doesn't exist, offer to create it
        if !config_path.exists() {
            anyhow::bail!(
                "Local config file doesn't exist at: {}\nRun 'outgrep --init-local-config' to create it first",
                config_path.display()
            );
        }

        Self::open_file_in_editor(&config_path)
    }

    /// Show current configuration status
    pub fn show_config_status() -> Result<()> {
        let hierarchy = ConfigHierarchy::load()?;

        println!("Configuration Status:");
        println!("====================");

        // Show global config
        match &hierarchy.global_config {
            Some(config) => {
                println!("Global config: {} (loaded)", config.path.display());
                println!("  {} arguments loaded", config.args.len());
            }
            None => {
                let default_path =
                    ConfigHierarchy::default_global_config_path()
                        .unwrap_or_else(|_| {
                            PathBuf::from("~/.config/outgrep/config")
                        });
                println!(
                    "Global config: {} (not found)",
                    default_path.display()
                );
            }
        }

        // Show local config
        match &hierarchy.local_config {
            Some(config) => {
                println!("Local config:  {} (loaded)", config.path.display());
                println!("  {} arguments loaded", config.args.len());
            }
            None => {
                let default_path =
                    ConfigHierarchy::default_local_config_path()
                        .unwrap_or_else(|_| PathBuf::from(".outgrep/config"));
                println!(
                    "Local config:  {} (not found)",
                    default_path.display()
                );
            }
        }

        println!();
        println!(
            "Priority order: CLI arguments > Local config > Global config"
        );

        Ok(())
    }

    /// Detect and launch file in user's preferred editor
    fn open_file_in_editor(file_path: &Path) -> Result<()> {
        let (editor_cmd, editor_args) = Self::detect_editor()?;

        let mut command = Command::new(&editor_cmd);

        // Add any editor-specific arguments
        for arg in &editor_args {
            command.arg(arg);
        }

        // Handle special cases for different editors
        if editor_cmd.file_name().and_then(|s| s.to_str()) == Some("open") {
            // macOS open command needs -t flag for text files
            command.arg("-t");
        }

        command.arg(file_path);

        let status = command.status().with_context(|| {
            format!(
                "Failed to launch editor: {} {}",
                editor_cmd.display(),
                editor_args.join(" ")
            )
        })?;

        if !status.success() {
            anyhow::bail!("Editor exited with non-zero status: {}", status);
        }

        Ok(())
    }

    /// Detect user's preferred editor, returning (command, args)
    fn detect_editor() -> Result<(PathBuf, Vec<String>)> {
        // Check EDITOR environment variable first
        if let Ok(editor) = env::var("EDITOR") {
            // Parse the editor command which might include arguments
            let parts: Vec<&str> = editor.split_whitespace().collect();
            if parts.is_empty() {
                anyhow::bail!("EDITOR environment variable is empty");
            }

            let cmd = PathBuf::from(parts[0]);
            let args = parts[1..].iter().map(|s| s.to_string()).collect();
            return Ok((cmd, args));
        }

        // Platform-specific fallbacks
        #[cfg(target_os = "windows")]
        let candidates = &["notepad.exe", "code.exe", "notepad++.exe"];

        #[cfg(target_os = "macos")]
        let candidates = &["nano", "vim", "vi", "open"];

        #[cfg(all(unix, not(target_os = "macos")))]
        let candidates = &["nano", "vim", "vi", "gedit"];

        for editor in candidates {
            if which::which(editor).is_ok() {
                return Ok((PathBuf::from(editor), vec![]));
            }
        }

        anyhow::bail!(
            "No suitable editor found. Please set the EDITOR environment variable.\n\
             Example: export EDITOR=nano"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // #[test]
    // fn test_global_template_valid() {
    //     // Ensure the template parses without errors
    //     let lines: Vec<&str> = ConfigTemplates::GLOBAL_TEMPLATE
    //         .lines()
    //         .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
    //         .collect();

    //     // Should have some non-comment lines
    //     assert!(!lines.is_empty());

    //     // All non-comment lines should start with --
    //     for line in lines {
    //         assert!(line.trim().starts_with("--"), "Invalid flag: {}", line);
    //     }
    // }

    #[test]
    fn test_local_template_valid() {
        // Ensure the template parses without errors
        let lines: Vec<&str> = ConfigTemplates::LOCAL_TEMPLATE
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
            .collect();

        // Local template is mostly comments/examples, so it might be empty
        // Just ensure it doesn't cause parse errors
        for line in lines {
            assert!(line.trim().starts_with("--"), "Invalid flag: {}", line);
        }
    }

    #[test]
    fn test_config_initialization() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config");

        // Write a config file
        fs::write(&config_path, ConfigTemplates::GLOBAL_TEMPLATE).unwrap();

        // Verify it was written
        assert!(config_path.exists());

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("--smart-case"));
        assert!(content.contains("# Global outgrep configuration"));
    }

    #[test]
    fn test_editor_detection() {
        // This test might fail in CI environments without editors
        // So we just test that it either finds an editor or fails gracefully
        match ConfigManager::detect_editor() {
            Ok((editor_path, _args)) => {
                assert!(!editor_path.as_os_str().is_empty());
            }
            Err(e) => {
                assert!(e.to_string().contains("No suitable editor found"));
            }
        }
    }
}
