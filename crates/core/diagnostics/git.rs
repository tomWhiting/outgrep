use std::path::{Path, PathBuf};
use std::collections::HashMap;
use git2::{Repository, Status, StatusOptions};

use crate::diagnostics::types::{GitDiagnostics, GitFileStatus, DiffStats};

pub struct GitAnalyzer {
    repo: Option<Repository>,
}

impl GitAnalyzer {
    /// Create a new GitAnalyzer for the given directory
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let repo = Repository::discover(path).ok();
        Self { repo }
    }

    /// Check if we're in a Git repository
    pub fn is_git_repo(&self) -> bool {
        self.repo.is_some()
    }

    /// Get the current branch name
    pub fn current_branch(&self) -> Option<String> {
        let repo = self.repo.as_ref()?;
        let head = repo.head().ok()?;
        
        if let Some(name) = head.shorthand() {
            Some(name.to_string())
        } else {
            // Detached HEAD
            let oid = head.target()?;
            Some(format!("detached@{}", &oid.to_string()[..8]))
        }
    }

    /// Get repository status for all files
    pub fn get_status(&self) -> Result<HashMap<PathBuf, GitFileStatus>, Box<dyn std::error::Error>> {
        let repo = match &self.repo {
            Some(repo) => repo,
            None => return Ok(HashMap::new()),
        };

        let mut status_options = StatusOptions::new();
        status_options.include_untracked(true);
        status_options.include_ignored(false);
        
        let statuses = repo.statuses(Some(&mut status_options))?;
        let mut file_statuses = HashMap::new();

        for entry in statuses.iter() {
            let path = PathBuf::from(entry.path().unwrap_or(""));
            let status = entry.status();
            
            let git_status = if status.contains(Status::INDEX_NEW) 
                || status.contains(Status::INDEX_MODIFIED) 
                || status.contains(Status::INDEX_DELETED) 
                || status.contains(Status::INDEX_RENAMED) 
                || status.contains(Status::INDEX_TYPECHANGE) {
                GitFileStatus::Staged
            } else if status.contains(Status::WT_NEW) {
                GitFileStatus::Untracked
            } else if status.contains(Status::WT_MODIFIED) 
                || status.contains(Status::WT_DELETED) 
                || status.contains(Status::WT_RENAMED) 
                || status.contains(Status::WT_TYPECHANGE) {
                GitFileStatus::Modified
            } else if status.contains(Status::CONFLICTED) {
                GitFileStatus::Conflicted
            } else {
                continue; // Skip clean files
            };
            
            file_statuses.insert(path, git_status);
        }

        Ok(file_statuses)
    }

    /// Get repository status filtered by current working directory
    pub fn get_status_for_cwd(&self) -> Result<HashMap<PathBuf, GitFileStatus>, Box<dyn std::error::Error>> {
        let all_statuses = self.get_status()?;
        let repo = match &self.repo {
            Some(repo) => repo,
            None => return Ok(HashMap::new()),
        };

        let repo_root = repo.workdir().ok_or("Repository has no working directory")?;
        let cwd = std::env::current_dir()?;
        
        // Get relative path from repo root to current working directory
        let cwd_relative = if cwd.starts_with(repo_root) {
            cwd.strip_prefix(repo_root).unwrap_or(Path::new(""))
        } else {
            return Ok(HashMap::new()); // Not in repo
        };

        let mut filtered_statuses = HashMap::new();
        
        for (file_path, status) in all_statuses {
            // Check if file is in current working directory or subdirectories
            if cwd_relative.as_os_str().is_empty() {
                // We're at repo root, include all files
                filtered_statuses.insert(file_path, status);
            } else if file_path.starts_with(cwd_relative) {
                // File is in current directory or subdirectories
                // Convert to relative path from current directory
                let relative_to_cwd = file_path.strip_prefix(cwd_relative).unwrap_or(&file_path);
                filtered_statuses.insert(relative_to_cwd.to_path_buf(), status);
            }
        }

        Ok(filtered_statuses)
    }

    /// Get the repository root directory
    pub fn get_repo_root(&self) -> Option<&Path> {
        self.repo.as_ref()?.workdir()
    }

    /// Convert a path to be relative to the repository root
    pub fn path_relative_to_repo(&self, path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let repo_root = self.get_repo_root().ok_or("Repository has no working directory")?;
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        
        let relative_path = absolute_path.strip_prefix(repo_root)
            .map_err(|_| "Path is not within repository")?;
        
        Ok(relative_path.to_path_buf())
    }

    /// Get basic Git diagnostics
    pub fn get_diagnostics(&self) -> Result<GitDiagnostics, Box<dyn std::error::Error>> {
        let repo = match &self.repo {
            Some(repo) => repo,
            None => return Ok(GitDiagnostics {
                is_repo: false,
                current_branch: None,
                total_commits: 0,
                ahead_behind: None,
                file_stats: DiffStats {
                    staged: 0,
                    modified: 0,
                    untracked: 0,
                    conflicted: 0,
                },
            }),
        };

        let current_branch = self.current_branch();
        let total_commits = self.count_commits()?;
        let ahead_behind = self.get_ahead_behind()?;
        let file_stats = self.get_file_stats()?;

        Ok(GitDiagnostics {
            is_repo: true,
            current_branch,
            total_commits,
            ahead_behind,
            file_stats,
        })
    }

    /// Count total commits in the repository
    fn count_commits(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let repo = self.repo.as_ref().unwrap();
        
        let head = match repo.head() {
            Ok(head) => head,
            Err(_) => return Ok(0), // Empty repository
        };
        
        let mut revwalk = repo.revwalk()?;
        revwalk.push_head()?;
        
        Ok(revwalk.count() as u64)
    }

    /// Get ahead/behind count compared to upstream
    fn get_ahead_behind(&self) -> Result<Option<(u64, u64)>, Box<dyn std::error::Error>> {
        let repo = self.repo.as_ref().unwrap();
        
        let head = match repo.head() {
            Ok(head) => head,
            Err(_) => return Ok(None), // Empty repository
        };
        
        let local_oid = match head.target() {
            Some(oid) => oid,
            None => return Ok(None),
        };
        
        // Try to find upstream branch
        let branch_name = match head.shorthand() {
            Some(name) => name,
            None => return Ok(None),
        };
        
        let upstream_name = format!("origin/{}", branch_name);
        let upstream_ref = match repo.find_reference(&upstream_name) {
            Ok(r) => r,
            Err(_) => return Ok(None), // No upstream
        };
        
        let upstream_oid = match upstream_ref.target() {
            Some(oid) => oid,
            None => return Ok(None),
        };
        
        let (ahead, behind) = repo.graph_ahead_behind(local_oid, upstream_oid)?;
        Ok(Some((ahead as u64, behind as u64)))
    }

    /// Get file statistics (staged, modified, untracked, conflicted)
    fn get_file_stats(&self) -> Result<DiffStats, Box<dyn std::error::Error>> {
        let statuses = self.get_status()?;
        
        let mut stats = DiffStats {
            staged: 0,
            modified: 0,
            untracked: 0,
            conflicted: 0,
        };
        
        for status in statuses.values() {
            match status {
                GitFileStatus::Staged => stats.staged += 1,
                GitFileStatus::Modified => stats.modified += 1,
                GitFileStatus::Untracked => stats.untracked += 1,
                GitFileStatus::Conflicted => stats.conflicted += 1,
            }
        }
        
        Ok(stats)
    }

    /// Format Git status for display
    pub fn format_status(&self, status: &GitFileStatus) -> &'static str {
        match status {
            GitFileStatus::Staged => "📁", // Staged
            GitFileStatus::Modified => "📝", // Modified
            GitFileStatus::Untracked => "❓", // Untracked
            GitFileStatus::Conflicted => "⚠️", // Conflicted
        }
    }

    /// Get a summary string of Git diagnostics
    pub fn diagnostics_summary(&self, diagnostics: &GitDiagnostics) -> String {
        if !diagnostics.is_repo {
            return "Not a Git repository".to_string();
        }

        let branch = diagnostics.current_branch.as_deref().unwrap_or("unknown");
        let commits = diagnostics.total_commits;
        let stats = &diagnostics.file_stats;
        
        let ahead_behind = match &diagnostics.ahead_behind {
            Some((ahead, behind)) if *ahead > 0 || *behind > 0 => {
                format!(" (↑{} ↓{})", ahead, behind)
            }
            _ => String::new(),
        };
        
        format!(
            "Branch: {}{} | Commits: {} | Changes: {} staged, {} modified, {} untracked, {} conflicted",
            branch, ahead_behind, commits, stats.staged, stats.modified, stats.untracked, stats.conflicted
        )
    }

    /// Get the content of a file at HEAD for diff comparison
    pub fn get_file_at_head(&self, path: &Path) -> Result<String, Box<dyn std::error::Error>> {
        let repo = match &self.repo {
            Some(repo) => repo,
            None => return Err("Not a Git repository".into()),
        };

        let head = repo.head()?;
        let head_commit = head.peel_to_commit()?;
        let head_tree = head_commit.tree()?;
        
        // Convert path to relative path from repo root
        let relative_path_buf = self.path_relative_to_repo(path)?;
        let relative_path = relative_path_buf.as_path();
        
        // Get the tree entry for this path
        let tree_entry = head_tree.get_path(relative_path)?;
        let object = tree_entry.to_object(repo)?;
        
        // Convert to blob and get content
        let blob = object.into_blob().map_err(|_| "Object is not a blob")?;
        let content = blob.content();
        
        // Convert bytes to string
        Ok(String::from_utf8_lossy(content).to_string())
    }

    /// Get semantic diff for a file using diffsitter
    pub fn get_semantic_diff(&self, path: &Path) -> Result<String, Box<dyn std::error::Error>> {
        // Get current file content
        let current_content = std::fs::read_to_string(path)?;
        
        // Get HEAD content - need to handle path resolution properly
        let head_content = match self.get_file_at_head(path) {
            Ok(content) => content,
            Err(_) => {
                // If direct path fails, try to resolve relative to current working directory
                let cwd = std::env::current_dir()?;
                let absolute_path = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    cwd.join(path)
                };
                self.get_file_at_head(&absolute_path)?
            }
        };
        
        // Try to get file extension for language detection
        let language = path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("txt");
        
        // Use diffsitter to generate the diff
        let diff_output = self.run_diffsitter(&head_content, &current_content, language)?;
        
        Ok(diff_output)
    }

    /// Run diffsitter to generate semantic diff
    fn run_diffsitter(&self, old_content: &str, new_content: &str, language: &str) -> Result<String, Box<dyn std::error::Error>> {
        use std::process::Command;
        use std::io::Write;
        
        // Create temporary files
        let mut old_file = tempfile::NamedTempFile::new()?;
        let mut new_file = tempfile::NamedTempFile::new()?;
        
        // Write content to temporary files
        old_file.write_all(old_content.as_bytes())?;
        new_file.write_all(new_content.as_bytes())?;
        
        // Run diffsitter
        let output = Command::new("diffsitter")
            .arg("--color=always")
            .arg("--language")
            .arg(language)
            .arg(old_file.path())
            .arg(new_file.path())
            .output();
        
        match output {
            Ok(output) => {
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    // Fall back to simple diff if diffsitter fails
                    self.fallback_diff(old_content, new_content)
                }
            }
            Err(_) => {
                // Fall back to simple diff if diffsitter is not available
                self.fallback_diff(old_content, new_content)
            }
        }
    }

    /// Fallback to simple diff if diffsitter is not available
    fn fallback_diff(&self, old_content: &str, new_content: &str) -> Result<String, Box<dyn std::error::Error>> {
        use similar::{ChangeTag, TextDiff};
        
        let diff = TextDiff::from_lines(old_content, new_content);
        let mut output = String::new();
        let mut has_changes = false;
        
        // Group changes into hunks with context
        for group in diff.grouped_ops(3) {
            if !has_changes {
                has_changes = true;
            } else {
                output.push_str("\x1b[90m...\x1b[0m\n"); // Gray separator
            }
            
            for op in &group {
                for change in diff.iter_changes(op) {
                    let (sign, color) = match change.tag() {
                        ChangeTag::Delete => ("-", "\x1b[31m"), // Red for deletions
                        ChangeTag::Insert => ("+", "\x1b[32m"), // Green for insertions
                        ChangeTag::Equal => (" ", "\x1b[90m"),  // Gray for context
                    };
                    
                    // Only show context lines (Equal) around changes, not all of them
                    match change.tag() {
                        ChangeTag::Delete | ChangeTag::Insert => {
                            output.push_str(&format!("{}{}{}\x1b[0m", color, sign, change));
                        }
                        ChangeTag::Equal => {
                            // Only show context lines, not all equal lines  
                            output.push_str(&format!("{}{}{}\x1b[0m", color, sign, change));
                        }
                    }
                }
            }
        }
        
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_non_git_directory() {
        let temp_dir = TempDir::new().unwrap();
        let analyzer = GitAnalyzer::new(temp_dir.path());
        
        assert!(!analyzer.is_git_repo());
        assert_eq!(analyzer.current_branch(), None);
        
        let diagnostics = analyzer.get_diagnostics().unwrap();
        assert!(!diagnostics.is_repo);
        assert_eq!(diagnostics.total_commits, 0);
    }

    #[test]
    fn test_git_analyzer_creation() {
        // Test with current directory (should be a git repo)
        let analyzer = GitAnalyzer::new(".");
        
        // This should be true since we're in a git repo
        assert!(analyzer.is_git_repo());
        
        // Should have a current branch
        assert!(analyzer.current_branch().is_some());
        
        // Should be able to get diagnostics
        let diagnostics = analyzer.get_diagnostics().unwrap();
        assert!(diagnostics.is_repo);
        assert!(diagnostics.total_commits > 0);
    }

    #[test]
    fn test_diagnostics_summary() {
        let analyzer = GitAnalyzer::new(".");
        
        let diagnostics = GitDiagnostics {
            is_repo: true,
            current_branch: Some("main".to_string()),
            total_commits: 42,
            ahead_behind: Some((2, 1)),
            file_stats: DiffStats {
                staged: 3,
                modified: 2,
                untracked: 1,
                conflicted: 0,
            },
        };
        
        let summary = analyzer.diagnostics_summary(&diagnostics);
        assert!(summary.contains("Branch: main"));
        assert!(summary.contains("Commits: 42"));
        assert!(summary.contains("3 staged"));
        assert!(summary.contains("↑2 ↓1"));
    }
}