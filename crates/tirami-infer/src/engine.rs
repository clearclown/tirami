use tirami_core::LayerRange;
use std::path::Path;

/// The inference engine trait. Implemented by Candle backend.
pub trait InferenceEngine: Send + Sync {
    /// Load a GGUF model (or a slice of layers for distributed inference).
    fn load(
        &mut self,
        model_path: &Path,
        tokenizer_path: &Path,
        layer_range: Option<LayerRange>,
    ) -> Result<(), tirami_core::TiramiError>;

    /// Check if a model is loaded and ready.
    fn is_loaded(&self) -> bool;

    /// Generate tokens from a prompt string.
    /// Returns a vector of generated text fragments.
    fn generate(
        &mut self,
        prompt: &str,
        max_tokens: u32,
        temperature: f32,
        top_p: Option<f64>,
        top_k: Option<i32>,
    ) -> Result<Vec<String>, tirami_core::TiramiError>;

    /// Generate tokens one at a time, sending each decoded fragment through the
    /// supplied callback.  The callback returns `true` to continue or `false` to
    /// stop early (e.g. because the client disconnected).
    ///
    /// Returns the total number of tokens generated.
    ///
    /// Default implementation buffers the full `generate()` output and calls
    /// the callback once per fragment — existing engine implementations get this
    /// for free without any changes.  `LlamaCppEngine` overrides it with a true
    /// token-by-token streaming path.
    fn generate_streaming(
        &mut self,
        prompt: &str,
        max_tokens: u32,
        temperature: f32,
        top_p: Option<f64>,
        top_k: Option<i32>,
        mut on_token: Box<dyn FnMut(&str) -> bool + Send>,
    ) -> Result<u32, tirami_core::TiramiError> {
        let tokens = self.generate(prompt, max_tokens, temperature, top_p, top_k)?;
        let count = tokens.len() as u32;
        for fragment in &tokens {
            if !on_token(fragment) {
                break;
            }
        }
        Ok(count)
    }

    /// Phase 14.3 — run a deterministic forward pass and return a SHA-256 hash
    /// of the resulting logits. Used by the audit challenge/response protocol:
    /// challenger and target both run this on the same `input_tokens` and
    /// compare hashes.
    ///
    /// Default implementation iterates `forward_tokens` and hashes the raw
    /// logit bytes. Concrete engines may override for tighter determinism.
    ///
    /// Note: the calling side must pick input tokens whose determinism is
    /// insensitive to Metal/CUDA non-determinism (small context, temperature=0).
    fn generate_audit(&mut self, input_tokens: &[u32]) -> Result<[u8; 32], tirami_core::TiramiError> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        for i in 0..input_tokens.len() {
            let logits = self.forward_tokens(&input_tokens[..=i], i)?;
            // Hash bytes of each f32 little-endian; truncating mantissa noise
            // to first 16 bits reduces FP non-determinism across backends.
            for f in logits.iter() {
                let bits = f.to_bits();
                // Keep sign + exponent + top 7 mantissa bits → less jitter.
                let stable = (bits & 0xFFFF_0000) as u32;
                hasher.update(stable.to_le_bytes());
            }
        }
        let out = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&out);
        Ok(hash)
    }

    /// Tokenize a prompt and return token IDs.
    fn tokenize(&self, prompt: &str) -> Result<Vec<u32>, tirami_core::TiramiError>;

    /// Decode token IDs back to text.
    fn decode(&self, tokens: &[u32]) -> Result<String, tirami_core::TiramiError>;

    /// Run a forward pass on token IDs and return raw logits for the last position.
    /// This is used for split-inference coordination — the seed runs the full model
    /// but exposes the forward pass result for activation routing.
    fn forward_tokens(
        &mut self,
        tokens: &[u32],
        pos: usize,
    ) -> Result<Vec<f32>, tirami_core::TiramiError>;

    /// Sample the next token from logits.
    fn sample_token(
        &mut self,
        logits: &[f32],
        temperature: f32,
        top_p: Option<f64>,
    ) -> Result<u32, tirami_core::TiramiError>;
}
