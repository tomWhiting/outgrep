/// Compiler diagnostics integration for various languages
use std::path::Path;
use std::process::Command;
use crate::diagnostics::types::{CompilerDiagnostic, DiagnosticSeverity, DiagnosticLocation, FileDiagnostics};

pub struct CompilerDiagnosticsRunner;

impl CompilerDiagnosticsRunner {
    /// Run compiler diagnostics for a file based on its language
    pub fn run_diagnostics(file_path: &Path, language: Option<&str>) -> Option<FileDiagnostics> {
        match language {
            Some("Rust") => Self::run_rust_diagnostics(file_path),
            Some("TypeScript") | Some("JavaScript") => Self::run_typescript_diagnostics(file_path),
            Some("Python") => Self::run_python_diagnostics(file_path),
            Some("Go") => Self::run_go_diagnostics(file_path),
            Some("Java") => Self::run_java_diagnostics(file_path),
            _ => None,
        }
    }

    /// Run Rust compiler diagnostics using cargo check
    fn run_rust_diagnostics(file_path: &Path) -> Option<FileDiagnostics> {
        // Check if we're in a Rust project (has Cargo.toml)
        let project_root = Self::find_project_root(file_path, "Cargo.toml")?;
        
        let output = Command::new("cargo")
            .arg("check")
            .arg("--message-format=json")
            .arg("--quiet")
            .current_dir(project_root)
            .output()
            .ok()?;

        Self::parse_rust_diagnostics(&output.stdout, file_path)
    }

    /// Run TypeScript/JavaScript diagnostics using tsc or eslint
    fn run_typescript_diagnostics(file_path: &Path) -> Option<FileDiagnostics> {
        // First try TypeScript compiler
        if let Some(diagnostics) = Self::run_tsc_diagnostics(file_path) {
            return Some(diagnostics);
        }
        
        // Fall back to ESLint if available
        Self::run_eslint_diagnostics(file_path)
    }

    /// Run TSC diagnostics
    fn run_tsc_diagnostics(file_path: &Path) -> Option<FileDiagnostics> {
        let output = Command::new("npx")
            .arg("tsc")
            .arg("--noEmit")
            .arg("--pretty")
            .arg("false")
            .arg(file_path)
            .output()
            .ok()?;

        Self::parse_tsc_diagnostics(&output.stdout, file_path)
    }

    /// Run ESLint diagnostics
    fn run_eslint_diagnostics(file_path: &Path) -> Option<FileDiagnostics> {
        let output = Command::new("npx")
            .arg("eslint")
            .arg("--format=json")
            .arg(file_path)
            .output()
            .ok()?;

        Self::parse_eslint_diagnostics(&output.stdout, file_path)
    }

    /// Run Python diagnostics using mypy or flake8
    fn run_python_diagnostics(file_path: &Path) -> Option<FileDiagnostics> {
        // Try mypy first for type checking
        if let Some(diagnostics) = Self::run_mypy_diagnostics(file_path) {
            return Some(diagnostics);
        }
        
        // Fall back to flake8 for style checks
        Self::run_flake8_diagnostics(file_path)
    }

    /// Run mypy diagnostics
    fn run_mypy_diagnostics(file_path: &Path) -> Option<FileDiagnostics> {
        let output = Command::new("mypy")
            .arg("--show-error-codes")
            .arg("--no-color-output")
            .arg(file_path)
            .output()
            .ok()?;

        Self::parse_mypy_diagnostics(&output.stdout, file_path)
    }

    /// Run flake8 diagnostics
    fn run_flake8_diagnostics(file_path: &Path) -> Option<FileDiagnostics> {
        let output = Command::new("flake8")
            .arg("--format=%(path)s:%(row)d:%(col)d: %(code)s %(text)s")
            .arg(file_path)
            .output()
            .ok()?;

        Self::parse_flake8_diagnostics(&output.stdout, file_path)
    }

    /// Run Go diagnostics using go vet
    fn run_go_diagnostics(file_path: &Path) -> Option<FileDiagnostics> {
        let output = Command::new("go")
            .arg("vet")
            .arg(file_path)
            .output()
            .ok()?;

        Self::parse_go_diagnostics(&output.stderr, file_path)
    }

    /// Run Java diagnostics using javac
    fn run_java_diagnostics(file_path: &Path) -> Option<FileDiagnostics> {
        let output = Command::new("javac")
            .arg("-Xlint:all")
            .arg("-d")
            .arg("/tmp") // Compile to temp directory
            .arg(file_path)
            .output()
            .ok()?;

        Self::parse_java_diagnostics(&output.stderr, file_path)
    }

