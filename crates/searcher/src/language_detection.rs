/*!
Language detection for AST-based context calculation.

This module provides functionality to detect the programming language of a file
and create appropriate AST calculators for that language.
*/

use std::path::Path;

use outgrep_ast_core::{
    tree_sitter::{LanguageExt, StrDoc},
    Language,
};
use outgrep_ast_language::SupportLang;

use crate::ast_context::{
    default_context_types, AstContextCalculator, AstContextError,
    AstContextType,
};

/// Detects the programming language from a file path and creates an AST context calculator.
///
/// This function fails fast - if the language is not supported or AST parsing fails,
/// it returns an error rather than falling back to line-based context.
pub fn create_ast_calculator_for_file(
    file_path: &Path,
    source: &str,
    context_types: Option<Vec<AstContextType>>,
) -> Result<AstContextCalculatorWrapper, AstContextError> {
    let lang = SupportLang::from_path(file_path).ok_or_else(|| {
        AstContextError::UnsupportedLanguage(format!(
            "File extension not supported for AST parsing: {}",
            file_path.to_string_lossy()
        ))
    })?;

    let context_types = context_types.unwrap_or_else(default_context_types);

    // This will fail if AST parsing fails - no fallback
    AstContextCalculatorWrapper::new(lang, source, context_types)
}

/// Wrapper around AST context calculator that handles different language types.
pub enum AstContextCalculatorWrapper {
    /// Calculator for a specific supported language
    Calculator(Box<dyn AstCalculator>),
}

impl AstContextCalculatorWrapper {
    /// Create a new calculator wrapper for the given language.
    pub fn new(
        lang: SupportLang,
        source: &str,
        context_types: Vec<AstContextType>,
    ) -> Result<Self, AstContextError> {
        // Macro to create calculator with error handling
        macro_rules! create_calculator {
            ($lang_impl:expr, $lang_name:expr) => {{
                // Try to parse the source with ast-grep
                let ast_grep = $lang_impl.ast_grep(source);

                // Check if parsing actually succeeded by trying to get the root
                let root = ast_grep.root();
                if root.range().start == 0 && root.range().end == 0 && !source.is_empty() {
                    return Err(AstContextError::ParseFailed {
                        language: $lang_name.to_string(),
                        reason: "Tree-sitter parser returned empty tree for non-empty source".to_string(),
                    });
                }

                Box::new(AstContextCalculator::new(ast_grep, context_types.clone())) as Box<dyn AstCalculator>
            }};
        }

        let calculator: Box<dyn AstCalculator> = match lang {
            SupportLang::Rust => {
                create_calculator!(outgrep_ast_language::Rust, "Rust")
            }
            SupportLang::JavaScript => {
                create_calculator!(
                    outgrep_ast_language::JavaScript,
                    "JavaScript"
                )
            }
            SupportLang::TypeScript => {
                create_calculator!(
                    outgrep_ast_language::TypeScript,
                    "TypeScript"
                )
            }
            SupportLang::Python => {
                create_calculator!(outgrep_ast_language::Python, "Python")
            }
            SupportLang::Go => {
                create_calculator!(outgrep_ast_language::Go, "Go")
            }
            SupportLang::Java => {
                create_calculator!(outgrep_ast_language::Java, "Java")
            }
            SupportLang::C => {
                create_calculator!(outgrep_ast_language::C, "C")
            }
            SupportLang::Cpp => {
                create_calculator!(outgrep_ast_language::Cpp, "C++")
            }
            SupportLang::CSharp => {
                create_calculator!(outgrep_ast_language::CSharp, "C#")
            }
            SupportLang::Ruby => {
                create_calculator!(outgrep_ast_language::Ruby, "Ruby")
            }
            SupportLang::Php => {
                create_calculator!(outgrep_ast_language::Php, "PHP")
            }
            SupportLang::Swift => {
                create_calculator!(outgrep_ast_language::Swift, "Swift")
            }
            SupportLang::Kotlin => {
                create_calculator!(outgrep_ast_language::Kotlin, "Kotlin")
            }
            SupportLang::Scala => {
                create_calculator!(outgrep_ast_language::Scala, "Scala")
            }
            SupportLang::Haskell => {
                create_calculator!(outgrep_ast_language::Haskell, "Haskell")
            }
            SupportLang::Elixir => {
                create_calculator!(outgrep_ast_language::Elixir, "Elixir")
            }
            SupportLang::Lua => {
                create_calculator!(outgrep_ast_language::Lua, "Lua")
            }
            SupportLang::Bash => {
                create_calculator!(outgrep_ast_language::Bash, "Bash")
            }
            // For languages without complex scoping, we can still try basic parsing
            SupportLang::Html => {
                create_calculator!(outgrep_ast_language::Html, "HTML")
            }
            SupportLang::Css => {
                create_calculator!(outgrep_ast_language::Css, "CSS")
            }
            SupportLang::Json => {
                create_calculator!(outgrep_ast_language::Json, "JSON")
            }
            SupportLang::Yaml => {
                create_calculator!(outgrep_ast_language::Yaml, "YAML")
            }
            SupportLang::Tsx => {
                create_calculator!(outgrep_ast_language::Tsx, "TSX")
            }
        };

        Ok(Self::Calculator(calculator))
    }

