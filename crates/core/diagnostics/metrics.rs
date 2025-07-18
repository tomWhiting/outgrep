use crate::diagnostics::types::CodeMetrics;
use std::path::Path;

pub struct MetricsCalculator;

#[derive(Debug)]
struct BasicMetrics {
    code: u64,
    comments: u64,
    blanks: u64,
}

#[derive(Debug)]
struct ComplexityMetrics {
    cyclomatic_complexity: u32,
    cognitive_complexity: u32,
    function_count: u32,
}

#[derive(Debug, PartialEq)]
enum Language {
    Rust,
    JavaScript,
    TypeScript,
    Python,
    Java,
    Go,
    Cpp,
    C,
    Php,
    Ruby,
    CSharp,
    Swift,
    Unknown,
}

impl MetricsCalculator {
    /// Calculate comprehensive code metrics for a file
    pub fn calculate_metrics(path: &Path, content: &str) -> Result<CodeMetrics, Box<dyn std::error::Error>> {
        // Use our own line counting
        let basic_metrics = Self::calculate_basic_metrics(path, content);
        
        // Add our own complexity and function counting
        let complexity_metrics = Self::calculate_complexity_metrics(path, content);
        
        Ok(CodeMetrics {
            lines_of_code: basic_metrics.code,
            comment_lines: basic_metrics.comments,
            blank_lines: basic_metrics.blanks,
            cyclomatic_complexity: complexity_metrics.cyclomatic_complexity,
            cognitive_complexity: complexity_metrics.cognitive_complexity,
            function_count: complexity_metrics.function_count,
        })
    }
    
    /// Calculate basic line metrics
    fn calculate_basic_metrics(path: &Path, content: &str) -> BasicMetrics {
        let language = Self::detect_language_from_extension(path);
        
        let mut code_lines = 0;
        let mut comment_lines = 0;
        let mut blank_lines = 0;
        
        for line in content.lines() {
            let trimmed = line.trim();
            
            if trimmed.is_empty() {
                blank_lines += 1;
            } else if Self::is_comment_line(&trimmed, &language) {
                comment_lines += 1;
            } else {
                code_lines += 1;
            }
        }
        
        BasicMetrics {
            code: code_lines,
            comments: comment_lines,
            blanks: blank_lines,
        }
    }
    
    /// Check if a line is a comment based on language
    fn is_comment_line(line: &str, language: &Language) -> bool {
        match language {
            Language::Rust | Language::JavaScript | Language::TypeScript 
            | Language::Java | Language::Go | Language::Cpp | Language::C 
            | Language::CSharp | Language::Swift => {
                line.starts_with("//") || line.starts_with("/*") || line.starts_with("*") || line.starts_with("///")
            }
            Language::Python | Language::Ruby => {
                line.starts_with("#")
            }
            Language::Php => {
                line.starts_with("//") || line.starts_with("#") || line.starts_with("/*")
            }
            Language::Unknown => {
                line.starts_with("//") || line.starts_with("#") || line.starts_with("/*") || line.starts_with("*")
            }
        }
    }
    
    /// Calculate complexity and function metrics
    fn calculate_complexity_metrics(path: &Path, content: &str) -> ComplexityMetrics {
        let language = Self::detect_language_from_extension(path);
        
        match language {
            Language::Rust => Self::calculate_rust_metrics(content),
            Language::JavaScript | Language::TypeScript => Self::calculate_js_metrics(content),
            Language::Python => Self::calculate_python_metrics(content),
            Language::Java => Self::calculate_java_metrics(content),
            Language::Go => Self::calculate_go_metrics(content),
            _ => Self::calculate_generic_metrics(content),
        }
    }
    
