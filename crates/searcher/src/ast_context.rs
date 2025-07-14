/*!
AST-based context calculation using ast-grep for semantic boundaries.

This module provides functionality to find enclosing symbols (functions, classes,
methods, etc.) around search matches using Abstract Syntax Tree analysis.
*/

use std::ops::Range;
use outgrep_ast_core::{AstGrep, Doc, Node};

/// Types of AST nodes that can provide meaningful context.
#[derive(Debug, Clone, PartialEq)]
pub enum AstContextType {
    /// Function declarations and definitions
    Function,
    /// Class/struct/interface declarations
    Class,
    /// Method definitions within classes
    Method,
    /// Block statements (if, for, while, etc.)
    Block,
    /// Module/namespace definitions
    Module,
    /// Type definitions (enum, union, typedef, etc.)
    TypeDef,
}

/// Result of AST context calculation.
#[derive(Debug)]
pub struct AstContextResult {
    /// The byte range of the enclosing symbol
    pub range: Range<usize>,
    /// Type of the enclosing node
    pub context_type: AstContextType,
    /// Name of the symbol if available (e.g., function name)
    pub symbol_name: Option<String>,
    /// Nesting level of the symbol
    pub depth: u32,
}

/// Calculator for AST-based context using ast-grep.
pub struct AstContextCalculator<D: Doc> {
    /// The ast-grep root
    ast_grep: AstGrep<D>,
    /// Types of context nodes we're interested in
    context_types: Vec<AstContextType>,
}

impl<D: Doc> AstContextCalculator<D> {
    /// Create a new AST context calculator.
    pub fn new(
        ast_grep: AstGrep<D>,
        context_types: Vec<AstContextType>,
    ) -> Self {
        Self {
            ast_grep,
            context_types,
        }
    }

    /// Calculate the enclosing symbol context for a given match range.
    pub fn calculate_context(
        &self,
        match_range: Range<usize>,
    ) -> Result<AstContextResult, AstContextError> {
        // Start from the root and find the deepest enclosing node
        let root = self.ast_grep.root();
        let mut best_node: Option<Node<D>> = None;
        let mut best_depth = 0;

        // Find enclosing node recursively
        self.find_enclosing_node_recursive(
            root,
            match_range.clone(),
            &mut best_node,
            &mut best_depth,
            0,
        );

        if let Some(node) = best_node {
            let context_type = self.classify_node(&node)?;
            let symbol_name = self.extract_symbol_name(&node);

            Ok(AstContextResult {
                range: node.range().start..node.range().end,
                context_type,
                symbol_name,
                depth: best_depth,
            })
        } else {
            Err(AstContextError::NoEnclosingSymbol {
                range: match_range
            })
        }
    }