    /// Calculate context for the given match range.
    pub fn calculate_context(
        &self,
        match_range: std::ops::Range<usize>,
    ) -> Result<crate::ast_context::AstContextResult, AstContextError> {
        match self {
            Self::Calculator(calc) => calc.calculate_context(match_range),
        }
    }
}

/// Trait for type-erased AST calculators.
pub trait AstCalculator {
    /// Calculate context for the given match range.
    fn calculate_context(
        &self,
        match_range: std::ops::Range<usize>,
    ) -> Result<crate::ast_context::AstContextResult, AstContextError>;

    /// Get syntax highlighting information as (range, kind) pairs.
    fn get_syntax_nodes(&self) -> Vec<(std::ops::Range<usize>, String)>;
}

impl<D> AstCalculator for AstContextCalculator<StrDoc<D>>
where
    D: LanguageExt,
{
    fn calculate_context(
        &self,
        match_range: std::ops::Range<usize>,
    ) -> Result<crate::ast_context::AstContextResult, AstContextError> {
        self.calculate_context(match_range)
    }

    fn get_syntax_nodes(&self) -> Vec<(std::ops::Range<usize>, String)> {
        // Simple string-based approach for clean syntax highlighting
        // This avoids AST node fragmentation issues and external dependencies

        let root = self.get_root_node();
        let content = root.text();
        let mut tokens = Vec::new();

        // Define keywords for different languages
        let keywords = [
            // Rust keywords
            "fn", "let", "mut", "const", "if", "else", "for", "while", "loop",
            "match", "return", "struct", "enum", "impl", "trait", "pub",
            "use", "mod", "crate", "self", "super", "where", "unsafe",
            "async", "await", "true", "false", "None", "Some",
            // Python keywords
            "def", "class", "import", "from", "elif", "try", "except",
            "finally", "with", "as", "yield", "break", "continue", "pass",
            "lambda", "global", "nonlocal", "True", "False",
            // Common keywords across languages
            "if", "else", "for", "while", "return", "import", "true", "false",
            "null",
        ];

        // Find keyword matches
        for keyword in keywords.iter() {
            let mut start = 0;
            while let Some(pos) = content[start..].find(keyword) {
                let abs_pos = start + pos;
                let end_pos = abs_pos + keyword.len();

                // Check word boundaries (simple approach)
                let before_ok = abs_pos == 0
                    || !content
                        .chars()
                        .nth(abs_pos - 1)
                        .unwrap_or(' ')
                        .is_alphanumeric();
                let after_ok = end_pos >= content.len()
                    || !content
                        .chars()
                        .nth(end_pos)
                        .unwrap_or(' ')
                        .is_alphanumeric();

                if before_ok && after_ok {
                    let range = abs_pos..end_pos;

                    // Check for overlaps
                    let overlaps = tokens.iter().any(
                        |(existing_range, _): &(
                            std::ops::Range<usize>,
                            String,
                        )| {
                            range.start < existing_range.end
                                && existing_range.start < range.end
                        },
                    );

                    if !overlaps {
                        tokens.push((range, "keyword".to_string()));
                    }
                }

                start = abs_pos + 1;
            }
        }

        // Find string literals (simple quotes)
        let string_patterns = ['"', '\''];
        for quote in string_patterns.iter() {
            let mut start = 0;
            while let Some(start_pos) = content[start..].find(*quote) {
                let abs_start = start + start_pos;
                if let Some(end_pos) = content[abs_start + 1..].find(*quote) {
                    let abs_end = abs_start + 1 + end_pos + 1;
                    let range = abs_start..abs_end;

                    // Check for overlaps
                    let overlaps = tokens.iter().any(
                        |(existing_range, _): &(
                            std::ops::Range<usize>,
                            String,
                        )| {
                            range.start < existing_range.end
                                && existing_range.start < range.end
                        },
                    );

                    if !overlaps {
                        tokens.push((range, "string".to_string()));
                    }

                    start = abs_end;
                } else {
                    break;
                }
            }
        }

        // Find comments
        let mut start = 0;
        while let Some(pos) = content[start..].find("//") {
            let abs_pos = start + pos;
            if let Some(end_pos) = content[abs_pos..].find('\n') {
                let abs_end = abs_pos + end_pos;
                let range = abs_pos..abs_end;

                // Check for overlaps
                let overlaps = tokens.iter().any(
                    |(existing_range, _): &(
                        std::ops::Range<usize>,
                        String,
                    )| {
                        range.start < existing_range.end
                            && existing_range.start < range.end
                    },
                );

                if !overlaps {
                    tokens.push((range, "comment".to_string()));
                }

                start = abs_end;
            } else {
                // Comment to end of file
                let range = abs_pos..content.len();
                let overlaps = tokens.iter().any(
                    |(existing_range, _): &(
                        std::ops::Range<usize>,
                        String,
                    )| {
                        range.start < existing_range.end
                            && existing_range.start < range.end
                    },
                );

                if !overlaps {
                    tokens.push((range, "comment".to_string()));
                }
                break;
            }
        }

        // Find Python-style comments
        start = 0;
        while let Some(pos) = content[start..].find('#') {
            let abs_pos = start + pos;
            if let Some(end_pos) = content[abs_pos..].find('\n') {
                let abs_end = abs_pos + end_pos;
                let range = abs_pos..abs_end;

                let overlaps = tokens.iter().any(
                    |(existing_range, _): &(
                        std::ops::Range<usize>,
                        String,
                    )| {
                        range.start < existing_range.end
                            && existing_range.start < range.end
                    },
                );

                if !overlaps {
                    tokens.push((range, "comment".to_string()));
                }

                start = abs_end;
            } else {
                let range = abs_pos..content.len();
                let overlaps = tokens.iter().any(
                    |(existing_range, _): &(
                        std::ops::Range<usize>,
                        String,
                    )| {
                        range.start < existing_range.end
                            && existing_range.start < range.end
                    },
                );

                if !overlaps {
                    tokens.push((range, "comment".to_string()));
                }
                break;
            }
        }

        // Sort by start position
        tokens.sort_by_key(|(range, _)| range.start);
        tokens
    }
}

// Removed redundant From implementation - it's already auto-generated

/// Check if a file extension is supported by AST parsing.
pub fn is_supported_file(path: &Path) -> bool {
    SupportLang::from_path(path).is_some()
}

/// Get the supported language for a file path.
pub fn get_language_for_file(path: &Path) -> Option<SupportLang> {
    SupportLang::from_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_language_detection() {
        assert_eq!(
            get_language_for_file(&PathBuf::from("test.rs")),
            Some(SupportLang::Rust)
        );
        assert_eq!(
            get_language_for_file(&PathBuf::from("test.js")),
            Some(SupportLang::JavaScript)
        );
        assert_eq!(
            get_language_for_file(&PathBuf::from("test.py")),
            Some(SupportLang::Python)
        );
        assert_eq!(
            get_language_for_file(&PathBuf::from("test.unknown")),
            None
        );
    }

    #[test]
    fn test_supported_file_check() {
        assert!(is_supported_file(&PathBuf::from("main.rs")));
        assert!(is_supported_file(&PathBuf::from("script.py")));
        assert!(!is_supported_file(&PathBuf::from("data.bin")));
    }
}
