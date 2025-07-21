# Outgrep Development Guidelines

This document defines the development standards and practices for the Outgrep project - a next-generation AST-aware code search tool with semantic intelligence.

## Project Overview

Outgrep is evolving from a ripgrep-based search tool into a comprehensive streaming code intelligence platform with:

- **AST-aware search** with 20+ language support via tree-sitter
- **Semantic search** capabilities with ONNX embeddings
- **Real-time file watching** with symbol-level change detection
- **LSP integration** for editor support
- **Streaming event system** for visualization and external integrations
- **Library API** for integration into other tools

## Core Development Principles

### Code Quality Standards

**No Emojis in Code**
- Emojis are strictly prohibited in all source code, comments, documentation, and machine-readable output
- Code must remain professional and machine-parseable at all times
- Use clear, descriptive text instead of emoji symbols

**No Mock or Placeholder Implementations**
- All code must be production-ready or clearly marked as incomplete
- Use `TODO: [specific description]` comments for incomplete functionality
- Never ship silent placeholder implementations that appear to work but do nothing
- If a feature cannot be implemented immediately, document exactly what needs to be done

**No Silent Failures**
- All errors must be explicitly handled or propagated
- Use `Result<T, E>` types for fallible operations
- Prefer `unwrap()` only in tests or when failure truly represents a programming error
- Use `.expect("descriptive message")` with clear context when unwrapping is necessary
- Log meaningful error messages that help with debugging
- Never ignore errors with `let _ = ...` unless explicitly documented why

### Error Handling Strategy

```rust
// Good: Explicit error handling
match parse_config(&path) {
    Ok(config) => config,
    Err(e) => {
        log::error!("Failed to parse config at {}: {}", path.display(), e);
        return Err(ConfigError::ParseFailed(e));
    }
}

// Good: Documented unwrap with clear expectation
let home_dir = env::var("HOME").expect("HOME environment variable must be set on Unix systems");

// Bad: Silent failure
let _ = save_cache(&data);

// Bad: Undocumented unwrap
let config = parse_config(&path).unwrap();
```

## Code Organization

### Module Structure

**Use Folder Modules**
- Organize related functionality into directory-based modules
- Create `mod.rs` files primarily for visibility control and module documentation
- Keep `mod.rs` files minimal - they should contain almost no actual implementation code

**Separation of Concerns**
- Each module should have a single, well-defined responsibility
- Avoid mixing concerns like I/O, business logic, and presentation in the same module
- Use dependency injection patterns to make modules testable

### Module Documentation Standards

**mod.rs File Purpose**
Every `mod.rs` file must contain:

1. **Module overview** - What this module does and why it exists
2. **Public API summary** - What functionality it exposes
3. **Usage examples** - How to use the module's main features
4. **Architecture notes** - Key design decisions and patterns
5. **Dependencies** - What other modules/crates this depends on

Example `mod.rs` structure:
```rust
//! # Module Name
//! 
//! Brief description of what this module provides and its purpose within
//! the larger system.
//!
//! ## Functionality
//! 
//! - Primary feature 1
//! - Primary feature 2
//! - Primary feature 3
//!
//! ## Usage
//!
//! ```rust
//! use crate::module_name::PrimaryType;
//! 
//! let instance = PrimaryType::new(config)?;
//! let result = instance.primary_operation(input)?;
//! ```
//!
//! ## Architecture
//!
//! This module follows the [pattern name] pattern because [reasoning].
//! Key design decisions:
//! - Decision 1 and rationale
//! - Decision 2 and rationale
//!
//! ## Dependencies
//!
//! - `other_module`: For [specific purpose]
//! - `external_crate`: For [specific purpose]

mod submodule1;
mod submodule2;

pub use submodule1::{PublicType1, PublicFunction1};
pub use submodule2::{PublicType2};

// Only include implementation code if absolutely necessary for module coordination
```

### File Organization

**Keep Files Maintainable**
- Limit file length to ~500 lines of code (excluding tests and documentation)
- Split large modules into logical sub-modules
- Use clear, descriptive file names that reflect their purpose
- Group related functionality together

**Test Organization**
- Keep tests in separate directories where possible
- Use `tests/` directory for integration tests
- Use `#[cfg(test)] mod tests` for unit tests within modules
- Create `test_utils/` modules for shared testing infrastructure
- Name test files clearly: `test_symbol_extraction.rs`, `integration_lsp.rs`

## Documentation Standards

### Code Documentation

**Function Documentation**
Every public function must have:
- Brief description of what it does
- Parameter descriptions (using `# Arguments`)
- Return value description
- Error conditions (using `# Errors`)
- Usage examples (using `# Examples`)

