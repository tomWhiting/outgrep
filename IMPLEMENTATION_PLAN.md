# Outgrep Evolution Design Outline

## Project Vision
Transform Outgrep from a ripgrep-based search tool into a next-generation streaming code intelligence platform with LSP integration, semantic search capabilities, and real-time symbol-level analysis.

## Architecture Overview

### Current State
- ✅ **AST-aware search** with 20+ language support via tree-sitter
- ✅ **Semantic search** infrastructure with ONNX embeddings
- ✅ **Real-time file watching** with change detection
- ✅ **Comprehensive JSON output** with tree structure
- ✅ **Symbol extraction** with precise boundary detection
- ✅ **Git integration** for diff analysis and change tracking

### Target State
- 🎯 **Library API** exposing all CLI functionality
- 🎯 **LSP Integration** for editor support with custom protocol extensions
- 🎯 **Streaming Events** for symbol-level change detection
- 🎯 **Multi-destination streaming** (visualization, Memgraph, embeddings service)
- 🎯 **Enhanced symbol granularity** with comment/doc separation

## Implementation Phases

---

## Phase 1: Foundation & Library API

### Library API Design & Implementation
**Goal:** Expose all CLI functionality as a clean library API

**API Surface Design:**
```rust
// Core library interface
pub struct OutgrepEngine {
    config: OutgrepConfig,
}

impl OutgrepEngine {
    pub fn new(config: OutgrepConfig) -> Result<Self>;

    // Search operations
    pub fn search(&self, query: SearchQuery) -> Result<SearchResults>;
    pub fn search_streaming(&self, query: SearchQuery) -> impl Stream<Item = SearchResult>;

    // Analysis operations
    pub fn analyze_file(&self, path: &Path) -> Result<FileAnalysis>;
    pub fn analyze_directory(&self, path: &Path) -> Result<DirectoryAnalysis>;
    pub fn watch_directory(&self, path: &Path) -> impl Stream<Item = FileChangeEvent>;

    // Tree and symbol operations
    pub fn build_tree(&self, path: &Path) -> Result<ProjectTree>;
    pub fn extract_symbols(&self, path: &Path) -> Result<SymbolCollection>;
}

// Configuration
#[derive(Debug, Clone)]
pub struct OutgrepConfig {
    pub enable_semantic_search: bool,
    pub enable_git_integration: bool,
    pub enable_diagnostics: bool,
    pub output_format: OutputFormat,
    // Mirror CLI flag options
}

// Error handling strategy
#[derive(Debug, thiserror::Error)]
pub enum OutgrepError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Language not supported: {0}")]
    UnsupportedLanguage(String),
}
```

The library API serves as the foundation for all external integrations, providing a clean interface to Outgrep's capabilities without requiring command-line invocation. This design prioritizes ease of use while maintaining the full power of the CLI tool.

The API design follows Rust conventions with builder patterns for configuration and streaming interfaces for real-time data. Error handling distinguishes between recoverable and unrecoverable errors, providing meaningful context for library consumers. All CLI functionality must be accessible through the library to support integration into IDEs, build systems, and other tools.

**Implementation Steps:**
- Create `crates/outgrep-lib/` crate with clean public API
- Extract core functionality from `main.rs` into reusable library modules
- Design error handling strategy appropriate for library contexts
- Implement builder pattern for flexible configuration
- Add comprehensive integration tests covering all API surfaces
- Document API usage patterns and integration examples

### Terminology Cleanup
**Goal:** Separate compiler diagnostics from code metrics

Clear terminology prevents confusion between actual compiler diagnostics (errors, warnings) and code analysis metrics (lines of code, complexity). This separation makes the codebase more intuitive and allows for targeted development of each capability.

**Changes:**
- Rename `diagnostics/metrics.rs` to `analysis/metrics.rs` to clarify purpose
- Keep `diagnostics/compiler.rs` focused on actual compiler errors and warnings
- Update module structure to reflect the distinction between analysis and diagnostics
- Update documentation and comments to use consistent terminology

---

## Phase 2: LSP Integration

### LSP Server Foundation
**Goal:** Integrate tower-lsp server with existing AST infrastructure

The LSP integration transforms Outgrep from a command-line tool into a first-class editor citizen. By implementing the Language Server Protocol, we enable real-time code intelligence directly within developers' editing environments. This approach leverages ast-grep's proven LSP architecture while extending it with Outgrep's unique semantic capabilities.

The design prioritizes editor compatibility while providing custom extensions for advanced features like semantic search and real-time analysis. Standard LSP capabilities include document symbols, workspace symbols, diagnostics, and hover information, while custom extensions enable features like project-wide semantic search and live code intelligence.

