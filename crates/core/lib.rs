/*!
Outgrep Core Library API

Exposes all Outgrep functionality for integration with external applications like GraphMother.
*/

// Declare modules (same as main.rs)
#[macro_use]
mod messages;

pub mod diagnostics;
pub mod flags;
pub mod haystack;
mod logger;
pub mod search;

// Re-export AST functionality from workspace crates
pub use outgrep_ast_config as ast_config;
pub use outgrep_ast_core as ast_core;
pub use outgrep_ast_language as ast_language;
pub use outgrep_ast_lsp as ast_lsp;

// Re-export diagnostics types
pub use crate::diagnostics::{
    FileWatcher, GitAnalyzer, MetricsCalculator, TreeBuilder, TreeDisplay,
    TreeDisplayOptions,
};

// Note: Core search, flags, and haystack types are available through their modules
// Individual types are pub(crate) and can't be re-exported, but modules provide full access

// Common error types
pub use anyhow::{Error, Result};

/// High-level API for external integrations like GraphMother
pub mod api {
    use crate::diagnostics;
    use anyhow::Result;
    use std::path::{Path, PathBuf};

    /*
    /// Extract all symbols from a file
    /// TODO: Implement once extract_ast_structure API is finalized
    pub fn extract_symbols(file_path: &Path) -> Result<diagnostics::AstSymbolSummary> {
        diagnostics::extract_ast_structure(file_path)
            .map(|ast| ast.symbols)
            .ok_or_else(|| anyhow::anyhow!("Failed to extract symbols from {}", file_path.display()))
    }

    /// Extract full AST structure from a file
    /// TODO: Implement once extract_ast_structure API is finalized
    pub fn extract_ast(file_path: &Path) -> Result<diagnostics::AstStructure> {
        diagnostics::extract_ast_structure(file_path)
            .ok_or_else(|| anyhow::anyhow!("Failed to extract AST from {}", file_path.display()))
    }

    /// Watch directory for changes
    /// TODO: Implement once FileWatcher streaming API is ready
    pub fn watch_directory(path: &Path) -> Result<impl futures::Stream<Item = PathBuf>> {
        let watcher = diagnostics::FileWatcher::new(path.to_path_buf())?;
        Ok(watcher.watch())
    }

    /// Build project tree
    /// TODO: Verify TreeBuilder::build_tree API signature
    pub fn build_tree(path: &Path) -> Result<diagnostics::TreeDisplay> {
        let mut builder = diagnostics::TreeBuilder::new();
        builder.build_tree(path)
    }

    /// Calculate code metrics for a file or directory
    /// TODO: Verify MetricsCalculator API signature
    pub fn calculate_metrics(path: &Path) -> Result<diagnostics::CodeMetrics> {
        let calculator = diagnostics::MetricsCalculator::new();
        calculator.calculate_metrics(path)
    }

    /// Analyze git repository information
    /// TODO: Verify GitAnalyzer API signature
    pub fn analyze_git(repo_path: &Path) -> Result<diagnostics::GitAnalysis> {
        let analyzer = diagnostics::GitAnalyzer::new(repo_path.to_path_buf())?;
        analyzer.analyze()
    }
    */
}

// TODO: Re-export API functions once implemented
// pub use api::*;
