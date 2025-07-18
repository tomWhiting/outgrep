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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiagnostics {
    pub is_repo: bool,
    pub current_branch: Option<String>,
    pub total_commits: u64,
    pub ahead_behind: Option<(u64, u64)>, // (ahead, behind)
    pub file_stats: DiffStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GitFileStatus {
    Modified,
    Staged,
    Untracked,
    Conflicted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffStats {
    pub staged: u64,
    pub modified: u64,
    pub untracked: u64,
    pub conflicted: u64,
}

#[derive(Debug, Clone)]
pub enum FileChangeEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}