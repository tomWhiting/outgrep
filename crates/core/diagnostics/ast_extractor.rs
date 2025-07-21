/*!
AST extraction for syntax tree analysis and JSON output.

This module extracts Abstract Syntax Tree information from source files
using the existing ast-grep infrastructure and formats it for JSON output.
*/

use std::path::Path;
use std::fs;

use outgrep_ast_core::{Node, Doc, Language};
use outgrep_ast_core::tree_sitter::LanguageExt;
use outgrep_ast_language::SupportLang;

use crate::diagnostics::types::{
    AstStructure, AstNodeInfo, SyntaxHighlightToken, AstSymbolSummary, SymbolInfo
};

/// Extract AST structure from a source file.
pub fn extract_ast_structure(file_path: &Path) -> Option<AstStructure> {
    // Check if file is supported for AST parsing
    let language = SupportLang::from_path(file_path)?;
    
    // Read file content
    let content = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(_) => return None,
    };

    // Skip empty files
    if content.trim().is_empty() {
        return None;
    }

    // Create AST directly using the language implementation and extract info immediately
    extract_ast_info_for_language(language, &content)
}

/// Extract AST information for a specific language and content.
fn extract_ast_info_for_language(language: SupportLang, content: &str) -> Option<AstStructure> {
    macro_rules! extract_ast {
        ($lang_impl:expr) => {{
            // Try to parse the source with ast-grep
            let ast_grep = $lang_impl.ast_grep(content);
            
            // Check if parsing actually succeeded by trying to get the root
            let root = ast_grep.root();
            if root.range().start == 0 && root.range().end == 0 && !content.is_empty() {
                return None; // Parsing failed
            }
            
            // Extract basic syntax highlighting (simplified)
            let syntax_tokens = extract_syntax_tokens(&root);
            
            // Extract root AST nodes (with depth limit to avoid huge structures)
            let root_nodes = if let Some(node_info) = extract_node_info(&root, 3, 0) {
                vec![node_info]
            } else {
                Vec::new()
            };
            
            // Extract symbol information
            let symbols = extract_symbols(&root);

            Some(AstStructure {
                language: format!("{:?}", language), // Use Debug format for now
                root_nodes,
                syntax_tokens,
                symbols,
            })
        }};
    }

    match language {
        SupportLang::Rust => extract_ast!(outgrep_ast_language::Rust),
        SupportLang::JavaScript => extract_ast!(outgrep_ast_language::JavaScript),
        SupportLang::TypeScript => extract_ast!(outgrep_ast_language::TypeScript),
        SupportLang::Python => extract_ast!(outgrep_ast_language::Python),
        SupportLang::Go => extract_ast!(outgrep_ast_language::Go),
        SupportLang::Java => extract_ast!(outgrep_ast_language::Java),
        SupportLang::C => extract_ast!(outgrep_ast_language::C),
        SupportLang::Cpp => extract_ast!(outgrep_ast_language::Cpp),
        SupportLang::CSharp => extract_ast!(outgrep_ast_language::CSharp),
        SupportLang::Ruby => extract_ast!(outgrep_ast_language::Ruby),
        SupportLang::Php => extract_ast!(outgrep_ast_language::Php),
        SupportLang::Swift => extract_ast!(outgrep_ast_language::Swift),
        SupportLang::Kotlin => extract_ast!(outgrep_ast_language::Kotlin),
        SupportLang::Scala => extract_ast!(outgrep_ast_language::Scala),
        SupportLang::Haskell => extract_ast!(outgrep_ast_language::Haskell),
        SupportLang::Elixir => extract_ast!(outgrep_ast_language::Elixir),
        SupportLang::Lua => extract_ast!(outgrep_ast_language::Lua),
        SupportLang::Bash => extract_ast!(outgrep_ast_language::Bash),
        SupportLang::Html => extract_ast!(outgrep_ast_language::Html),
        SupportLang::Css => extract_ast!(outgrep_ast_language::Css),
        SupportLang::Json => extract_ast!(outgrep_ast_language::Json),
        SupportLang::Yaml => extract_ast!(outgrep_ast_language::Yaml),
        SupportLang::Tsx => extract_ast!(outgrep_ast_language::Tsx),
    }
}

/// Extract syntax highlighting tokens from the AST.
fn extract_syntax_tokens<D: Doc>(root: &Node<D>) -> Vec<SyntaxHighlightToken> {
    let mut tokens = Vec::new();
    let content = root.text();
    
    // Define keywords for syntax highlighting
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
        "function", "var", "const", "null", "undefined", "this",
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

                // Check for overlaps with existing tokens
                let overlaps = tokens.iter().any(|existing: &SyntaxHighlightToken| {
                    range.start < existing.range.end && existing.range.start < range.end
                });

                if !overlaps {
                    tokens.push(SyntaxHighlightToken {
                        range,
                        token_type: "keyword".to_string(),
                    });
                }
            }

            start = abs_pos + 1;
        }
    }

    // Find string literals
    let string_patterns = ['"', '\''];
    for quote in string_patterns.iter() {
        let mut start = 0;
        while let Some(start_pos) = content[start..].find(*quote) {
            let abs_start = start + start_pos;
            if let Some(end_pos) = content[abs_start + 1..].find(*quote) {
                let abs_end = abs_start + 1 + end_pos + 1;
                let range = abs_start..abs_end;

                let overlaps = tokens.iter().any(|existing: &SyntaxHighlightToken| {
                    range.start < existing.range.end && existing.range.start < range.end
                });

                if !overlaps {
                    tokens.push(SyntaxHighlightToken {
                        range,
                        token_type: "string".to_string(),
                    });
                }

                start = abs_end;
            } else {
                break;
            }
        }
    }

    // Find comments  
    let comment_patterns = ["//", "#"];
    for comment_start in comment_patterns.iter() {
        let mut start = 0;
        while let Some(pos) = content[start..].find(comment_start) {
            let abs_pos = start + pos;
            if let Some(end_pos) = content[abs_pos..].find('\n') {
                let abs_end = abs_pos + end_pos;
                let range = abs_pos..abs_end;

                let overlaps = tokens.iter().any(|existing: &SyntaxHighlightToken| {
                    range.start < existing.range.end && existing.range.start < range.end
                });

                if !overlaps {
                    tokens.push(SyntaxHighlightToken {
                        range,
                        token_type: "comment".to_string(),
                    });
                }

                start = abs_end;
            } else {
                // Comment to end of file
                let range = abs_pos..content.len();
                let overlaps = tokens.iter().any(|existing: &SyntaxHighlightToken| {
                    range.start < existing.range.end && existing.range.start < range.end
                });

                if !overlaps {
                    tokens.push(SyntaxHighlightToken {
                        range,
                        token_type: "comment".to_string(),
                    });
                }
                break;
            }
        }
    }

    // Sort by start position
    tokens.sort_by_key(|token| token.range.start);
    tokens
}

