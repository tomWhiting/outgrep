use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::diagnostics::types::{TreeNode, DirectoryNode, FileNode, GitFileStatus, CodeMetrics};
use crate::diagnostics::{MetricsCalculator, GitAnalyzer};

/// Builder for constructing directory trees with metrics and git information
pub struct TreeBuilder {
    git_analyzer: GitAnalyzer,
    git_status: HashMap<PathBuf, GitFileStatus>,
}

impl TreeBuilder {
    /// Create a new tree builder for the given directory
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let git_analyzer = GitAnalyzer::new(path);
        let git_status = git_analyzer.get_status_for_cwd().unwrap_or_default();
        
        Self {
            git_analyzer,
            git_status,
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
        
        // Calculate metrics for source files
        if self.is_source_file(full_path) {
            if let Ok(content) = std::fs::read_to_string(full_path) {
                if let Ok(metrics) = MetricsCalculator::calculate_metrics(full_path, &content) {
                    file_node.metrics = Some(metrics);
                }
            }
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
}

/// Display a tree structure with proper formatting
pub struct TreeDisplay;

/// Options for displaying additional information with files
#[derive(Debug, Default)]
pub struct TreeDisplayOptions {
    pub show_metrics: bool,
    pub show_diffs: bool,
    pub show_analysis: bool,
    pub truncate_diffs: bool,
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
        Self::display_node_with_options(node, "", true, options);
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
                println!("{}└─ 📊 Analysis:", file_prefix);
                println!("{}   • Lines of code: {}", file_prefix, metrics.lines_of_code);
                println!("{}   • Comment lines: {}", file_prefix, metrics.comment_lines);
                println!("{}   • Functions: {}", file_prefix, metrics.function_count);
                println!("{}   • Complexity: {}", file_prefix, metrics.cyclomatic_complexity);
            }
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
            TreeNode::Directory(_) => "📁 ",
            TreeNode::File(file) => {
                if let Some(language) = &file.language {
                    match language.as_str() {
                        "Rust" => "🦀 ",
                        "JavaScript" | "TypeScript" => "📜 ",
                        "Python" => "🐍 ",
                        "Java" => "☕ ",
                        "Go" => "🐹 ",
                        "C" | "C++" => "⚙️ ",
                        "JSON" | "YAML" | "TOML" => "📋 ",
                        "Markdown" => "📝 ",
                        "HTML" => "🌐 ",
                        "CSS" | "SCSS" => "🎨 ",
                        _ => "📄 ",
                    }
                } else {
                    "📄 "
                }
            }
        }
    }
    
    /// Get git status icon
    fn get_git_icon(status: &Option<GitFileStatus>) -> &'static str {
        match status {
            Some(GitFileStatus::Modified) => "📝",
            Some(GitFileStatus::Staged) => "📁",
            Some(GitFileStatus::Untracked) => "❓",
            Some(GitFileStatus::Conflicted) => "⚠️",
            None => "",
        }
    }
    
    /// Display directory statistics summary
    pub fn display_summary(node: &TreeNode) {
        if let TreeNode::Directory(dir) = node {
            println!();
            println!("📊 Directory Summary:");
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
                println!("📚 Languages:");
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