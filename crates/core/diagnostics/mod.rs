pub mod watcher;
pub mod types;
pub mod metrics;
pub mod git;
pub mod tree;
pub mod compiler;

#[cfg(test)]
mod test_watcher;

pub use watcher::FileWatcher;
pub use types::*;
pub use metrics::MetricsCalculator;
pub use git::GitAnalyzer;
pub use tree::{TreeBuilder, TreeDisplay, TreeDisplayOptions};
pub use compiler::CompilerDiagnosticsRunner;