**New Crate Structure:**
```
crates/outgrep-lsp/
├── src/
│   ├── lib.rs           # Main LSP backend
│   ├── server.rs        # Server setup and initialization
│   ├── handlers.rs      # LSP message handlers
│   ├── diagnostics.rs   # Convert outgrep diagnostics to LSP format
│   ├── symbols.rs       # Document symbols and workspace symbols
│   └── utils.rs         # Conversion utilities
└── tests/
    └── integration.rs   # LSP protocol tests
```

**Core Implementation:**
```rust
pub struct OutgrepLspBackend {
    client: Client,
    workspace_root: PathBuf,
    file_cache: DashMap<Uri, VersionedDocument>,
    outgrep_engine: OutgrepEngine,
}

impl LanguageServer for OutgrepLspBackend {
    // Standard LSP capabilities
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult>;
    async fn did_open(&self, params: DidOpenTextDocumentParams);
    async fn did_change(&self, params: DidChangeTextDocumentParams);
    async fn document_symbol(&self, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>>;

    // Custom extensions for semantic search
    async fn workspace_symbol(&self, params: WorkspaceSymbolParams) -> Result<Option<Vec<SymbolInformation>>>;
}
```

**LSP Capabilities:**
- Document symbols (functions, classes, types, modules)
- Workspace symbols with semantic search
- Diagnostics (compiler errors/warnings)
- Code actions (basic AST-based fixes)
- Hover information with symbol details

**Integration Points:**
- Use existing `extract_symbols()` for document symbols
- Leverage `CompilerDiagnosticsRunner` for diagnostics
- Bridge semantic search for workspace symbols

### Editor Integration Testing
**Goal:** Validate LSP server with major editors

Editor integration testing ensures broad compatibility across the development ecosystem. The testing approach validates both standard LSP functionality and custom extensions, ensuring a consistent experience regardless of editor choice. This validation includes testing protocol compliance, performance characteristics, and feature completeness.

Primary testing targets include VS Code for mainstream adoption, Neovim for terminal-based development, and Emacs for advanced users. Each editor requires specific configuration patterns and may expose different edge cases in the LSP implementation. Custom protocol extensions enable advanced features while maintaining fallback compatibility for editors that don't support them.

---

## Phase 3: Enhanced Symbol Granularity

### Comment & Documentation Extraction
**Goal:** Extend symbol extraction to capture comments and docs separately

Enhanced symbol granularity provides the detailed code understanding necessary for advanced analysis and visualization. By separately capturing comments, documentation, and code structure, we enable rich representations of code that preserve both implementation and intent. This granular approach supports sophisticated analysis patterns while maintaining the performance characteristics required for real-time operation.

The extraction process must handle diverse language conventions for documentation, from JSDoc and Rust doc comments to Python docstrings and inline comments. This language-aware approach ensures consistent representation across the entire codebase while respecting each language's documentation idioms.

**Enhancement to `extract_symbols()`:**
```rust
#[derive(Debug, Clone, Serialize)]
pub struct EnhancedSymbolInfo {
    pub name: String,
    pub symbol_type: String,
    pub range: Range<usize>,
    pub location: SymbolLocation,

    // Enhanced granularity
    pub comments: Vec<CommentInfo>,
    pub documentation: Vec<DocInfo>,
    pub attributes: Vec<AttributeInfo>,
    pub body_range: Option<Range<usize>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommentInfo {
    pub comment_type: CommentType, // Line, Block, Doc
    pub content: String,
    pub range: Range<usize>,
    pub location: SymbolLocation,
}

#[derive(Debug, Clone, Serialize)]
pub enum CommentType {
    Line,           // // or #
    Block,          // /* */ or """ """
    Documentation,  // /// or /** */ or """..."""
}
```

**Language-Specific Comment Patterns:**
- **Rust:** `//`, `///`, `//!`, `/* */`, `/** */`
- **Python:** `#`, `"""docstring"""`, `'''docstring'''`
- **JavaScript/TypeScript:** `//`, `/* */`, `/** JSDoc */`
- **Go:** `//`, `/* */`, `// Package doc`
- **Java:** `//`, `/* */`, `/** JavaDoc */`

**Implementation Strategy:**
The implementation extends the existing AST traversal to identify and classify comment nodes according to their type and purpose. Comment classification distinguishes between inline comments, block comments, and formal documentation, enabling appropriate handling for each type. Symbol-to-comment relationships preserve the semantic connections between code and its documentation, supporting intelligent analysis and presentation.

Language-specific handling ensures that each language's documentation conventions are properly recognized and preserved. This includes handling complex cases like nested comments, documentation inheritance, and cross-reference patterns that vary significantly between programming languages.

### Symbol-Level Change Detection
**Goal:** Detect changes at individual symbol granularity

Symbol-level change detection enables precise tracking of code modifications, supporting real-time analysis and intelligent caching strategies. Rather than invalidating entire files on change, this approach identifies exactly which symbols have been modified, allowing for targeted re-analysis and efficient update propagation.

