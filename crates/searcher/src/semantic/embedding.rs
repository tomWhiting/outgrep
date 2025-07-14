use super::types::{Embedding, SemanticConfig};
use ndarray::{Array2, ArrayD, CowArray};
use ort::{
    Environment, GraphOptimizationLevel, Session, SessionBuilder, Value,
};
use std::path::Path;
use std::sync::Arc;

/// ONNX-based embedder using all-MiniLM-L6-v2
pub struct OnnxEmbedder {
    session: Session,
    tokenizer: tokenizers::Tokenizer,
    environment: Arc<Environment>,
}

impl OnnxEmbedder {
    /// Create new embedder with all-MiniLM-L6-v2 model
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let model_path = Path::new("models/model.onnx");
        let tokenizer_path = Path::new("models/tokenizer.json");

        if !model_path.exists() || !tokenizer_path.exists() {
            return Err(
                "Model files not found. Run from project root directory."
                    .into(),
            );
        }

        // Create environment
        let environment = Arc::new(
            Environment::builder().with_name("outgrep-semantic").build()?,
        );

        // Create session
        let session = SessionBuilder::new(&environment)?
            .with_optimization_level(GraphOptimizationLevel::Level1)?
            .with_model_from_file(model_path)?;

        // Load tokenizer
        let tokenizer = tokenizers::Tokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

        Ok(Self { session, tokenizer, environment })
    }

    /// Generate embedding using ONNX model
    pub fn embed(
        &self,
        text: &str,
    ) -> Result<Embedding, Box<dyn std::error::Error>> {
        // Tokenize input
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| format!("Tokenization failed: {}", e))?;

        let input_ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();

        // Convert to 2D ndarray tensors (batch_size=1, seq_len)
        let input_ids_2d: Array2<i64> = Array2::from_shape_vec(
            (1, input_ids.len()),
            input_ids.iter().map(|&x| x as i64).collect(),
        )?;
        let attention_mask_2d: Array2<i64> = Array2::from_shape_vec(
            (1, attention_mask.len()),
            attention_mask.iter().map(|&x| x as i64).collect(),
        )?;

        // Create token_type_ids (all zeros for single sentence)
        let token_type_ids_2d: Array2<i64> =
            Array2::zeros((1, input_ids.len()));

        // Convert to dynamic arrays for ort
        let input_ids_dyn: ArrayD<i64> = input_ids_2d.into_dyn();
        let attention_mask_dyn: ArrayD<i64> = attention_mask_2d.into_dyn();
        let token_type_ids_dyn: ArrayD<i64> = token_type_ids_2d.into_dyn();

        // Create cow arrays
        let input_ids_cow = CowArray::from(&input_ids_dyn);
        let attention_mask_cow = CowArray::from(&attention_mask_dyn);
        let token_type_ids_cow = CowArray::from(&token_type_ids_dyn);

        // Run inference
        let outputs = self.session.run(vec![
            Value::from_array(self.session.allocator(), &input_ids_cow)?,
            Value::from_array(self.session.allocator(), &attention_mask_cow)?,
            Value::from_array(self.session.allocator(), &token_type_ids_cow)?,
        ])?;

        // Extract embeddings from last_hidden_state
        let last_hidden_state = &outputs[0];
        let embedding_data = last_hidden_state.try_extract::<f32>()?;

        // Get tensor dimensions
        let tensor_view = embedding_data.view();
        let tensor_shape = tensor_view.shape();
        let _batch_size = tensor_shape[0];
        let seq_len = tensor_shape[1];
        let hidden_size = tensor_shape[2];

        let mut pooled = vec![0.0f32; hidden_size];
        let mut mask_sum = 0.0f32;

        // Mean pooling: average over sequence length weighted by attention mask
        for seq_idx in 0..seq_len {
            let mask_val = attention_mask[seq_idx] as f32;
            mask_sum += mask_val;

            for hidden_idx in 0..hidden_size {
                let val = tensor_view[[0, seq_idx, hidden_idx]];
                pooled[hidden_idx] += val * mask_val;
            }
        }

        // Normalize by mask sum
        if mask_sum > 0.0 {
            for val in &mut pooled {
                *val /= mask_sum;
            }
        }

        // L2 normalize for cosine similarity
        let norm = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut pooled {
                *val /= norm;
            }
        }

        // Verify normalization worked
        let norm_check = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
        if (norm_check - 1.0).abs() > 0.001 {
            eprintln!(
                "WARNING: Embedding not properly normalized! Norm: {:.6}",
                norm_check
            );
        }

        // Ensure we always return exactly 384 dimensions
        if pooled.len() != 384 {
            eprintln!(
                "WARNING: ONNX model produced {} dimensions, expected 384",
                pooled.len()
            );
            // Pad or truncate to exactly 384 dimensions
            pooled.resize(384, 0.0);
        }

        Ok(Embedding { vector: pooled, dimensions: 384 })
    }
}

/// Generate embedding for a code snippet
pub fn generate_embedding(code: &str, config: &SemanticConfig) -> Embedding {
    // Try ONNX model first, fall back to hash-based
    match OnnxEmbedder::new() {
        Ok(embedder) => {
            match embedder.embed(code) {
                Ok(embedding) => {
                    // Ensure ONNX embedding is exactly 384 dimensions
                    let mut vector = embedding.vector;
                    if vector.len() != 384 {
                        vector.resize(384, 0.0);
                    }
                    return Embedding { vector, dimensions: 384 };
                }
                Err(_) => {
                    // Fall through to hash-based embedding
                }
            }
        }
        Err(_) => {
            // Fall through to hash-based embedding
        }
    }

    // Fallback to hash-based embedding - always 384 dimensions
    let hash = simple_hash(code);
    let vector = hash_to_vector(hash, 384);

    Embedding { vector, dimensions: 384 }
}

/// Calculate cosine similarity between two embeddings
pub fn cosine_similarity(a: &Embedding, b: &Embedding) -> f32 {
    if a.dimensions != b.dimensions {
        return 0.0;
    }

    let dot_product: f32 =
        a.vector.iter().zip(b.vector.iter()).map(|(x, y)| x * y).sum();

    let magnitude_a: f32 = a.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.vector.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        0.0
    } else {
        dot_product / (magnitude_a * magnitude_b)
    }
}

fn simple_hash(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

fn hash_to_vector(hash: u64, dimensions: usize) -> Vec<f32> {
    let mut vector = Vec::with_capacity(dimensions);
    let mut current_hash = hash;

    for _ in 0..dimensions {
        vector.push((current_hash as f32) / (u64::MAX as f32));
        current_hash =
            current_hash.wrapping_mul(1103515245).wrapping_add(12345);
    }

    vector
}
