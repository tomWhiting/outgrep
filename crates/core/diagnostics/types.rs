use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;
use std::collections::BTreeMap;

/// AST-related types for syntax tree structure

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstNodeInfo {
    /// The type of AST node (e.g., "function_declaration", "class_definition")
    pub node_type: String,
    /// Byte range of this node in the source
    pub range: std::ops::Range<usize>,
    /// Line and column range for display
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    /// Symbol name if this node represents a named entity
    pub symbol_name: Option<String>,
    /// Child nodes
    pub children: Vec<AstNodeInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxHighlightToken {
    /// Byte range of the token
    pub range: std::ops::Range<usize>,
    /// Token type (e.g., "keyword", "string", "comment")
    pub token_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstStructure {
    /// Programming language detected
    pub language: String,
    /// Root nodes of the AST (typically one per file)
    pub root_nodes: Vec<AstNodeInfo>,
    /// Syntax highlighting tokens
    pub syntax_tokens: Vec<SyntaxHighlightToken>,
    /// Symbol summary - quick access to important symbols
    pub symbols: AstSymbolSummary,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AstSymbolSummary {
    /// Function/method definitions
    pub functions: Vec<SymbolInfo>,
    /// Class/struct/interface definitions
    pub classes: Vec<SymbolInfo>,
    /// Type definitions
    pub types: Vec<SymbolInfo>,
    /// Module/namespace definitions  
    pub modules: Vec<SymbolInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    /// Name of the symbol
    pub name: String,
    /// Type of symbol
    pub symbol_type: String,
    /// Byte range in source
    pub range: std::ops::Range<usize>,
    /// Line number (1-based)
    pub line: u32,
    /// Column number (1-based) 
    pub column: u32,
}

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

/// Tree structure types for directory representation

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TreeNode {
    Directory(DirectoryNode),
    File(FileNode),
}

impl TreeNode {
    /// Get the name of the node
    pub fn name(&self) -> &str {
        match self {
            TreeNode::Directory(dir) => &dir.name,
            TreeNode::File(file) => &file.name,
        }
    }

    /// Get the path of the node
    pub fn path(&self) -> &PathBuf {
        match self {
            TreeNode::Directory(dir) => &dir.path,
            TreeNode::File(file) => &file.path,
        }
    }

    /// Check if this node is a directory
    pub fn is_directory(&self) -> bool {
        matches!(self, TreeNode::Directory(_))
    }

    /// Check if this node is a file
    pub fn is_file(&self) -> bool {
        matches!(self, TreeNode::File(_))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryNode {
    pub name: String,
    pub path: PathBuf,
    pub children: BTreeMap<String, TreeNode>,
    pub git_status: Option<GitFileStatus>,
    pub stats: DirectoryStats,
}

impl DirectoryNode {
    /// Create a new directory node
    pub fn new(name: String, path: PathBuf) -> Self {
        Self {
            name,
            path,
            children: BTreeMap::new(),
            git_status: None,
            stats: DirectoryStats::default(),
        }
    }

    /// Add a child node to this directory
    pub fn add_child(&mut self, child: TreeNode) {
        let name = child.name().to_string();
        self.children.insert(name, child);
    }

    /// Update directory statistics by aggregating from children
    pub fn update_stats(&mut self) {
        let mut stats = DirectoryStats::default();
        
        for child in self.children.values() {
            match child {
                TreeNode::Directory(dir) => {
                    stats.total_directories += 1;
                    stats.total_files += dir.stats.total_files;
                    stats.total_loc += dir.stats.total_loc;
                    stats.total_comments += dir.stats.total_comments;
                    stats.total_functions += dir.stats.total_functions;
                    stats.total_complexity += dir.stats.total_complexity;
                    
                    // Merge language counts
                    for (lang, count) in &dir.stats.languages {
                        *stats.languages.entry(lang.clone()).or_insert(0) += count;
                    }
                }
                TreeNode::File(file) => {
                    stats.total_files += 1;
                    
                    if let Some(metrics) = &file.metrics {
                        stats.total_loc += metrics.lines_of_code;
                        stats.total_comments += metrics.comment_lines;
                        stats.total_functions += metrics.function_count;
                        stats.total_complexity += metrics.cyclomatic_complexity;
                    }
                    
                    if let Some(language) = &file.language {
                        *stats.languages.entry(language.clone()).or_insert(0) += 1;
                    }
                }
            }
        }
        
        self.stats = stats;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub language: Option<String>,
    pub git_status: Option<GitFileStatus>,
    pub metrics: Option<CodeMetrics>,
    pub last_modified: Option<SystemTime>,
    pub diagnostics: Option<FileDiagnostics>,
    pub ast_structure: Option<AstStructure>,
}

impl FileNode {
    /// Create a new file node
    pub fn new(name: String, path: PathBuf) -> Self {
        Self {
            name,
            path,
            language: None,
            git_status: None,
            metrics: None,
            last_modified: None,
            diagnostics: None,
            ast_structure: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DirectoryStats {
    pub total_files: u64,
    pub total_directories: u64,
    pub total_loc: u64,
    pub total_comments: u64,
    pub total_functions: u32,
    pub total_complexity: u32,
    pub languages: BTreeMap<String, u32>,
}

/// Compiler diagnostic types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticLocation {
    pub line: u32,
    pub column: u32,
    pub length: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilerDiagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub code: Option<String>,
    pub location: DiagnosticLocation,
    pub file_path: PathBuf,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileDiagnostics {
    pub errors: Vec<CompilerDiagnostic>,
    pub warnings: Vec<CompilerDiagnostic>,
    pub infos: Vec<CompilerDiagnostic>,
    pub hints: Vec<CompilerDiagnostic>,
}

impl FileDiagnostics {
    pub fn add_diagnostic(&mut self, diagnostic: CompilerDiagnostic) {
        match diagnostic.severity {
            DiagnosticSeverity::Error => self.errors.push(diagnostic),
            DiagnosticSeverity::Warning => self.warnings.push(diagnostic),
            DiagnosticSeverity::Info => self.infos.push(diagnostic),
            DiagnosticSeverity::Hint => self.hints.push(diagnostic),
        }
    }

    pub fn total_count(&self) -> usize {
        self.errors.len() + self.warnings.len() + self.infos.len() + self.hints.len()
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}