The detection mechanism must balance precision with performance, identifying meaningful changes while avoiding noise from whitespace or comment modifications. This granular approach enables sophisticated features like dependency impact analysis and selective re-computation of semantic relationships.

**Change Detection Strategy:**
```rust
#[derive(Debug, Clone)]
pub struct SymbolChange {
    pub change_type: SymbolChangeType,
    pub symbol: EnhancedSymbolInfo,
    pub file_path: PathBuf,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone)]
pub enum SymbolChangeType {
    Added,
    Modified { old_symbol: EnhancedSymbolInfo },
    Removed { old_symbol: EnhancedSymbolInfo },
    Moved { old_location: SymbolLocation },
}
```

**Implementation Approach:**
The approach maintains an in-memory symbol index for each file, enabling efficient comparison between current and previous states. When file changes occur, the system re-extracts symbols and performs intelligent matching based on symbol signatures and locations. This matching process must handle common refactoring patterns like symbol moves and renames while maintaining accuracy.

Performance optimization focuses on incremental processing and intelligent debouncing to handle rapid editing scenarios. Background processing ensures that change detection doesn't interfere with editor responsiveness, while caching strategies minimize redundant computation during active development sessions.

---

## Phase 4: Streaming Event System

### Event Schema & Infrastructure
**Goal:** Design unified event system for real-time streaming

The streaming event system provides the foundation for real-time code intelligence by delivering granular change notifications to multiple consumers. This design prioritizes simplicity and flexibility, enabling easy extension while maintaining performance and reliability. The event-driven architecture supports diverse use cases from real-time visualization to persistent graph storage.

The simplified schema uses a flexible, action-based approach that scales from basic file operations to complex relationship tracking. This design avoids over-engineering while providing the extensibility needed for future capabilities. Event routing and destination management ensure reliable delivery while supporting different consumer requirements.

**Core Event Schema:**
```rust
#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub event_type: EventType,
    pub action: EventAction,
    pub timestamp: SystemTime,
    pub payload: EventPayload,
    pub destinations: Vec<EventDestination>,
}

#[derive(Debug, Clone, Serialize)]
pub enum EventType {
    File,
    Symbol,
    Relationship,
    Diagnostic,  // ephemeral
    System,      // system status/logging
}

#[derive(Debug, Clone, Serialize)]
pub enum EventAction {
    Added,
    Modified,
    Removed,
}

#[derive(Debug, Clone, Serialize)]
pub enum EventDestination {
    Visualization,
    Memgraph,
    Embeddings,
    All,  // broadcast to all destinations
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum EventPayload {
    File(FileEventData),
    Symbol(SymbolEventData),
    Relationship(RelationshipEventData),
    Diagnostic(DiagnosticEventData),
    System(SystemEventData),
}
```

**Event Infrastructure:**
The event infrastructure manages reliable delivery to multiple destinations while handling failures gracefully. Event routing uses destination specifications to control which consumers receive specific events, enabling efficient resource usage and targeted updates. Buffer management and retry logic ensure reliability while maintaining real-time characteristics for interactive use cases.

### Multi-Destination Streaming
**Goal:** Reliable event delivery to multiple consumers

Multi-destination streaming enables the same event data to flow to different consumers with varying requirements and reliability characteristics. The visualization layer requires real-time delivery for interactive responsiveness, while Memgraph benefits from batched operations for efficiency. The embeddings service operates asynchronously, allowing for queuing and background processing.

Error handling strategies account for the different failure modes and recovery requirements of each destination. Circuit breaker patterns prevent cascading failures, while replay capabilities ensure data consistency when destinations recover. System events provide observability into the streaming infrastructure itself, enabling monitoring and debugging of the event flow.

**Streaming Architecture:**
The architecture separates concerns between event generation, routing, and delivery. Event generators focus on detecting and creating events, while the routing layer handles destination selection and delivery strategies. Individual sinks implement destination-specific protocols and error handling, enabling independent evolution of each integration point.

---

## Phase 5: External Integrations

### Memgraph Integration
**Goal:** Persistent graph storage for project state

Memgraph integration provides persistent storage for the code intelligence graph, enabling fast startup and historical analysis capabilities. The graph database naturally represents the relationships between files, symbols, and dependencies, supporting complex queries that would be difficult with traditional storage approaches. This integration enables the visualization layer to load quickly while maintaining consistency with real-time updates.

**Data Model:**
```cypher
// Nodes
CREATE (f:File {path: $path, last_modified: $timestamp})
CREATE (s:Symbol {name: $name, type: $type, file_path: $path, range: $range})
CREATE (c:Comment {content: $content, type: $type, file_path: $path})

// Relationships
CREATE (s)-[:DEFINED_IN]->(f)
CREATE (c)-[:DOCUMENTS]->(s)
CREATE (s1)-[:CALLS]->(s2)
CREATE (s1)-[:DEPENDS_ON]->(s2)
```