```rust
/// Extracts AST symbols from a source file with enhanced granularity.
///
/// This function parses the file using tree-sitter and extracts all named
/// symbols including functions, classes, types, and their associated 
/// comments and documentation.
///
/// # Arguments
///
/// * `file_path` - Path to the source file to analyze
/// * `options` - Configuration options for symbol extraction
///
/// # Returns
///
/// Returns `Ok(SymbolCollection)` containing all extracted symbols,
/// or `Err(ExtractionError)` if parsing fails or the file is unsupported.
///
/// # Errors
///
/// * `ExtractionError::UnsupportedLanguage` - File extension not recognized
/// * `ExtractionError::ParseFailed` - Tree-sitter parsing failed
/// * `ExtractionError::IoError` - File could not be read
///
/// # Examples
///
/// ```rust
/// use outgrep::symbol_extraction::{extract_symbols, ExtractionOptions};
/// 
/// let options = ExtractionOptions::default();
/// let symbols = extract_symbols("src/main.rs", &options)?;
/// 
/// for symbol in symbols.functions() {
///     println!("Found function: {}", symbol.name);
/// }
/// ```
pub fn extract_symbols(file_path: &Path, options: &ExtractionOptions) -> Result<SymbolCollection, ExtractionError> {
    // Implementation
}
```

**Type Documentation**
All public types must document:
- Purpose and use cases
- Key methods and their behavior
- Invariants and constraints
- Relationship to other types

### Architecture Documentation

**Decision Records**
Document significant architectural decisions in `docs/architecture/`:
- `001-lsp-integration.md`
- `002-streaming-events.md`
- `003-symbol-identification.md`

**API Documentation**
Maintain up-to-date API documentation in `docs/api/`:
- Library API usage examples
- LSP protocol extensions
- Event streaming schemas

## Development Practices

### Testing Strategy

**Comprehensive Testing**
- Unit tests for all public functions
- Integration tests for major workflows
- Property-based tests for complex algorithms
- Performance regression tests for critical paths

**Test Naming**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_extraction_rust_functions() {
        // Test extracting function symbols from Rust code
    }

    #[test]
    fn test_symbol_extraction_empty_file() {
        // Test behavior with empty files
    }

    #[test]
    fn test_symbol_extraction_unsupported_language() {
        // Test error handling for unsupported file types
    }
}
```

### Performance Considerations

**Measurement-Driven Development**
- Profile before optimizing
- Include performance tests in CI
- Document performance requirements in code
- Use appropriate data structures for access patterns

**Resource Management**
- Prefer streaming over loading entire datasets into memory
- Use bounded channels for inter-thread communication
- Implement proper cleanup in Drop implementations
- Monitor memory usage in long-running operations

### Dependencies

**Dependency Hygiene**
- Justify each dependency in commit messages
- Prefer standard library solutions when performance adequate
- Pin major version updates and test thoroughly
- Document why each dependency was chosen

**No Feature Flags Policy**
- **All functionality is included by default** - nothing is hidden behind feature flags
- Every feature we implement is part of the core package and experience
- This includes AST analysis, semantic search, LSP integration, streaming events, and all planned capabilities
- Feature flags may only be introduced after explicit discussion and advance planning
- The goal is a comprehensive, batteries-included code intelligence platform

## Language-Specific Guidelines

### Rust Best Practices

**Error Types**
- Use `thiserror` for custom error types
- Implement `From` traits for error conversion
- Provide context with error messages

**Async Code**
- Use `tokio` as the async runtime
- Prefer `async fn` over `impl Future`
- Handle cancellation gracefully with `tokio::select!`

**Memory Safety**
- Minimize `unsafe` code
- Document all `unsafe` blocks with safety invariants
- Use `Arc<Mutex<T>>` sparingly - prefer message passing

### API Design

**Builder Patterns**
Use builder patterns for complex configuration:
```rust
let config = OutgrepConfig::builder()
    .enable_semantic_search(true)
    .lsp_port(3000)
    .max_file_size(1024 * 1024)
    .build()?;
```

**Streaming APIs**
Prefer streaming interfaces for large datasets:
```rust
pub fn watch_changes(&self) -> impl Stream<Item = AnalysisEvent> + Send {
    // Return stream of events
}
```

## CI/CD Requirements

### Automated Checks

**Required Checks**
- `cargo test` - All tests must pass
- `cargo clippy -- -D warnings` - No clippy warnings
- `cargo fmt --check` - Code must be formatted
- `cargo audit` - No known security vulnerabilities
- Documentation builds without warnings

**Performance Monitoring**
- Benchmark regression tests
- Memory usage monitoring
- Integration test timing

### Release Process

**Version Management**
- Follow semantic versioning strictly
- Update CHANGELOG.md for all releases
- Tag releases with descriptive commit messages

## Security Considerations

**Input Validation**
- Validate all external inputs
- Sanitize file paths to prevent directory traversal
- Limit resource usage (memory, CPU, file handles)

**Dependency Security**
- Regular `cargo audit` runs
- Pin dependencies and review updates
- Minimize attack surface through feature flags

This document is living and should be updated as the project evolves. All team members are expected to follow these guidelines and suggest improvements through pull requests.