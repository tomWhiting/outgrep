use std::ops::Range;
use instant_distance::{HnswMap, Point, Search};

/// Wrapper around Vec<f32> that implements Point trait
#[derive(Debug, Clone)]
pub struct EmbeddingPoint(pub Vec<f32>);

impl Point for EmbeddingPoint {
    fn distance(&self, other: &Self) -> f32 {
        // Euclidean distance
        self.0.iter()
            .zip(other.0.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            .sqrt()
    }
}

/// Vector embedding representation
#[derive(Debug, Clone)]
pub struct Embedding {
    /// The embedding vector components
    pub vector: Vec<f32>,
    /// Number of dimensions in the vector
    pub dimensions: usize,
}

/// Search result with similarity score
#[derive(Debug, Clone)]
pub struct SemanticMatch {
    /// Similarity score between 0.0 and 1.0
    pub similarity: f32,
    /// Byte range in the source text
    pub byte_range: Range<usize>,
    /// The matched content
    pub content: String,
}

/// Configuration for semantic search
#[derive(Debug, Clone)]
pub struct SemanticConfig {
    /// Minimum similarity score to include in results
    pub similarity_threshold: f32,
    /// Maximum number of results to return
    pub max_results: usize,
    /// Number of dimensions in embeddings
    pub embedding_dimensions: usize,
}

/// Index for fast vector similarity search
pub struct SemanticIndex {
    /// Instant-distance HNSW map for efficient nearest neighbor search
    pub hnsw_map: HnswMap<EmbeddingPoint, usize>,
    /// Search helper
    pub search: Search,
    /// Embeddings for direct similarity calculation
    pub embeddings: Vec<Embedding>,
    /// Metadata for each indexed item
    pub metadata: Vec<SemanticMatch>,
}

impl Default for SemanticConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.2,  // 20% similarity threshold
            max_results: 10,
            embedding_dimensions: 384,
        }
    }
}