**Integration Approach:**
The integration strategy balances initial load performance with real-time update efficiency. On startup, the system queries existing state from Memgraph to populate the in-memory representation, enabling fast subsequent operations. Incremental updates flow through the event stream, maintaining graph consistency without requiring full re-scanning. Conflict resolution uses timestamps and event ordering to handle concurrent modifications reliably.

### Python Embeddings Service Integration
**Goal:** Offload semantic processing to dedicated service

The Python embeddings service integration enables sophisticated semantic search capabilities without impacting the core Rust performance characteristics. By offloading computationally intensive embedding generation to a dedicated service, the main application maintains its responsiveness while supporting advanced AI-powered features. This separation of concerns enables independent scaling and optimization of each component.

**Service Interface:**
```python
# Python service endpoints
POST /embeddings/generate
{
    "symbols": [{"name": "func", "content": "...", "context": "..."}],
    "model": "code-embedding-model"
}

POST /embeddings/search
{
    "query": "authentication logic",
    "threshold": 0.8,
    "limit": 50
}
```

**Integration Pattern:**
The integration uses asynchronous patterns to avoid blocking the main application flow. Background queuing ensures that embedding generation doesn't interfere with interactive operations, while async HTTP clients maintain non-blocking characteristics. Fallback strategies provide graceful degradation when the embeddings service is unavailable, ensuring core functionality remains accessible.

---

## Cross-Cutting Concerns

### Performance Requirements
Performance requirements ensure that Outgrep maintains its responsiveness characteristics while adding sophisticated analysis capabilities. Symbol change detection must remain fast enough for real-time editing scenarios, while event streaming provides near-instantaneous updates for interactive visualizations. LSP response times directly impact developer productivity, requiring optimization of the most frequently used operations.

Memory usage targets balance comprehensive analysis with practical resource constraints. The baseline memory requirement supports the core functionality, while the per-symbol scaling factor ensures predictable resource usage for projects of varying sizes. These targets guide architectural decisions throughout the implementation.

### Extensibility Considerations
Extensibility design ensures that Outgrep can evolve to meet future requirements without fundamental architectural changes. The event-driven architecture naturally supports plugin development, enabling custom analysis capabilities through well-defined interfaces. Language support extensibility leverages the tree-sitter ecosystem, making new language addition straightforward and consistent.

Protocol extensions enable domain-specific features while maintaining compatibility with standard tooling. This approach supports advanced capabilities like semantic search and real-time analysis while ensuring broad editor compatibility through graceful fallbacks.

### Testing Strategy
The testing strategy validates both individual components and system-wide behavior across multiple integration points. Unit tests ensure correctness of core algorithms, while integration tests validate complex interactions between components. Performance testing prevents regressions in the characteristics that make Outgrep valuable for large-scale development.

End-to-end testing with real editors ensures practical usability and catches issues that might not surface in isolated component testing. This comprehensive approach builds confidence in the system's reliability and performance characteristics.

### Documentation Plan
Documentation serves multiple audiences, from library integrators to end users configuring editors. API documentation enables developers to integrate Outgrep capabilities into their own tools, while integration guides support practical deployment scenarios. Architecture documentation preserves design decisions and rationale for future development.

The documentation strategy emphasizes practical examples and real-world usage patterns, ensuring that the sophisticated capabilities remain accessible to developers with varying levels of familiarity with the underlying technologies.

---

## Risk Mitigation

### High-Risk Areas
1. **Symbol-level change detection performance** - Implement incremental parsing, debouncing
2. **LSP protocol edge cases** - Extensive testing with multiple editors
3. **Multi-destination streaming reliability** - Circuit breakers, retry logic, monitoring

### Rollback Strategies
Rollback strategies ensure that development progress doesn't compromise existing functionality or user workflows. CLI compatibility preservation enables existing users to continue their workflows while new capabilities are developed and tested. This compatibility extends to output formats and command-line interface design, ensuring seamless transitions.

Graceful degradation patterns ensure that component failures don't cascade throughout the system. When advanced features like semantic search or LSP integration encounter issues, the core search and analysis capabilities remain fully functional. This approach maintains reliability while enabling aggressive development of advanced features.

### Success Metrics
Success metrics provide objective measures of the implementation's effectiveness and adoption. Performance metrics ensure that advanced capabilities don't compromise the speed characteristics that make Outgrep valuable for daily development workflows. Reliability metrics validate the production-readiness of server components that developers depend on continuously.

Adoption metrics demonstrate practical value through successful integration across diverse development environments. Extensibility metrics validate the architectural decisions by measuring community engagement and contribution patterns. These metrics guide both development priorities and architectural refinements.
