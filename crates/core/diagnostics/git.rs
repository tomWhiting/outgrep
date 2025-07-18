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