/// Extract detailed AST node information recursively.
fn extract_node_info<D: Doc>(node: &Node<D>, max_depth: usize, current_depth: usize) -> Option<AstNodeInfo> {
    if current_depth >= max_depth {
        return None;
    }

    let node_type = node.kind().to_string();
    let range = node.range();
    let start_pos = node.start_pos();
    let end_pos = node.end_pos();

    // Extract symbol name for named nodes
    let symbol_name = extract_symbol_name(node);

    // Recursively extract children (with depth limit)
    let children = if current_depth < max_depth - 1 {
        node.children()
            .filter_map(|child| extract_node_info(&child, max_depth, current_depth + 1))
            .collect()
    } else {
        Vec::new()
    };

    Some(AstNodeInfo {
        node_type,
        range,
        start_line: start_pos.line() as u32,
        start_column: start_pos.column(node) as u32,
        end_line: end_pos.line() as u32,
        end_column: end_pos.column(node) as u32,
        symbol_name,
        children,
    })
}

/// Extract symbol name from an AST node if it represents a named entity.
fn extract_symbol_name<D: Doc>(node: &Node<D>) -> Option<String> {
    let kind = node.kind();
    
    // For named entities, try to find identifier children
    if is_named_entity(&kind) {
        for child in node.children() {
            let child_kind = child.kind();
            if matches!(child_kind.as_ref(), "identifier" | "name" | "type_identifier") {
                return Some(child.text().to_string());
            }
        }
    }
    
    None
}

/// Check if a node type represents a named entity we're interested in.
fn is_named_entity(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function_definition"
            | "function_item"
            | "method_definition"
            | "class_declaration"
            | "class_definition"
            | "struct_item"
            | "impl_item"
            | "trait_item"
            | "interface_declaration"
            | "type_alias"
            | "typedef"
            | "type_definition"
            | "enum_declaration"
            | "union_declaration"
            | "type_item"
            | "module"
            | "namespace"
            | "mod_item"
    )
}

/// Extract symbol information for the symbol summary.
fn extract_symbols<D: Doc>(node: &Node<D>) -> AstSymbolSummary {
    let mut symbols = AstSymbolSummary::default();
    
    // Traverse the AST and collect symbols
    for ast_node in node.dfs() {
        let kind = ast_node.kind();
        let range = ast_node.range();
        let start_pos = ast_node.start_pos();
        
        if let Some(name) = extract_symbol_name(&ast_node) {
            let symbol_info = SymbolInfo {
                name,
                symbol_type: kind.to_string(),
                range,
                line: (start_pos.line() + 1) as u32, // 1-based line numbers
                column: (start_pos.column(&ast_node) + 1) as u32, // 1-based column numbers
            };
            
            // Categorize symbol by type
            match kind.as_ref() {
                "function_declaration" | "function_definition" | "function_item" | "method_definition" => {
                    symbols.functions.push(symbol_info);
                }
                "class_declaration" | "class_definition" | "struct_item" | "trait_item" | "interface_declaration" => {
                    symbols.classes.push(symbol_info);
                }
                "type_alias" | "typedef" | "type_definition" | "enum_declaration" | "union_declaration" | "type_item" => {
                    symbols.types.push(symbol_info);
                }
                "module" | "namespace" | "mod_item" => {
                    symbols.modules.push(symbol_info);
                }
                _ => {}
            }
        }
    }
    
    symbols
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_ast_extraction_rust_file() {
        // Test with a simple Rust file
        let temp_file = std::env::temp_dir().join("test.rs");
        std::fs::write(&temp_file, "fn main() { println!(\"Hello\"); }").unwrap();
        
        let ast_structure = extract_ast_structure(&temp_file);
        
        // Should succeed for Rust files
        assert!(ast_structure.is_some());
        
        if let Some(ast) = ast_structure {
            assert_eq!(ast.language, "Rust");
            assert!(!ast.syntax_tokens.is_empty());
        }
        
        // Clean up
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_ast_extraction_unsupported_file() {
        let temp_file = std::env::temp_dir().join("test.unknown");
        std::fs::write(&temp_file, "some content").unwrap();
        
        let ast_structure = extract_ast_structure(&temp_file);
        
        // Should fail for unsupported files
        assert!(ast_structure.is_none());
        
        // Clean up
        let _ = std::fs::remove_file(&temp_file);
    }
}