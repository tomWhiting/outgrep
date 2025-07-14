/*!
Language detection for AST-based context calculation.

This module provides functionality to detect the programming language of a file
and create appropriate AST calculators for that language.
*/

use std::path::Path;

use outgrep_ast_core::{Language, tree_sitter::{LanguageExt, StrDoc}};
use outgrep_ast_language::SupportLang;

use crate::ast_context::{AstContextCalculator, AstContextError, AstContextType, default_context_types};

/// Detects the programming language from a file path and creates an AST context calculator.
pub fn create_ast_calculator_for_file(
    file_path: &Path,
    source: &str,
    context_types: Option<Vec<AstContextType>>,
) -> Result<AstContextCalculatorWrapper, AstContextError> {
    let lang = SupportLang::from_path(file_path)
        .ok_or_else(|| AstContextError::UnsupportedLanguage(
            file_path.to_string_lossy().to_string()
        ))?;

    let context_types = context_types.unwrap_or_else(default_context_types);
    
    Ok(AstContextCalculatorWrapper::new(lang, source, context_types)?)
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
        let calculator: Box<dyn AstCalculator> = match lang {
            SupportLang::Rust => {
                let ast_grep = outgrep_ast_language::Rust.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::JavaScript => {
                let ast_grep = outgrep_ast_language::JavaScript.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::TypeScript => {
                let ast_grep = outgrep_ast_language::TypeScript.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Python => {
                let ast_grep = outgrep_ast_language::Python.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Go => {
                let ast_grep = outgrep_ast_language::Go.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Java => {
                let ast_grep = outgrep_ast_language::Java.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::C => {
                let ast_grep = outgrep_ast_language::C.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Cpp => {
                let ast_grep = outgrep_ast_language::Cpp.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::CSharp => {
                let ast_grep = outgrep_ast_language::CSharp.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Ruby => {
                let ast_grep = outgrep_ast_language::Ruby.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Php => {
                let ast_grep = outgrep_ast_language::Php.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Swift => {
                let ast_grep = outgrep_ast_language::Swift.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Kotlin => {
                let ast_grep = outgrep_ast_language::Kotlin.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Scala => {
                let ast_grep = outgrep_ast_language::Scala.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Haskell => {
                let ast_grep = outgrep_ast_language::Haskell.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Elixir => {
                let ast_grep = outgrep_ast_language::Elixir.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Lua => {
                let ast_grep = outgrep_ast_language::Lua.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Bash => {
                let ast_grep = outgrep_ast_language::Bash.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            // For languages without complex scoping, we can still try basic parsing
            SupportLang::Html => {
                let ast_grep = outgrep_ast_language::Html.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Css => {
                let ast_grep = outgrep_ast_language::Css.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Json => {
                let ast_grep = outgrep_ast_language::Json.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Yaml => {
                let ast_grep = outgrep_ast_language::Yaml.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
            SupportLang::Tsx => {
                let ast_grep = outgrep_ast_language::Tsx.ast_grep(source);
                Box::new(AstContextCalculator::new(ast_grep, context_types))
            },
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
        assert_eq!(get_language_for_file(&PathBuf::from("test.rs")), Some(SupportLang::Rust));
        assert_eq!(get_language_for_file(&PathBuf::from("test.js")), Some(SupportLang::JavaScript));
        assert_eq!(get_language_for_file(&PathBuf::from("test.py")), Some(SupportLang::Python));
        assert_eq!(get_language_for_file(&PathBuf::from("test.unknown")), None);
    }

    #[test]
    fn test_supported_file_check() {
        assert!(is_supported_file(&PathBuf::from("main.rs")));
        assert!(is_supported_file(&PathBuf::from("script.py")));
        assert!(!is_supported_file(&PathBuf::from("data.bin")));
    }
}