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

## Phase 2: LSP Integration ✅ **READY-TO-USE**

### LSP Server Status
**Goal:** Enable editor integration with existing LSP server

**✅ What's Already Available:**
The `outgrep-ast-lsp` crate provides a fully functional LSP server inherited from ast-grep with:

- **Complete LSP Backend** (`Backend<L>`) implementing `LanguageServer` trait
- **Core capabilities:**
  - Text document sync (full document updates)
  - Real-time diagnostics with AST-based pattern matching
  - Code actions (quickfix + source.fixAll)
  - Execute commands (ast-grep.applyAllFixes) 
  - Hover support with rule documentation
  - Workspace and file management
- **Multi-language support** via tree-sitter (`SupportLang`)
- **Rule-based analysis** with configurable severity and fixing
- **Working tests** validating LSP protocol compliance

**Current Architecture:**
```rust
// Already implemented in crates/ast-lsp/src/lib.rs
pub struct Backend<L: LSPLang> {
    client: Client,
    map: DashMap<String, VersionedAst<StrDoc<L>>>,
    base: PathBuf,
    rules: RuleCollection<L>,
    // ... file caching and rule management
}
```

### Integration Tasks
**What needs to be done:**

1. **Binary/Service Creation** - Create executable to run the LSP server
2. **Rule Configuration** - Connect Outgrep's pattern rules to LSP diagnostics
3. **Editor Testing** - Validate with VS Code, Neovim for basic functionality
4. **Optional Extensions:**
   - Document symbols (if needed by GraphMother)
   - Workspace symbols with semantic search integration
   - Custom protocol extensions for Outgrep-specific features

**No major development required** - the LSP infrastructure is production-ready from ast-grep.

---

## Phase 3: Enhanced Symbol Granularity

### Comment & Documentation Extraction 
**Goal:** Leverage existing AST parsing to capture comments and docs separately

**✅ What's Already Available:**
Outgrep already has sophisticated comment detection and AST parsing capabilities:

- **Tree-sitter Comment Detection**: All comment nodes identified via `node.kind().contains("comment")`
- **Syntax Highlighting**: Comment tokens already extracted with precise ranges
- **AST Structure**: `AstNodeInfo` with symbol boundaries and hierarchical structure
- **Enclosing Symbol Detection**: `--enclosing-symbol` flag provides precise symbol boundaries
- **Multi-language Support**: 21+ languages with language-specific comment patterns

**Current Infrastructure:**
```rust
// Already available in crates/core/diagnostics/types.rs
pub struct AstNodeInfo {
    pub node_type: String,           // e.g., "comment", "function_declaration"
    pub range: Range<usize>,         // Precise byte boundaries
    pub symbol_name: Option<String>, // Symbol name if applicable
    pub children: Vec<AstNodeInfo>,  // Hierarchical structure
}

pub struct SyntaxHighlightToken {
    pub range: Range<usize>,
    pub token_type: String,          // "comment", "string", "keyword", etc.
}
```

**Simple Enhancement Strategy:**
Rather than over-engineering, extend existing `extract_ast_structure()` to separate comment types:

```rust
// Simple addition to existing types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommentType {
    Line,           // // or #
    Block,          // /* */ or """ """  
    Documentation,  // /// or /** */ or """..."""
}

// Enhance existing AstNodeInfo for comment nodes
impl AstNodeInfo {
    pub fn comment_type(&self) -> Option<CommentType> {
        if !self.node_type.contains("comment") { return None; }
        // Use tree-sitter node analysis to classify comment type
        // Implementation leverages existing AST parsing
    }
}
```

**Implementation Tasks:**
1. **Extend AST extraction** to classify comment types using existing tree-sitter infrastructure
2. **Symbol-comment relationships** by traversing existing AST hierarchy
3. **Language-specific patterns** using existing `SupportLang` infrastructure

**Complexity: Low** - leverages existing parsing, no new infrastructure needed.

### Symbol-Level Change Detection
**Goal:** Build on existing FileWatcher for granular change detection

**✅ What's Already Available:**
- **FileWatcher**: Real-time file change detection
- **Symbol extraction**: Precise symbol boundaries and metadata
- **AST comparison**: Tree-sitter provides node comparison capabilities

**Simple Implementation:**
```rust
// Extend existing SymbolInfo with change detection
#[derive(Debug, Clone)]
pub struct SymbolChange {
    pub change_type: SymbolChangeType,
    pub symbol_id: String,           // Simple hash of symbol signature
    pub file_path: PathBuf,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone)]
pub enum SymbolChangeType {
    Added,
    Modified,
    Removed,
    Moved,
}
```

**Implementation Approach:**
1. **Hash symbol signatures** (name + type + basic structure) for change detection
2. **Integrate with FileWatcher** to trigger symbol re-extraction on file changes
3. **Simple diff algorithm** comparing symbol hashes before/after

**Complexity: Low** - builds on existing infrastructure, avoids complex matching algorithms.

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
