use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::diagnostics::types::{TreeNode, DirectoryNode, FileNode, GitFileStatus, FileDiagnostics};
use crate::diagnostics::{MetricsCalculator, GitAnalyzer};
use crate::diagnostics::compiler::CompilerDiagnosticsRunner;

/// Builder for constructing directory trees with metrics and git information
pub struct TreeBuilder {
    git_analyzer: GitAnalyzer,
    git_status: HashMap<PathBuf, GitFileStatus>,
    workspace_diagnostics: HashMap<PathBuf, FileDiagnostics>,
    options: TreeDisplayOptions,
}

impl TreeBuilder {
    /// Create a new tree builder for the given directory
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self::with_options(path, TreeDisplayOptions::default())
    }

    /// Create a new tree builder with specific display options
    pub fn with_options<P: AsRef<Path>>(path: P, options: TreeDisplayOptions) -> Self {
        let git_analyzer = GitAnalyzer::new(&path);
        let git_status = git_analyzer.get_status_for_cwd().unwrap_or_default();
        
        // Run workspace-wide diagnostics once if diagnostics are enabled
        let workspace_diagnostics = if options.show_diagnostics {
            Self::run_workspace_diagnostics(&path)
        } else {
            HashMap::new()
        };
        
        Self {
            git_analyzer,
            git_status,
            workspace_diagnostics,
            options,
        }
    }
    
    /// Build a directory tree from the given root path
    pub fn build_tree<P: AsRef<Path>>(&self, root: P) -> anyhow::Result<TreeNode> {
        let root_path = root.as_ref();
        let mut root_node = DirectoryNode::new(
            root_path.file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("."))
                .to_string_lossy()
                .to_string(),
            root_path.to_path_buf(),
        );
        
        // Walk the directory tree
        let walker = ignore::WalkBuilder::new(root_path)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .ignore(true)
            .parents(true)
            .build();
        
        let mut all_entries: Vec<_> = walker
            .filter_map(|result| result.ok())
            .collect();
        
        // Sort entries to ensure consistent tree building
        all_entries.sort_by(|a, b| a.path().cmp(b.path()));
        
        for entry in all_entries {
            let path = entry.path();
            
            // Skip the root directory itself
            if path == root_path {
                continue;
            }
            
            // Skip lock files
            if self.should_skip_file(path) {
                continue;
            }
            
            // Get relative path from root
            let relative_path = match path.strip_prefix(root_path) {
                Ok(rel) => rel,
                Err(_) => continue,
            };
            
            if entry.file_type().map_or(false, |ft| ft.is_dir()) {
                self.add_directory_to_tree(&mut root_node, relative_path, path)?;
            } else {
                self.add_file_to_tree(&mut root_node, relative_path, path)?;
            }
        }
        
        // Update all directory statistics
        root_node.update_stats();
        
        Ok(TreeNode::Directory(root_node))
    }
    
    /// Add a directory to the tree
    fn add_directory_to_tree(
        &self,
        root: &mut DirectoryNode,
        relative_path: &Path,
        _full_path: &Path,
    ) -> anyhow::Result<()> {
        let mut current = root;
        
        for component in relative_path.components() {
            let name = component.as_os_str().to_string_lossy().to_string();
            
            // Check if directory exists, and create if not
            let needs_insert = !current.children.contains_key(&name);
            if needs_insert {
                let dir_path = if current.path.as_os_str() == "." {
                    PathBuf::from(&name)
                } else {
                    current.path.join(&name)
                };
                
                let mut dir_node = DirectoryNode::new(name.clone(), dir_path.clone());
                
                // Set git status for this directory if available
                dir_node.git_status = self.git_status.get(&dir_path).cloned();
                
                current.children.insert(name.clone(), TreeNode::Directory(dir_node));
            }
            
            // Move to the child directory
            match current.children.get_mut(&name) {
                Some(TreeNode::Directory(dir)) => {
                    current = dir;
                }
                _ => return Err(anyhow::anyhow!("Expected directory node")),
            }
        }
        
        Ok(())
    }
    
    /// Add a file to the tree
    fn add_file_to_tree(
        &self,
        root: &mut DirectoryNode,
        relative_path: &Path,
        full_path: &Path,
    ) -> anyhow::Result<()> {
        let file_name = relative_path.file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?
            .to_string_lossy()
            .to_string();
        
        // Navigate to the parent directory
        let parent_path = relative_path.parent().unwrap_or(Path::new(""));
        let mut current = root;
        
        for component in parent_path.components() {
            let name = component.as_os_str().to_string_lossy().to_string();
            
            // Check if directory exists, and create if not
            let needs_insert = !current.children.contains_key(&name);
            if needs_insert {
                let dir_path = if current.path.as_os_str() == "." {
                    PathBuf::from(&name)
                } else {
                    current.path.join(&name)
                };
                
                let dir_node = DirectoryNode::new(name.clone(), dir_path);
                current.children.insert(name.clone(), TreeNode::Directory(dir_node));
            }
            
            match current.children.get_mut(&name) {
                Some(TreeNode::Directory(dir)) => {
                    current = dir;
                }
                _ => return Err(anyhow::anyhow!("Expected directory node")),
            }
        }
        
        // Create the file node
        let mut file_node = FileNode::new(file_name.clone(), full_path.to_path_buf());
        
        // Set git status
        file_node.git_status = self.git_status.get(relative_path).cloned();
        
        // Detect language from extension
        file_node.language = self.detect_language(full_path);
        
        // Calculate metrics for source files if analysis is enabled
        if self.options.show_analysis && self.is_source_file(full_path) {
            if let Ok(content) = std::fs::read_to_string(full_path) {
                if let Ok(metrics) = MetricsCalculator::calculate_metrics(full_path, &content) {
                    file_node.metrics = Some(metrics);
                }
            }
        }
        
        // Run compiler diagnostics for this file if diagnostics are enabled
        if self.options.show_diagnostics && self.is_source_file(full_path) {
            file_node.diagnostics = self.run_diagnostics_for_file(full_path);
        }
        
        // Extract AST structure for supported files if syntax analysis is enabled
        if self.options.show_syntax && self.is_source_file(full_path) {
            file_node.ast_structure = crate::diagnostics::extract_ast_structure(full_path);
        }
        
        // Set last modified time
        if let Ok(metadata) = std::fs::metadata(full_path) {
            file_node.last_modified = metadata.modified().ok();
        }
        
        current.children.insert(file_name, TreeNode::File(file_node));
        
        Ok(())
    }
    
    /// Check if a file should be skipped (lock files, etc.)
    fn should_skip_file(&self, path: &Path) -> bool {
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            matches!(file_name,
                "Cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | 
                "composer.lock" | "Gemfile.lock" | "poetry.lock" | "Pipfile.lock"
            )
        } else {
            false
        }
    }
    
    /// Detect programming language from file extension
    fn detect_language(&self, path: &Path) -> Option<String> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                match ext.to_lowercase().as_str() {
                    "rs" => "Rust",
                    "js" => "JavaScript",
                    "jsx" => "JavaScript",
                    "ts" => "TypeScript", 
                    "tsx" => "TypeScript",
                    "py" => "Python",
                    "java" => "Java",
                    "go" => "Go",
                    "c" => "C",
                    "cpp" | "cc" | "cxx" => "C++",
                    "h" | "hpp" => "C/C++ Header",
                    "php" => "PHP",
                    "rb" => "Ruby",
                    "cs" => "C#",
                    "swift" => "Swift",
                    "kt" => "Kotlin",
                    "scala" => "Scala",
                    "clj" | "cljs" => "Clojure",
                    "hs" => "Haskell",
                    "elm" => "Elm",
                    "ex" | "exs" => "Elixir",
                    "erl" => "Erlang",
                    "lua" => "Lua",
                    "r" => "R",
                    "jl" => "Julia",
                    "dart" => "Dart",
                    "sh" | "bash" | "zsh" => "Shell",
                    "yml" | "yaml" => "YAML",
                    "json" => "JSON",
                    "toml" => "TOML",
                    "xml" => "XML",
                    "html" => "HTML",
                    "css" => "CSS",
                    "scss" | "sass" => "SCSS",
                    "md" => "Markdown",
                    _ => "Other",
                }
                .to_string()
            })
    }
    
    /// Check if a file is a source code file that should have metrics calculated
    fn is_source_file(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            matches!(ext.to_lowercase().as_str(),
                "rs" | "js" | "jsx" | "ts" | "tsx" | "py" | "java" | "go" | 
                "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "php" | "rb" | 
                "cs" | "swift" | "kt" | "scala" | "clj" | "cljs" | "hs" | 
                "elm" | "ex" | "exs" | "erl" | "lua" | "r" | "jl" | "dart"
            )
        } else {
            false
        }
    }
    
    /// Run workspace-wide diagnostics once and cache results per file
    fn run_workspace_diagnostics<P: AsRef<Path>>(path: P) -> HashMap<PathBuf, FileDiagnostics> {
        let mut diagnostics_map = HashMap::new();
        
        // Check if we're in a Rust workspace
        if let Some(project_root) = Self::find_rust_project_root(path.as_ref()) {
            if let Some(workspace_diagnostics) = Self::run_rust_workspace_diagnostics(&project_root) {
                diagnostics_map.extend(workspace_diagnostics);
            }
        }
        
        // TODO: Add other language workspace diagnostics here
        // - TypeScript: run tsc --noEmit on workspace
        // - Python: run mypy on workspace 
        // - Go: run go vet ./...
        
        diagnostics_map
    }
    
    /// Find Rust project root by looking for Cargo.toml
    fn find_rust_project_root(start_path: &Path) -> Option<PathBuf> {
        let mut current = start_path;
        
        loop {
            if current.join("Cargo.toml").exists() {
                return Some(current.to_path_buf());
            }
            
            current = current.parent()?;
        }
    }
    
    /// Run Rust diagnostics for entire workspace and return per-file results
    fn run_rust_workspace_diagnostics(project_root: &Path) -> Option<HashMap<PathBuf, FileDiagnostics>> {
        use std::process::Command;
        
        let output = Command::new("cargo")
            .arg("check")
            .arg("--message-format=json")
            .arg("--quiet")
            .current_dir(project_root)
            .output()
            .ok()?;

        Self::parse_rust_workspace_diagnostics(&output.stdout, project_root)
    }
    
    /// Parse Rust cargo check JSON output and organize by file
    fn parse_rust_workspace_diagnostics(output: &[u8], project_root: &Path) -> Option<HashMap<PathBuf, FileDiagnostics>> {
        let output_str = String::from_utf8_lossy(output);
        let mut diagnostics_by_file: HashMap<PathBuf, FileDiagnostics> = HashMap::new();

        for line in output_str.lines() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(message) = json.get("message") {
                    if let Some(spans) = message.get("spans").and_then(|s| s.as_array()) {
                        for span in spans {
                            if let Some((file_path, diagnostic)) = Self::parse_rust_workspace_span(span, message, project_root) {
                                diagnostics_by_file
                                    .entry(file_path)
                                    .or_insert_with(FileDiagnostics::default)
                                    .add_diagnostic(diagnostic);
                            }
                        }
                    }
                }
            }
        }

        if diagnostics_by_file.is_empty() {
            None
        } else {
            Some(diagnostics_by_file)
        }
    }
    
    /// Parse a single Rust span for workspace diagnostics
    fn parse_rust_workspace_span(span: &serde_json::Value, message_obj: &serde_json::Value, project_root: &Path) -> Option<(PathBuf, crate::diagnostics::types::CompilerDiagnostic)> {
        let span_file = span.get("file_name")?.as_str()?;
        
        // Convert relative path to absolute path from project root
        let file_path = if Path::new(span_file).is_absolute() {
            PathBuf::from(span_file)
        } else {
            project_root.join(span_file)
        };

        let line = span.get("line_start")?.as_u64()? as u32;
        let column = span.get("column_start")?.as_u64()? as u32;
        let length = span.get("column_end")
            .and_then(|end| end.as_u64())
            .map(|end| (end as u32).saturating_sub(column));

        // Get the full message from the parent message object
        let full_message = message_obj.get("message")?.as_str()?.to_string();
        
        // Get the span label as additional context
        let span_label = span.get("label").and_then(|l| l.as_str()).unwrap_or("");
        
        // Combine full message with span label if they're different
        let combined_message = if !span_label.is_empty() && !full_message.contains(span_label) {
            format!("{} ({})", full_message, span_label)
        } else {
            full_message
        };
        
        // Parse severity from message level
        let severity = match message_obj.get("level").and_then(|l| l.as_str()) {
            Some("error") => crate::diagnostics::types::DiagnosticSeverity::Error,
            Some("warning") => crate::diagnostics::types::DiagnosticSeverity::Warning,
            Some("note") | Some("info") => crate::diagnostics::types::DiagnosticSeverity::Info,
            Some("help") => crate::diagnostics::types::DiagnosticSeverity::Hint,
            _ => crate::diagnostics::types::DiagnosticSeverity::Warning,
        };
        
        // Extract error code if available
        let code = message_obj.get("code")
            .and_then(|c| c.get("code"))
            .and_then(|c| c.as_str())
            .map(|s| s.to_string());
        
        let diagnostic = crate::diagnostics::types::CompilerDiagnostic {
            severity,
            message: combined_message,
            code,
            location: crate::diagnostics::types::DiagnosticLocation { line, column, length },
            file_path: file_path.clone(),
            suggestions: Vec::new(),
        };
        
        Some((file_path, diagnostic))
    }
    
    /// Run compiler diagnostics for a file
    fn run_diagnostics_for_file(&self, file_path: &Path) -> Option<FileDiagnostics> {
        // First check cached diagnostics
        if let Some(diagnostics) = self.get_diagnostics_for_file(file_path) {
            return Some(diagnostics);
        }
        
        // Run fresh diagnostics using CompilerDiagnosticsRunner
        let language_str = Self::detect_language_from_extension(file_path);
        CompilerDiagnosticsRunner::run_diagnostics(file_path, language_str)
    }

    /// Get diagnostics for a file with robust path matching
    fn get_diagnostics_for_file(&self, file_path: &Path) -> Option<FileDiagnostics> {
        // Try exact path match first
        if let Some(diagnostics) = self.workspace_diagnostics.get(file_path) {
            return Some(diagnostics.clone());
        }
        
        // Try all stored paths to find a match
        for (stored_path, diagnostics) in &self.workspace_diagnostics {
            // Check if paths point to the same file
            if Self::paths_match(file_path, stored_path) {
                return Some(diagnostics.clone());
            }
        }
        
        None
    }
    
    /// Detect language from file extension
    fn detect_language_from_extension(path: &Path) -> Option<&'static str> {
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            match extension.to_lowercase().as_str() {
                "rs" => Some("Rust"),
                "js" | "jsx" => Some("JavaScript"),
                "ts" | "tsx" => Some("TypeScript"),
                "py" => Some("Python"),
                "java" => Some("Java"),
                "go" => Some("Go"),
                _ => None,
            }
        } else {
            None
        }
    }
    
    /// Check if two paths refer to the same file
    fn paths_match(path1: &Path, path2: &Path) -> bool {
        // Try exact match
        if path1 == path2 {
            return true;
        }
        
        // Try canonicalized paths
        if let (Ok(canon1), Ok(canon2)) = (path1.canonicalize(), path2.canonicalize()) {
            if canon1 == canon2 {
                return true;
            }
        }
        
        // Try file name match (last resort)
        if let (Some(name1), Some(name2)) = (path1.file_name(), path2.file_name()) {
            if name1 == name2 {
                // Check if the path endings match (same directory structure)
                let components1: Vec<_> = path1.components().rev().take(3).collect();
                let components2: Vec<_> = path2.components().rev().take(3).collect();
                return components1 == components2;
            }
        }
        
        false
    }
}

