use super::types::{Embedding, EmbeddingPoint, SemanticIndex, SemanticMatch, SemanticConfig};
use instant_distance::{Builder, Search};
use std::ops::Range;

/// Build index from embeddings and their associated data
pub fn build_index(
    embeddings: Vec<(Embedding, Range<usize>, String)>,
    config: &SemanticConfig,
) -> SemanticIndex {
    let mut embedding_vectors = Vec::new();
    let mut metadata = Vec::new();
    let mut points = Vec::new();
    let mut values = Vec::new();

    for (idx, (embedding, range, content)) in
        embeddings.into_iter().enumerate()
    {
        // Ensure embedding matches configured dimensions
        let mut vector = embedding.vector.clone();
        if vector.len() != config.embedding_dimensions {
            eprintln!(
                "DEBUG: Resizing vector from {} to {} dimensions",
                vector.len(),
                config.embedding_dimensions
            );
        }
        vector.resize(config.embedding_dimensions, 0.0);

        // Add point and its index value
        points.push(EmbeddingPoint(vector));
        values.push(idx);

        embedding_vectors.push(embedding);
        metadata.push(SemanticMatch {
            similarity: 1.0,
            byte_range: range,
            content,
        });
    }

    // Build the HNSW map
    let hnsw_map = Builder::default().build(points, values);
    let search = Search::default();

    SemanticIndex { hnsw_map, search, embeddings: embedding_vectors, metadata }
}

/// Add new embedding to existing index
pub fn add_to_index(
    index: &mut SemanticIndex,
    embedding: Embedding,
    range: Range<usize>,
    content: String,
) {
    // Note: instant-distance doesn't support dynamic insertion easily
    // For now, just add to the data structures without rebuilding the index
    index.embeddings.push(embedding);
    index.metadata.push(SemanticMatch {
        similarity: 1.0,
        byte_range: range,
        content,
    });
}