    /// Detect language from file extension
    fn detect_language_from_extension(path: &Path) -> Language {
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            match extension.to_lowercase().as_str() {
                "rs" => Language::Rust,
                "js" | "jsx" => Language::JavaScript,
                "ts" | "tsx" => Language::TypeScript,
                "py" => Language::Python,
                "java" => Language::Java,
                "go" => Language::Go,
                "cpp" | "cc" | "cxx" | "c++" => Language::Cpp,
                "c" => Language::C,
                "php" => Language::Php,
                "rb" => Language::Ruby,
                "cs" => Language::CSharp,
                "swift" => Language::Swift,
                _ => Language::Unknown,
            }
        } else {
            Language::Unknown
        }
    }
    
    /// Calculate Rust-specific metrics
    fn calculate_rust_metrics(content: &str) -> ComplexityMetrics {
        let mut function_count = 0;
        let mut cyclomatic_complexity = 1; // Base complexity
        
        for line in content.lines() {
            let trimmed = line.trim();
            
            // Count functions
            if trimmed.starts_with("fn ") || trimmed.contains(" fn ") {
                function_count += 1;
            }
            
            // Count complexity-adding constructs
            if trimmed.contains("if ") || trimmed.contains("else if ") {
                cyclomatic_complexity += 1;
            }
            if trimmed.contains("while ") || trimmed.contains("for ") {
                cyclomatic_complexity += 1;
            }
            if trimmed.contains("match ") {
                cyclomatic_complexity += 1;
            }
            if trimmed.contains("&&") || trimmed.contains("||") {
                cyclomatic_complexity += 1;
            }
        }
        
        ComplexityMetrics {
            cyclomatic_complexity,
            cognitive_complexity: (cyclomatic_complexity as f32 * 0.8) as u32, // Approximation
            function_count,
        }
    }
    
    /// Calculate JavaScript/TypeScript metrics
    fn calculate_js_metrics(content: &str) -> ComplexityMetrics {
        let mut function_count = 0;
        let mut cyclomatic_complexity = 1;
        
        for line in content.lines() {
            let trimmed = line.trim();
            
            // Count functions
            if trimmed.contains("function ") || trimmed.contains("=> ") || trimmed.contains("() {") {
                function_count += 1;
            }
            
            // Count complexity
            if trimmed.contains("if (") || trimmed.contains("else if") {
                cyclomatic_complexity += 1;
            }
            if trimmed.contains("while (") || trimmed.contains("for (") {
                cyclomatic_complexity += 1;
            }
            if trimmed.contains("switch (") {
                cyclomatic_complexity += 1;
            }
            if trimmed.contains("&&") || trimmed.contains("||") {
                cyclomatic_complexity += 1;
            }
        }
        
        ComplexityMetrics {
            cyclomatic_complexity,
            cognitive_complexity: (cyclomatic_complexity as f32 * 0.9) as u32,
            function_count,
        }
    }
    
    /// Calculate Python metrics
    fn calculate_python_metrics(content: &str) -> ComplexityMetrics {
        let mut function_count = 0;
        let mut cyclomatic_complexity = 1;
        
        for line in content.lines() {
            let trimmed = line.trim();
            
            // Count functions
            if trimmed.starts_with("def ") {
                function_count += 1;
            }
            
            // Count complexity
            if trimmed.starts_with("if ") || trimmed.starts_with("elif ") {
                cyclomatic_complexity += 1;
            }
            if trimmed.starts_with("while ") || trimmed.starts_with("for ") {
                cyclomatic_complexity += 1;
            }
            if trimmed.contains(" and ") || trimmed.contains(" or ") {
                cyclomatic_complexity += 1;
            }
        }
        
        ComplexityMetrics {
            cyclomatic_complexity,
            cognitive_complexity: cyclomatic_complexity,
            function_count,
        }
    }
    
    /// Calculate Java metrics
    fn calculate_java_metrics(content: &str) -> ComplexityMetrics {
        let mut function_count = 0;
        let mut cyclomatic_complexity = 1;
        
        for line in content.lines() {
            let trimmed = line.trim();
            
            // Count methods (simplified)
            if (trimmed.contains("public ") || trimmed.contains("private ") || trimmed.contains("protected ")) 
                && trimmed.contains("(") && trimmed.contains(")") && trimmed.contains("{") {
                function_count += 1;
            }
            
            // Count complexity
            if trimmed.contains("if (") || trimmed.contains("else if") {
                cyclomatic_complexity += 1;
            }
            if trimmed.contains("while (") || trimmed.contains("for (") {
                cyclomatic_complexity += 1;
            }
            if trimmed.contains("switch (") {
                cyclomatic_complexity += 1;
            }
        }
        
        ComplexityMetrics {
            cyclomatic_complexity,
            cognitive_complexity: (cyclomatic_complexity as f32 * 1.1) as u32,
            function_count,
        }
    }
    
    /// Calculate Go metrics
    fn calculate_go_metrics(content: &str) -> ComplexityMetrics {
        let mut function_count = 0;
        let mut cyclomatic_complexity = 1;
        
        for line in content.lines() {
            let trimmed = line.trim();
            
            // Count functions
            if trimmed.starts_with("func ") {
                function_count += 1;
            }
            
            // Count complexity
            if trimmed.contains("if ") {
                cyclomatic_complexity += 1;
            }
            if trimmed.contains("for ") || trimmed.contains("range ") {
                cyclomatic_complexity += 1;
            }
            if trimmed.contains("switch ") {
                cyclomatic_complexity += 1;
            }
        }
        
        ComplexityMetrics {
            cyclomatic_complexity,
            cognitive_complexity: cyclomatic_complexity,
            function_count,
        }
    }
    
    /// Generic metrics for unknown languages
    fn calculate_generic_metrics(content: &str) -> ComplexityMetrics {
        let mut function_count = 0;
        let mut cyclomatic_complexity = 1;
        
        for line in content.lines() {
            let trimmed = line.trim();
            
            // Basic function detection
            if trimmed.contains("function ") || trimmed.contains("def ") 
                || trimmed.contains("fn ") || trimmed.contains("func ") {
                function_count += 1;
            }
            
            // Basic complexity detection
            if trimmed.contains("if") || trimmed.contains("while") 
                || trimmed.contains("for") || trimmed.contains("switch") {
                cyclomatic_complexity += 1;
            }
        }
        
        ComplexityMetrics {
            cyclomatic_complexity,
            cognitive_complexity: cyclomatic_complexity,
            function_count,
        }
    }
    
    /// Get a summary string of the metrics
    pub fn metrics_summary(metrics: &CodeMetrics) -> String {
        format!(
            "LOC: {}, Comments: {}, Blank: {}, Functions: {}, Complexity: {}",
            metrics.lines_of_code,
            metrics.comment_lines,
            metrics.blank_lines,
            metrics.function_count,
            metrics.cyclomatic_complexity
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    #[test]
    fn test_rust_metrics() {
        let rust_code = r#"
/// This is a documentation comment
fn main() {
    println!("Hello, world!");
    
    if true {
        println!("Condition met");
    }
}

fn another_function() -> i32 {
    // This is a comment
    42
}
"#;
        
        let path = PathBuf::from("test.rs");
        let metrics = MetricsCalculator::calculate_metrics(&path, rust_code).unwrap();
        
        assert!(metrics.lines_of_code > 0);
        assert!(metrics.comment_lines > 0);
        assert!(metrics.function_count >= 2);
        assert!(metrics.cyclomatic_complexity >= 1);
        
        println!("Rust metrics: {}", MetricsCalculator::metrics_summary(&metrics));
    }
    
    #[test]
    fn test_javascript_metrics() {
        let js_code = r#"
// This is a comment
function hello() {
    console.log("Hello");
    
    if (condition) {
        return true;
    }
    return false;
}

const arrow = () => {
    // Another comment
    console.log("Arrow function");
};
"#;
        
        let path = PathBuf::from("test.js");
        let metrics = MetricsCalculator::calculate_metrics(&path, js_code).unwrap();
        
        assert!(metrics.lines_of_code > 0);
        assert!(metrics.comment_lines > 0);
        assert!(metrics.function_count >= 2);
        assert!(metrics.cyclomatic_complexity >= 1);
        
        println!("JavaScript metrics: {}", MetricsCalculator::metrics_summary(&metrics));
    }
    
    #[test]
    fn test_language_detection() {
        assert_eq!(MetricsCalculator::detect_language_from_extension(&PathBuf::from("test.rs")), Language::Rust);
        assert_eq!(MetricsCalculator::detect_language_from_extension(&PathBuf::from("test.js")), Language::JavaScript);
        assert_eq!(MetricsCalculator::detect_language_from_extension(&PathBuf::from("test.py")), Language::Python);
        assert_eq!(MetricsCalculator::detect_language_from_extension(&PathBuf::from("test.unknown")), Language::Unknown);
    }
    
    #[test]
    fn test_metrics_summary() {
        let metrics = CodeMetrics {
            lines_of_code: 100,
            comment_lines: 20,
            blank_lines: 10,
            cyclomatic_complexity: 15,
            cognitive_complexity: 12,
            function_count: 8,
        };
        
        let summary = MetricsCalculator::metrics_summary(&metrics);
        assert!(summary.contains("LOC: 100"));
        assert!(summary.contains("Comments: 20"));
        assert!(summary.contains("Functions: 8"));
        assert!(summary.contains("Complexity: 15"));
    }
}