    /// Recursively find the best enclosing node.
    fn find_enclosing_node_recursive<'a>(
        &self,
        node: Node<'a, D>,
        match_range: Range<usize>,
        best_node: &mut Option<Node<'a, D>>,
        best_depth: &mut u32,
        current_depth: u32,
    ) {
        let node_range = node.range();
        
        // Check if this node contains our match range
        if node_range.start <= match_range.start && node_range.end >= match_range.end {
            // Check if this node type is one we're interested in
            if self.is_context_node(&node) {
                // Update best node if this is deeper (more specific)
                if current_depth > *best_depth {
                    *best_node = Some(node.clone());
                    *best_depth = current_depth;
                }
            }

            // Recurse into children
            for child in node.children() {
                self.find_enclosing_node_recursive(
                    child,
                    match_range.clone(),
                    best_node,
                    best_depth,
                    current_depth + 1,
                );
            }
        }
    }

    /// Check if a node type is one of our target context types.
    fn is_context_node(&self, node: &Node<D>) -> bool {
        let kind = node.kind();
        
        for context_type in &self.context_types {
            if self.node_matches_context_type(&kind, context_type) {
                return true;
            }
        }
        false
    }

    /// Determine if a node kind matches a context type.
    fn node_matches_context_type(&self, kind: &str, context_type: &AstContextType) -> bool {
        match context_type {
            AstContextType::Function => {
                matches!(kind, 
                    "function_declaration" | "function_definition" | "function_item" |
                    "method_definition" | "function" | "arrow_function" |
                    "function_expression" | "generator_function" | "async_function"
                )
            },
            AstContextType::Class => {
                matches!(kind,
                    "class_declaration" | "class_definition" | "struct_item" |
                    "impl_item" | "trait_item" | "interface_declaration" |
                    "class" | "struct" | "union" | "enum"
                )
            },
            AstContextType::Method => {
                matches!(kind,
                    "method_definition" | "method_declaration" | "function_definition" |
                    "method" | "impl_item"
                )
            },
            AstContextType::Block => {
                matches!(kind,
                    "block" | "compound_statement" | "if_statement" |
                    "for_statement" | "while_statement" | "match_expression" |
                    "switch_statement" | "try_statement"
                )
            },
            AstContextType::Module => {
                matches!(kind,
                    "module" | "namespace" | "package" | "mod_item" |
                    "namespace_definition" | "module_declaration"
                )
            },
            AstContextType::TypeDef => {
                matches!(kind,
                    "type_alias" | "typedef" | "type_definition" |
                    "enum_declaration" | "union_declaration" | "type_item"
                )
            },
        }
    }

    /// Classify a node into one of our context types.
    fn classify_node(&self, node: &Node<D>) -> Result<AstContextType, AstContextError> {
        let kind = node.kind();
        
        for context_type in &self.context_types {
            if self.node_matches_context_type(&kind, context_type) {
                return Ok(context_type.clone());
            }
        }
        
        Err(AstContextError::UnknownNodeType(kind.to_string()))
    }

    /// Extract the symbol name from a node if possible.
    fn extract_symbol_name(&self, node: &Node<D>) -> Option<String> {
        // Try to find identifier children that represent the symbol name
        for child in node.children() {
            let kind = child.kind();
            if matches!(kind.as_ref(), "identifier" | "name" | "type_identifier") {
                return Some(child.text().to_string());
            }
        }
        None
    }
}

/// Errors that can occur during AST context calculation.
#[derive(Debug, thiserror::Error)]
pub enum AstContextError {
    /// No enclosing symbol found for the given match range.
    #[error("No enclosing symbol found for match at bytes {start}-{end}. The match may be at the top level or outside any recognizable code structure.", start = .range.start, end = .range.end)]
    NoEnclosingSymbol {
        /// The range where no enclosing symbol was found
        range: std::ops::Range<usize>
    },
    
    /// Unknown AST node type encountered.
    #[error("Unknown node type: {0}")]
    UnknownNodeType(String),
    
    /// Invalid byte offset provided.
    #[error("Invalid byte offset: {0}")]
    InvalidOffset(usize),
    
    /// Unsupported programming language.
    #[error("{0}")]
    UnsupportedLanguage(String),
    
    /// AST parsing failed completely.
    #[error("Failed to parse file as {language}: {reason}")]
    ParseFailed {
        /// The language that failed to parse
        language: String,
        /// Reason for the parse failure
        reason: String,
    },
}

/// Default context types for common programming scenarios.
pub fn default_context_types() -> Vec<AstContextType> {
    vec![
        AstContextType::Function,
        AstContextType::Class,
        AstContextType::Method,
        AstContextType::Module,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_type_matching() {
        let context_types = vec![AstContextType::Function];
        let ast_grep = unsafe { std::mem::zeroed() }; // Not used in this test
        let calculator = AstContextCalculator {
            ast_grep,
            context_types,
        };

        assert!(calculator.node_matches_context_type("function_declaration", &AstContextType::Function));
        assert!(calculator.node_matches_context_type("function_definition", &AstContextType::Function));
        assert!(!calculator.node_matches_context_type("class_declaration", &AstContextType::Function));
    }
}