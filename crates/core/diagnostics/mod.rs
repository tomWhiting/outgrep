pub mod watcher;
pub mod types;
pub mod metrics;
pub mod git;
pub mod tree;
pub mod compiler;
pub mod ast_extractor;

#[cfg(test)]
mod test_watcher;

pub use watcher::FileWatcher;
pub use types::*;
pub use metrics::MetricsCalculator;
pub use git::GitAnalyzer;
pub use tree::{TreeBuilder, TreeDisplay, TreeDisplayOptions};
// CompilerDiagnosticsRunner is used internally by TreeBuilder
pub use ast_extractor::extract_ast_structure;