/// Display a tree structure with proper formatting
pub struct TreeDisplay;

/// Options for displaying additional information with files
#[derive(Debug, Default, Clone)]
pub struct TreeDisplayOptions {
    pub show_metrics: bool,
    pub show_diffs: bool,
    pub show_analysis: bool,
    pub show_diagnostics: bool,
    pub show_syntax: bool,
    pub truncate_diffs: bool,
    pub output_json: bool,
    pub git_status: std::collections::HashMap<std::path::PathBuf, crate::diagnostics::GitFileStatus>,
}

impl TreeDisplay {
    /// Display a tree node with proper indentation and formatting (legacy method)
    pub fn display_tree(node: &TreeNode, show_metrics: bool) {
        let options = TreeDisplayOptions {
            show_metrics,
            ..Default::default()
        };
        Self::display_tree_with_options(node, &options);
    }
    
    /// Display a tree node with enhanced options for file-centric information
    pub fn display_tree_with_options(node: &TreeNode, options: &TreeDisplayOptions) {
        if options.output_json {
            Self::output_json(node, options);
        } else {
            Self::display_node_with_options(node, "", true, options);
        }
    }
    
    /// Output tree data as JSON with comprehensive analysis data
    pub fn output_json(node: &TreeNode, options: &TreeDisplayOptions) {
        let enhanced_json = Self::create_enhanced_json(node, options);
        match serde_json::to_string_pretty(&enhanced_json) {
            Ok(json) => println!("{}", json),
            Err(e) => eprintln!("Error serializing enhanced tree to JSON: {}", e),
        }
    }
    