    /// Find project root by looking for a specific file (e.g., Cargo.toml, package.json)
    fn find_project_root<'a>(start_path: &'a Path, marker_file: &str) -> Option<&'a Path> {
        let mut current = start_path;
        
        loop {
            if current.join(marker_file).exists() {
                return Some(current);
            }
            
            current = current.parent()?;
        }
    }

    /// Parse Rust cargo check JSON output
    fn parse_rust_diagnostics(output: &[u8], file_path: &Path) -> Option<FileDiagnostics> {
        let output_str = String::from_utf8_lossy(output);
        let mut diagnostics = FileDiagnostics::default();

        for line in output_str.lines() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(message) = json.get("message") {
                    if let Some(spans) = message.get("spans").and_then(|s| s.as_array()) {
                        for span in spans {
                            if let Some(diagnostic) = Self::parse_rust_span(span, file_path) {
                                diagnostics.add_diagnostic(diagnostic);
                            }
                        }
                    }
                }
            }
        }

        if diagnostics.total_count() > 0 {
            Some(diagnostics)
        } else {
            None
        }
    }

    /// Parse a single Rust span into a diagnostic
    fn parse_rust_span(span: &serde_json::Value, file_path: &Path) -> Option<CompilerDiagnostic> {
        let span_file = span.get("file_name")?.as_str()?;
        
        // Only return diagnostics for the specific file we're checking
        // cargo check returns relative paths from project root, but we might get absolute paths
        let span_path = Path::new(span_file);
        let matches = if span_path.is_absolute() {
            span_path == file_path
        } else {
            // Convert absolute file_path to relative from project root for comparison
            if let Ok(current_dir) = std::env::current_dir() {
                if let Ok(relative_file_path) = file_path.strip_prefix(&current_dir) {
                    span_path == relative_file_path
                } else {
                    false
                }
            } else {
                // Fallback: check if the file path ends with the span file path
                file_path.ends_with(span_path)
            }
        };
        
        // Additional check: handle "./" prefix in file paths
        let matches = matches || {
            if let Some(stripped) = file_path.to_string_lossy().strip_prefix("./") {
                Path::new(stripped) == span_path
            } else {
                false
            }
        };
        
        
        if !matches {
            return None;
        }

        let line = span.get("line_start")?.as_u64()? as u32;
        let column = span.get("column_start")?.as_u64()? as u32;
        let length = span.get("column_end")
            .and_then(|end| end.as_u64())
            .map(|end| (end as u32).saturating_sub(column));

        let message = span.get("label")?.as_str()?.to_string();
        
        Some(CompilerDiagnostic {
            severity: DiagnosticSeverity::Error, // Rust cargo check mostly reports errors
            message,
            code: None,
            location: DiagnosticLocation { line, column, length },
            file_path: file_path.to_path_buf(),
            suggestions: Vec::new(),
        })
    }

    /// Parse TypeScript compiler diagnostics
    fn parse_tsc_diagnostics(output: &[u8], file_path: &Path) -> Option<FileDiagnostics> {
        let output_str = String::from_utf8_lossy(output);
        let mut diagnostics = FileDiagnostics::default();

        // Parse TSC output format: filename(line,col): error TS#### message
        for line in output_str.lines() {
            if let Some(diagnostic) = Self::parse_tsc_line(line, file_path) {
                diagnostics.add_diagnostic(diagnostic);
            }
        }

        if diagnostics.total_count() > 0 {
            Some(diagnostics)
        } else {
            None
        }
    }

    /// Parse a single TSC output line
    fn parse_tsc_line(line: &str, file_path: &Path) -> Option<CompilerDiagnostic> {
        // Format: filename(line,col): error TS#### message
        let parts: Vec<&str> = line.split(": ").collect();
        if parts.len() < 2 {
            return None;
        }

        let location_part = parts[0];
        let message_part = parts[1..].join(": ");

        // Extract line and column from parentheses
        let paren_start = location_part.rfind('(')?;
        let paren_end = location_part.rfind(')')?;
        let coords = &location_part[paren_start + 1..paren_end];
        let coord_parts: Vec<&str> = coords.split(',').collect();
        
        if coord_parts.len() != 2 {
            return None;
        }

        let line_num: u32 = coord_parts[0].parse().ok()?;
        let col_num: u32 = coord_parts[1].parse().ok()?;

        // Extract error code if present
        let code = if message_part.contains("TS") {
            message_part.split_whitespace().find(|s| s.starts_with("TS")).map(|s| s.to_string())
        } else {
            None
        };

        Some(CompilerDiagnostic {
            severity: DiagnosticSeverity::Error,
            message: message_part.to_string(),
            code,
            location: DiagnosticLocation { line: line_num, column: col_num, length: None },
            file_path: file_path.to_path_buf(),
            suggestions: Vec::new(),
        })
    }

    /// Parse ESLint JSON diagnostics
    fn parse_eslint_diagnostics(output: &[u8], file_path: &Path) -> Option<FileDiagnostics> {
        let output_str = String::from_utf8_lossy(output);
        let mut diagnostics = FileDiagnostics::default();

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&output_str) {
            if let Some(results) = json.as_array() {
                for result in results {
                    if let Some(messages) = result.get("messages").and_then(|m| m.as_array()) {
                        for message in messages {
                            if let Some(diagnostic) = Self::parse_eslint_message(message, file_path) {
                                diagnostics.add_diagnostic(diagnostic);
                            }
                        }
                    }
                }
            }
        }

        if diagnostics.total_count() > 0 {
            Some(diagnostics)
        } else {
            None
        }
    }

    /// Parse a single ESLint message
    fn parse_eslint_message(message: &serde_json::Value, file_path: &Path) -> Option<CompilerDiagnostic> {
        let line = message.get("line")?.as_u64()? as u32;
        let column = message.get("column")?.as_u64()? as u32;
        let msg = message.get("message")?.as_str()?.to_string();
        let severity_num = message.get("severity")?.as_u64()?;
        let rule_id = message.get("ruleId").and_then(|r| r.as_str()).map(|s| s.to_string());

        let severity = match severity_num {
            1 => DiagnosticSeverity::Warning,
            2 => DiagnosticSeverity::Error,
            _ => DiagnosticSeverity::Info,
        };

        Some(CompilerDiagnostic {
            severity,
            message: msg,
            code: rule_id,
            location: DiagnosticLocation { line, column, length: None },
            file_path: file_path.to_path_buf(),
            suggestions: Vec::new(),
        })
    }

    /// Parse mypy diagnostics
    fn parse_mypy_diagnostics(output: &[u8], file_path: &Path) -> Option<FileDiagnostics> {
        let output_str = String::from_utf8_lossy(output);
        let mut diagnostics = FileDiagnostics::default();

        // Parse mypy output format: filename:line: severity: message [error-code]
        for line in output_str.lines() {
            if let Some(diagnostic) = Self::parse_mypy_line(line, file_path) {
                diagnostics.add_diagnostic(diagnostic);
            }
        }

        if diagnostics.total_count() > 0 {
            Some(diagnostics)
        } else {
            None
        }
    }

    /// Parse a single mypy output line
    fn parse_mypy_line(line: &str, file_path: &Path) -> Option<CompilerDiagnostic> {
        let parts: Vec<&str> = line.split(": ").collect();
        if parts.len() < 3 {
            return None;
        }

        // Extract line number from filename:line
        let location_parts: Vec<&str> = parts[0].split(':').collect();
        if location_parts.len() < 2 {
            return None;
        }

        let line_num: u32 = location_parts[1].parse().ok()?;
        let severity_str = parts[1];
        let message_part = parts[2..].join(": ");

        let severity = match severity_str {
            "error" => DiagnosticSeverity::Error,
            "warning" => DiagnosticSeverity::Warning,
            "note" => DiagnosticSeverity::Info,
            _ => DiagnosticSeverity::Error,
        };

        // Extract error code from brackets if present
        let code = if message_part.contains('[') && message_part.contains(']') {
            let start = message_part.rfind('[')? + 1;
            let end = message_part.rfind(']')?;
            Some(message_part[start..end].to_string())
        } else {
            None
        };

        Some(CompilerDiagnostic {
            severity,
            message: message_part.to_string(),
            code,
            location: DiagnosticLocation { line: line_num, column: 1, length: None },
            file_path: file_path.to_path_buf(),
            suggestions: Vec::new(),
        })
    }

    /// Parse flake8 diagnostics
    fn parse_flake8_diagnostics(output: &[u8], file_path: &Path) -> Option<FileDiagnostics> {
        let output_str = String::from_utf8_lossy(output);
        let mut diagnostics = FileDiagnostics::default();

        // Parse flake8 output format: filename:line:col: code message
        for line in output_str.lines() {
            if let Some(diagnostic) = Self::parse_flake8_line(line, file_path) {
                diagnostics.add_diagnostic(diagnostic);
            }
        }

        if diagnostics.total_count() > 0 {
            Some(diagnostics)
        } else {
            None
        }
    }

    /// Parse a single flake8 output line
    fn parse_flake8_line(line: &str, file_path: &Path) -> Option<CompilerDiagnostic> {
        let parts: Vec<&str> = line.split(": ").collect();
        if parts.len() < 2 {
            return None;
        }

        let location_parts: Vec<&str> = parts[0].split(':').collect();
        if location_parts.len() < 3 {
            return None;
        }

        let line_num: u32 = location_parts[1].parse().ok()?;
        let col_num: u32 = location_parts[2].parse().ok()?;
        
        let message_part = parts[1];
        let code = message_part.split_whitespace().next().map(|s| s.to_string());

        Some(CompilerDiagnostic {
            severity: DiagnosticSeverity::Warning, // flake8 mostly reports style warnings
            message: message_part.to_string(),
            code,
            location: DiagnosticLocation { line: line_num, column: col_num, length: None },
            file_path: file_path.to_path_buf(),
            suggestions: Vec::new(),
        })
    }

    /// Parse Go vet diagnostics
    fn parse_go_diagnostics(output: &[u8], file_path: &Path) -> Option<FileDiagnostics> {
        let output_str = String::from_utf8_lossy(output);
        let mut diagnostics = FileDiagnostics::default();

        // Parse Go vet output format: filename:line:col: message
        for line in output_str.lines() {
            if let Some(diagnostic) = Self::parse_go_line(line, file_path) {
                diagnostics.add_diagnostic(diagnostic);
            }
        }

        if diagnostics.total_count() > 0 {
            Some(diagnostics)
        } else {
            None
        }
    }

    /// Parse a single Go vet output line
    fn parse_go_line(line: &str, file_path: &Path) -> Option<CompilerDiagnostic> {
        let parts: Vec<&str> = line.split(": ").collect();
        if parts.len() < 2 {
            return None;
        }

        let location_parts: Vec<&str> = parts[0].split(':').collect();
        if location_parts.len() < 3 {
            return None;
        }

        let line_num: u32 = location_parts[1].parse().ok()?;
        let col_num: u32 = location_parts[2].parse().ok()?;
        let message = parts[1..].join(": ");

        Some(CompilerDiagnostic {
            severity: DiagnosticSeverity::Warning, // Go vet reports warnings
            message,
            code: None,
            location: DiagnosticLocation { line: line_num, column: col_num, length: None },
            file_path: file_path.to_path_buf(),
            suggestions: Vec::new(),
        })
    }

    /// Parse Java javac diagnostics
    fn parse_java_diagnostics(output: &[u8], file_path: &Path) -> Option<FileDiagnostics> {
        let output_str = String::from_utf8_lossy(output);
        let mut diagnostics = FileDiagnostics::default();

        // Parse javac output format: filename:line: severity: message
        for line in output_str.lines() {
            if let Some(diagnostic) = Self::parse_java_line(line, file_path) {
                diagnostics.add_diagnostic(diagnostic);
            }
        }

        if diagnostics.total_count() > 0 {
            Some(diagnostics)
        } else {
            None
        }
    }

    /// Parse a single Java javac output line
    fn parse_java_line(line: &str, file_path: &Path) -> Option<CompilerDiagnostic> {
        let parts: Vec<&str> = line.split(": ").collect();
        if parts.len() < 3 {
            return None;
        }

        let location_parts: Vec<&str> = parts[0].split(':').collect();
        if location_parts.len() < 2 {
            return None;
        }

        let line_num: u32 = location_parts[1].parse().ok()?;
        let severity_str = parts[1];
        let message = parts[2..].join(": ");

        let severity = match severity_str {
            "error" => DiagnosticSeverity::Error,
            "warning" => DiagnosticSeverity::Warning,
            _ => DiagnosticSeverity::Error,
        };

        Some(CompilerDiagnostic {
            severity,
            message,
            code: None,
            location: DiagnosticLocation { line: line_num, column: 1, length: None },
            file_path: file_path.to_path_buf(),
            suggestions: Vec::new(),
        })
    }
}