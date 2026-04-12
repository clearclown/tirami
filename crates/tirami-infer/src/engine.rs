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