    /// Create enhanced JSON structure that includes all analysis data
    pub fn create_enhanced_json(node: &TreeNode, options: &TreeDisplayOptions) -> serde_json::Value {
        match node {
            TreeNode::Directory(dir) => {
                let mut dir_obj = serde_json::Map::new();
                dir_obj.insert("type".to_string(), serde_json::Value::String("directory".to_string()));
                dir_obj.insert("name".to_string(), serde_json::Value::String(dir.name.clone()));
                dir_obj.insert("path".to_string(), serde_json::Value::String(dir.path.to_string_lossy().to_string()));
                
                // Add absolute path
                if let Ok(absolute_path) = dir.path.canonicalize() {
                    dir_obj.insert("absolute_path".to_string(), serde_json::Value::String(
                        absolute_path.to_string_lossy().to_string()
                    ));
                } else if let Ok(current_dir) = std::env::current_dir() {
                    // Fallback: join with current directory if canonicalize fails
                    let absolute_fallback = current_dir.join(&dir.path);
                    dir_obj.insert("absolute_path".to_string(), serde_json::Value::String(
                        absolute_fallback.to_string_lossy().to_string()
                    ));
                }
                
                // Add git status if available
                if let Some(status) = &dir.git_status {
                    dir_obj.insert("git_status".to_string(), serde_json::Value::String(Self::git_status_to_string(status)));
                }
                
                // Add directory statistics if metrics are enabled
                if options.show_metrics {
                    let mut stats = serde_json::Map::new();
                    stats.insert("total_files".to_string(), serde_json::Value::Number(dir.stats.total_files.into()));
                    stats.insert("total_directories".to_string(), serde_json::Value::Number(dir.stats.total_directories.into()));
                    stats.insert("total_loc".to_string(), serde_json::Value::Number(dir.stats.total_loc.into()));
                    stats.insert("total_comments".to_string(), serde_json::Value::Number(dir.stats.total_comments.into()));
                    stats.insert("total_functions".to_string(), serde_json::Value::Number(dir.stats.total_functions.into()));
                    stats.insert("total_complexity".to_string(), serde_json::Value::Number(dir.stats.total_complexity.into()));
                    
                    // Add language breakdown
                    let languages: serde_json::Map<String, serde_json::Value> = dir.stats.languages.iter()
                        .map(|(lang, count)| (lang.clone(), serde_json::Value::Number((*count).into())))
                        .collect();
                    stats.insert("languages".to_string(), serde_json::Value::Object(languages));
                    
                    dir_obj.insert("statistics".to_string(), serde_json::Value::Object(stats));
                }
                
                // Process children
                let children: Vec<serde_json::Value> = dir.children.values()
                    .map(|child| Self::create_enhanced_json(child, options))
                    .collect();
                dir_obj.insert("children".to_string(), serde_json::Value::Array(children));
                
                serde_json::Value::Object(dir_obj)
            }
            TreeNode::File(file) => {
                let mut file_obj = serde_json::Map::new();
                file_obj.insert("type".to_string(), serde_json::Value::String("file".to_string()));
                file_obj.insert("name".to_string(), serde_json::Value::String(file.name.clone()));
                file_obj.insert("path".to_string(), serde_json::Value::String(file.path.to_string_lossy().to_string()));
                
                // Add absolute path
                if let Ok(absolute_path) = file.path.canonicalize() {
                    file_obj.insert("absolute_path".to_string(), serde_json::Value::String(
                        absolute_path.to_string_lossy().to_string()
                    ));
                } else if let Ok(current_dir) = std::env::current_dir() {
                    // Fallback: join with current directory if canonicalize fails
                    let absolute_fallback = current_dir.join(&file.path);
                    file_obj.insert("absolute_path".to_string(), serde_json::Value::String(
                        absolute_fallback.to_string_lossy().to_string()
                    ));
                }
                
                // Add language if available
                if let Some(language) = &file.language {
                    file_obj.insert("language".to_string(), serde_json::Value::String(language.clone()));
                }
                
                // Add git status if available
                if let Some(status) = &file.git_status {
                    file_obj.insert("git_status".to_string(), serde_json::Value::String(Self::git_status_to_string(status)));
                }
                
                // Add last modified time if available
                if let Some(modified) = &file.last_modified {
                    if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        file_obj.insert("last_modified".to_string(), serde_json::Value::Number(duration.as_secs().into()));
                    }
                }
                
                // Add metrics if available and enabled
                if options.show_metrics || options.show_analysis {
                    if let Some(metrics) = &file.metrics {
                        let mut metrics_obj = serde_json::Map::new();
                        metrics_obj.insert("lines_of_code".to_string(), serde_json::Value::Number(metrics.lines_of_code.into()));
                        metrics_obj.insert("comment_lines".to_string(), serde_json::Value::Number(metrics.comment_lines.into()));
                        metrics_obj.insert("blank_lines".to_string(), serde_json::Value::Number(metrics.blank_lines.into()));
                        metrics_obj.insert("function_count".to_string(), serde_json::Value::Number(metrics.function_count.into()));
                        metrics_obj.insert("cyclomatic_complexity".to_string(), serde_json::Value::Number(metrics.cyclomatic_complexity.into()));
                        file_obj.insert("metrics".to_string(), serde_json::Value::Object(metrics_obj));
                    }
                }
                
                // Add diff information if enabled and file has changes
                if options.show_diffs {
                    // Enhanced path matching for git status lookup
                    let git_status = options.git_status.get(&file.path)
                        .or_else(|| {
                            // Try looking up by relative path
                            if let Ok(current_dir) = std::env::current_dir() {
                                if let Ok(relative) = file.path.strip_prefix(&current_dir) {
                                    return options.git_status.get(relative);
                                }
                            }
                            None
                        })
                        .or_else(|| {
                            // Try stripping ./ prefix if present
                            if let Some(stripped) = file.path.to_string_lossy().strip_prefix("./") {
                                let path_without_prefix = std::path::Path::new(stripped);
                                return options.git_status.get(path_without_prefix);
                            }
                            None
                        });

                    if let Some(status) = git_status {
                        if matches!(status, crate::diagnostics::GitFileStatus::Modified | crate::diagnostics::GitFileStatus::Staged) {
                            // Get diff content
                            if let Ok(output) = std::process::Command::new("git")
                                .args(&["diff", "HEAD", "--"])
                                .arg(&file.path)
                                .output()
                            {
                                if !output.stdout.is_empty() {
                                    let diff_content = String::from_utf8_lossy(&output.stdout);
                                    let diff_lines: Vec<serde_json::Value> = diff_content.lines()
                                        .map(|line| serde_json::Value::String(line.to_string()))
                                        .collect();
                                    file_obj.insert("diff".to_string(), serde_json::Value::Array(diff_lines));
                                }
                            }
                        }
                    }
                }
                
                // Add diagnostics if available and enabled
                if options.show_diagnostics {
                    if let Some(diagnostics) = &file.diagnostics {
                        let mut diagnostics_obj = serde_json::Map::new();
                        
                        // Add counts
                        diagnostics_obj.insert("error_count".to_string(), serde_json::Value::Number(diagnostics.errors.len().into()));
                        diagnostics_obj.insert("warning_count".to_string(), serde_json::Value::Number(diagnostics.warnings.len().into()));
                        diagnostics_obj.insert("info_count".to_string(), serde_json::Value::Number(diagnostics.infos.len().into()));
                        diagnostics_obj.insert("hint_count".to_string(), serde_json::Value::Number(diagnostics.hints.len().into()));
                        diagnostics_obj.insert("total_count".to_string(), serde_json::Value::Number(diagnostics.total_count().into()));
                        
                        // Add error details
                        let errors: Vec<serde_json::Value> = diagnostics.errors.iter()
                            .map(|error| Self::diagnostic_to_json(error))
                            .collect();
                        diagnostics_obj.insert("errors".to_string(), serde_json::Value::Array(errors));
                        
                        // Add warning details
                        let warnings: Vec<serde_json::Value> = diagnostics.warnings.iter()
                            .map(|warning| Self::diagnostic_to_json(warning))
                            .collect();
                        diagnostics_obj.insert("warnings".to_string(), serde_json::Value::Array(warnings));
                        
                        // Add info details
                        let infos: Vec<serde_json::Value> = diagnostics.infos.iter()
                            .map(|info| Self::diagnostic_to_json(info))
                            .collect();
                        diagnostics_obj.insert("infos".to_string(), serde_json::Value::Array(infos));
                        
                        // Add hint details
                        let hints: Vec<serde_json::Value> = diagnostics.hints.iter()
                            .map(|hint| Self::diagnostic_to_json(hint))
                            .collect();
                        diagnostics_obj.insert("hints".to_string(), serde_json::Value::Array(hints));
                        
                        file_obj.insert("diagnostics".to_string(), serde_json::Value::Object(diagnostics_obj));
                    }
                }
                
                // Add AST structure if available and syntax analysis is enabled
                if options.show_syntax {
                    if let Some(ast_structure) = &file.ast_structure {
                        if let Ok(ast_json) = serde_json::to_value(ast_structure) {
                            file_obj.insert("ast_structure".to_string(), ast_json);
                        }
                    }
                }
                
                serde_json::Value::Object(file_obj)
            }
        }
    }
    
    /// Convert a diagnostic to JSON format
    fn diagnostic_to_json(diagnostic: &crate::diagnostics::types::CompilerDiagnostic) -> serde_json::Value {
        let mut diag_obj = serde_json::Map::new();
        
        diag_obj.insert("severity".to_string(), serde_json::Value::String(
            match diagnostic.severity {
                crate::diagnostics::types::DiagnosticSeverity::Error => "error",
                crate::diagnostics::types::DiagnosticSeverity::Warning => "warning", 
                crate::diagnostics::types::DiagnosticSeverity::Info => "info",
                crate::diagnostics::types::DiagnosticSeverity::Hint => "hint",
            }.to_string()
        ));
        
        diag_obj.insert("message".to_string(), serde_json::Value::String(diagnostic.message.clone()));
        
        if let Some(code) = &diagnostic.code {
            diag_obj.insert("code".to_string(), serde_json::Value::String(code.clone()));
        }
        
        // Add location information
        let mut location_obj = serde_json::Map::new();
        location_obj.insert("line".to_string(), serde_json::Value::Number(diagnostic.location.line.into()));
        location_obj.insert("column".to_string(), serde_json::Value::Number(diagnostic.location.column.into()));
        if let Some(length) = diagnostic.location.length {
            location_obj.insert("length".to_string(), serde_json::Value::Number(length.into()));
        }
        diag_obj.insert("location".to_string(), serde_json::Value::Object(location_obj));
        
        serde_json::Value::Object(diag_obj)
    }
    
    /// Convert git status to string for JSON
    fn git_status_to_string(status: &crate::diagnostics::GitFileStatus) -> String {
        match status {
            crate::diagnostics::GitFileStatus::Modified => "modified".to_string(),
            crate::diagnostics::GitFileStatus::Staged => "staged".to_string(),
            crate::diagnostics::GitFileStatus::Untracked => "untracked".to_string(),
            crate::diagnostics::GitFileStatus::Conflicted => "conflicted".to_string(),
        }
    }
    
    /// Recursively display a tree node
    fn display_node(node: &TreeNode, prefix: &str, is_last: bool, show_metrics: bool) {
        let options = TreeDisplayOptions {
            show_metrics,
            ..Default::default()
        };
        Self::display_node_with_options(node, prefix, is_last, &options);
    }
    
    /// Recursively display a tree node with enhanced options
    fn display_node_with_options(node: &TreeNode, prefix: &str, is_last: bool, options: &TreeDisplayOptions) {
        let connector = if is_last { "└── " } else { "├── " };
        let icon = Self::get_icon(node);
        let name = node.name();
        
        match node {
            TreeNode::Directory(dir) => {
                let stats_info = if options.show_metrics {
                    format!(" ({} files, {} LOC)", dir.stats.total_files, dir.stats.total_loc)
                } else {
                    String::new()
                };
                
                let git_icon = Self::get_git_icon(&dir.git_status);
                println!("{}{}{}{}{}{}", prefix, connector, git_icon, icon, name, stats_info);
                
                // Display children
                let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
                let children: Vec<_> = dir.children.values().collect();
                
                for (i, child) in children.iter().enumerate() {
                    let is_last_child = i == children.len() - 1;
                    Self::display_node_with_options(child, &new_prefix, is_last_child, options);
                }
            }
            TreeNode::File(file) => {
                Self::display_file_with_info(file, prefix, connector, icon, name, options);
            }
        }
    }
    
    /// Display a file with all its associated information (metrics, diffs, etc.)
    fn display_file_with_info(
        file: &crate::diagnostics::types::FileNode, 
        prefix: &str, 
        connector: &str, 
        icon: &str, 
        name: &str, 
        options: &TreeDisplayOptions
    ) {
        // Basic file line with metrics and language info
        let metrics_info = if options.show_metrics {
            if let Some(metrics) = &file.metrics {
                format!(" ({} LOC, {} funcs, {}cc)", 
                    metrics.lines_of_code, 
                    metrics.function_count,
                    metrics.cyclomatic_complexity
                )
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        
        let language_info = if let Some(lang) = &file.language {
            format!(" [{}]", lang)
        } else {
            String::new()
        };
        
        let git_icon = Self::get_git_icon(&file.git_status);
        println!("{}{}{}{}{}{}{}", 
            prefix, connector, git_icon, icon, name, language_info, metrics_info);
        
        // Show additional file-centric information with proper indentation
        let file_prefix = format!("{}    ", prefix);
        
        // Show diff information if requested and file has changes
        if options.show_diffs {
            let file_path = &file.path;
            
            // Try to get status from file or from options map
            let status = file.git_status.as_ref()
                .or_else(|| options.git_status.get(file_path))
                .or_else(|| {
                    // Try looking up by relative path from current directory
                    if let Ok(current_dir) = std::env::current_dir() {
                        if let Ok(relative) = file_path.strip_prefix(&current_dir) {
                            return options.git_status.get(relative);
                        }
                    }
                    None
                });
            
            if let Some(status) = status {
                match status {
                    crate::diagnostics::GitFileStatus::Modified => {
                        println!("{}├─ Modified:", file_prefix);
                        Self::display_file_diff_with_options(file_path, &format!("{}│  ", file_prefix), options);
                    }
                    crate::diagnostics::GitFileStatus::Staged => {
                        println!("{}├─ Staged:", file_prefix);
                        Self::display_file_diff_with_options(file_path, &format!("{}│  ", file_prefix), options);
                    }
                    crate::diagnostics::GitFileStatus::Untracked => {
                        println!("{}├─ Untracked:", file_prefix);
                        Self::display_file_diff_with_options(file_path, &format!("{}│  ", file_prefix), options);
                    }
                    crate::diagnostics::GitFileStatus::Conflicted => {
                        println!("{}├─ Conflicted:", file_prefix);
                    }
                }
            }
        }
        
        // Show analysis information if requested
        if options.show_analysis && file.metrics.is_some() {
            if let Some(metrics) = &file.metrics {
                println!("{}├─ Analysis:", file_prefix);
                println!("{}│  • Lines of code: {}", file_prefix, metrics.lines_of_code);
                println!("{}│  • Comment lines: {}", file_prefix, metrics.comment_lines);
                println!("{}│  • Functions: {}", file_prefix, metrics.function_count);
                println!("{}│  • Complexity: {}", file_prefix, metrics.cyclomatic_complexity);
            }
        }
        
        // Show compiler diagnostics if requested
        if options.show_diagnostics && file.diagnostics.is_some() {
            if let Some(diagnostics) = &file.diagnostics {
                let has_other_sections = options.show_analysis && file.metrics.is_some();
                let connector = if has_other_sections { "├─" } else { "└─" };
                
                if diagnostics.total_count() > 0 {
                    println!("{}{} Diagnostics ({} issues):", file_prefix, connector, diagnostics.total_count());
                    
                    // Show errors
                    for error in &diagnostics.errors {
                        println!("{}│  E Line {}: {}", file_prefix, error.location.line, error.message);
                        if let Some(code) = &error.code {
                            println!("{}│     Code: {}", file_prefix, code);
                        }
                    }
                    
                    // Show warnings
                    for warning in &diagnostics.warnings {
                        println!("{}│  W Line {}: {}", file_prefix, warning.location.line, warning.message);
                        if let Some(code) = &warning.code {
                            println!("{}│     Code: {}", file_prefix, code);
                        }
                    }
                    
                    // Show info messages
                    for info in &diagnostics.infos {
                        println!("{}│  ℹ️  Line {}: {}", file_prefix, info.location.line, info.message);
                        if let Some(code) = &info.code {
                            println!("{}│     Code: {}", file_prefix, code);
                        }
                    }
                    
                    // Show hints
                    for hint in &diagnostics.hints {
                        println!("{}│  H Line {}: {}", file_prefix, hint.location.line, hint.message);
                        if let Some(code) = &hint.code {
                            println!("{}│     Code: {}", file_prefix, code);
                        }
                    }
                } else {
                    println!("{}{} No diagnostics issues", file_prefix, connector);
                }
            }
        }
        
        // Show AST structure if requested and available
        if options.show_syntax && file.ast_structure.is_some() {
            if let Some(ast_structure) = &file.ast_structure {
                let has_other_sections = (options.show_analysis && file.metrics.is_some()) || 
                                        (options.show_diagnostics && file.diagnostics.is_some());
                let connector = if has_other_sections { "├─" } else { "└─" };
                
                println!("{}{} AST Structure:", file_prefix, connector);
                Self::display_ast_structure(ast_structure, &format!("{}│  ", file_prefix));
            }
        }
    }
    
    /// Display AST structure in a readable tree format
    fn display_ast_structure(ast: &crate::diagnostics::types::AstStructure, prefix: &str) {
        println!("{}Language: {}", prefix, ast.language);
        
        if !ast.root_nodes.is_empty() {
            println!("{}Root nodes: {}", prefix, ast.root_nodes.len());
            for (i, root) in ast.root_nodes.iter().enumerate().take(3) {
                println!("{}  {}. {} ({}..{})", prefix, i + 1, root.node_type, root.range.start, root.range.end);
            }
            if ast.root_nodes.len() > 3 {
                println!("{}  ... and {} more", prefix, ast.root_nodes.len() - 3);
            }
        }
        
        if !ast.symbols.functions.is_empty() {
            println!("{}Functions:", prefix);
            for func in &ast.symbols.functions {
                println!("{}  • {} (line {})", prefix, func.name, func.line);
            }
        }
        
        if !ast.symbols.classes.is_empty() {
            println!("{}Classes/Structs:", prefix);
            for class in &ast.symbols.classes {
                println!("{}  • {} (line {})", prefix, class.name, class.line);
            }
        }
        
        if !ast.symbols.types.is_empty() {
            println!("{}Types:", prefix);
            for type_def in &ast.symbols.types {
                println!("{}  • {} (line {})", prefix, type_def.name, type_def.line);
            }
        }
        
        if !ast.symbols.modules.is_empty() {
            println!("{}Modules:", prefix);
            for module in &ast.symbols.modules {
                println!("{}  • {} (line {})", prefix, module.name, module.line);
            }
        }
        
        if !ast.syntax_tokens.is_empty() {
            println!("{}Syntax tokens: {} total", prefix, ast.syntax_tokens.len());
        }
    }
    
    /// Display diff information for a file with original formatting and optional truncation
    fn display_file_diff_with_options(file_path: &std::path::Path, prefix: &str, options: &TreeDisplayOptions) {
        // Try regular git diff for tracked files first
        if let Ok(output) = std::process::Command::new("git")
            .args(&["diff", "HEAD", "--"])
            .arg(file_path)
            .output()
        {
            if !output.stdout.is_empty() {
                let diff_content = String::from_utf8_lossy(&output.stdout);
                Self::print_diff_content(&diff_content, prefix, options.truncate_diffs);
                return;
            }
        }
        
        // Fall back to diff against /dev/null for untracked files
        if let Ok(output) = std::process::Command::new("git")
            .args(&["diff", "--no-index", "/dev/null"])
            .arg(file_path)
            .output()
        {
            if !output.stdout.is_empty() {
                let diff_content = String::from_utf8_lossy(&output.stdout);
                Self::print_diff_content(&diff_content, prefix, options.truncate_diffs);
            }
        }
    }
    
    /// Print diff content with syntax highlighting and optional truncation
    fn print_diff_content(diff_content: &str, prefix: &str, truncate: bool) {
        let lines: Vec<&str> = diff_content.lines().collect();
        
        let lines_to_show = if truncate && lines.len() > 15 {
            &lines[..15]
        } else {
            &lines
        };
        
        // Print lines with syntax highlighting
        for line in lines_to_show {
            let highlighted_line = Self::highlight_diff_line(line);
            println!("{}{}", prefix, highlighted_line);
        }
        
        // Show truncation message if needed
        if truncate && lines.len() > 15 {
            println!("{}... (truncated, showing first 15 lines of {} total)", prefix, lines.len());
        }
    }
    
    /// Apply syntax highlighting to a diff line based on its prefix
    fn highlight_diff_line(line: &str) -> String {
        if line.is_empty() {
            return line.to_string();
        }
        
        let first_char = line.chars().next().unwrap();
        match first_char {
            '+' => {
                // Green for additions
                format!("\x1b[32m{}\x1b[0m", line)
            }
            '-' => {
                // Red for deletions
                format!("\x1b[31m{}\x1b[0m", line)
            }
            '@' => {
                // Cyan for hunk headers
                format!("\x1b[36m{}\x1b[0m", line)
            }
            '\\' => {
                // Yellow for "No newline at end of file" messages
                format!("\x1b[33m{}\x1b[0m", line)
            }
            _ => {
                // Default color for context lines and other content
                line.to_string()
            }
        }
    }
    
    /// Get icon for different node types
    fn get_icon(node: &TreeNode) -> &'static str {
        match node {
            TreeNode::Directory(_) => "",
            TreeNode::File(file) => {
                if let Some(language) = &file.language {
                    match language.as_str() {
                        "Rust" => "R ",
                        "JavaScript" | "TypeScript" => "J ",
                        "Python" => "P ",
                        "Java" => "J ",
                        "Go" => "G ",
                        "C" | "C++" => "C ",
                        "JSON" | "YAML" | "TOML" => "",
                        "Markdown" => "M ",
                        "HTML" => "H ",
                        "CSS" | "SCSS" => "C ",
                        _ => "",
                    }
                } else {
                    ""
                }
            }
        }
    }
    
    /// Get git status icon
    fn get_git_icon(status: &Option<GitFileStatus>) -> &'static str {
        match status {
            Some(GitFileStatus::Modified) => "M",
            Some(GitFileStatus::Staged) => "S",
            Some(GitFileStatus::Untracked) => "?",
            Some(GitFileStatus::Conflicted) => "!",
            None => "",
        }
    }
    
    /// Display directory statistics summary
    pub fn display_summary(node: &TreeNode) {
        if let TreeNode::Directory(dir) = node {
            println!();
            println!("Directory Summary:");
            println!("  Total files: {}", dir.stats.total_files);
            println!("  Total directories: {}", dir.stats.total_directories);
            println!("  Total lines of code: {}", dir.stats.total_loc);
            println!("  Total comment lines: {}", dir.stats.total_comments);
            println!("  Total functions: {}", dir.stats.total_functions);
            println!("  Average complexity: {:.1}", 
                if dir.stats.total_functions > 0 { 
                    dir.stats.total_complexity as f64 / dir.stats.total_functions as f64 
                } else { 
                    0.0 
                }
            );
            
            if !dir.stats.languages.is_empty() {
                println!();
                println!("Languages:");
                let mut lang_vec: Vec<_> = dir.stats.languages.iter().collect();
                lang_vec.sort_by(|a, b| b.1.cmp(a.1)); // Sort by count descending
                
                for (language, count) in lang_vec {
                    let percentage = (*count as f64 / dir.stats.total_files as f64) * 100.0;
                    println!("  {}: {} files ({:.1}%)", language, count, percentage);
                }
            }
        }
    }
}