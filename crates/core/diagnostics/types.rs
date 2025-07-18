use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeMetrics {
    pub lines_of_code: u64,
    pub comment_lines: u64,
    pub blank_lines: u64,
    pub cyclomatic_complexity: u32,
    pub cognitive_complexity: u32,
    pub function_count: u32,
}

#[derive(Debug, Clone)]
pub struct FileIndex {
    pub path: PathBuf,
    pub last_modified: SystemTime,
    pub language: Option<String>,
    pub metrics: Option<CodeMetrics>,
    pub dirty: bool,
}

#[derive(Debug, Clone)]
pub struct GitDiagnostics {
    pub status: GitFileStatus,
    pub staged: bool,
    pub diff_stats: Option<DiffStats>,
    pub last_commit: Option<String>,
}

#[derive(Debug, Clone)]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Untracked,
    Clean,
}

#[derive(Debug, Clone)]
pub struct DiffStats {
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone)]
pub enum FileChangeEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}