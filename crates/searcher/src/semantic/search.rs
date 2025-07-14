use super::types::{SemanticConfig, SemanticIndex, SemanticMatch, EmbeddingPoint};
use super::embedding::generate_embedding;

/// Perform semantic search against an index
pub fn search_semantic(
    query: &str,
    index: &mut SemanticIndex,
    config: &SemanticConfig,
) -> Vec<SemanticMatch> {
    let query_embedding = generate_embedding(query, config);
    
    // Ensure query embedding is exactly 384 dimensions
    let mut query_vector = query_embedding.vector.clone();
    if query_vector.len() != 384 {
        eprintln!("DEBUG: Resizing query vector from {} to 384 dimensions", query_vector.len());
    }
    query_vector.resize(384, 0.0);
    
    // Search for nearest neighbors
    let query_point = EmbeddingPoint(query_vector);
    let nearest = index.hnsw_map.search(&query_point, &mut index.search);
    
    let mut similarities = Vec::new();
    
    for neighbor in nearest.take(config.max_results * 2) {
        let idx = *neighbor.value;
        if let Some(stored_embedding) = index.embeddings.get(idx) {
            // Calculate proper cosine similarity
            let dot_product: f32 = query_embedding.vector.iter()
                .zip(stored_embedding.vector.iter())
                .map(|(a, b)| a * b)
                .sum();
            
            let query_magnitude: f32 = query_embedding.vector.iter()
                .map(|x| x * x)
                .sum::<f32>()
                .sqrt();
            
            let stored_magnitude: f32 = stored_embedding.vector.iter()
                .map(|x| x * x)
                .sum::<f32>()
                .sqrt();
            
            let similarity = if query_magnitude > 0.0 && stored_magnitude > 0.0 {
                dot_product / (query_magnitude * stored_magnitude)
            } else {
                0.0
            };
            
            // Clamp to [-1, 1] range for safety
            let similarity = similarity.max(-1.0).min(1.0);
            similarities.push((idx, similarity));
        }
    }
    
    // Sort by similarity (descending) and take top results
    similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    similarities.truncate(config.max_results);
    
    similarities
        .into_iter()
        .filter_map(|(idx, similarity)| {
            if similarity >= config.similarity_threshold {
                index.metadata.get(idx).map(|match_data| SemanticMatch {
                    similarity,
                    byte_range: match_data.byte_range.clone(),
                    content: match_data.content.clone(),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Coordinator for semantic search operations
pub struct SemanticSearcher {
    index: Option<SemanticIndex>,
    config: SemanticConfig,
}

impl SemanticSearcher {
    /// Create a new semantic searcher with the given configuration
    pub fn new(config: SemanticConfig) -> Self {
        Self {
            index: None,
            config,
        }
    }
    
    /// Set the search index
    pub fn set_index(&mut self, index: SemanticIndex) {
        self.index = Some(index);
    }
    
    /// Perform semantic search against the index
    pub fn search(&mut self, query: &str) -> Vec<SemanticMatch> {
        match &mut self.index {
            Some(index) => search_semantic(query, index, &self.config),
            None => Vec::new(),
        }
    }
}