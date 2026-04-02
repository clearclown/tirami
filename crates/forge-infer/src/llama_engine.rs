use crate::engine::InferenceEngine;
use crate::token_stream::TokenOutputStream;
use forge_core::{ForgeError, LayerRange};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use tokenizers::Tokenizer;

/// Thread-safe llama.cpp inference engine.
///
/// llama-cpp-2 types (LlamaModel, LlamaContext) are not Send.
/// Instead of unsafe impl Send, we run all llama.cpp operations
/// on a dedicated thread and communicate via channels.
pub struct LlamaCppEngine {
    /// Channel to send commands to the inference thread.
    cmd_tx: Option<mpsc::Sender<EngineCmd>>,
    /// HF tokenizer (Send+Sync safe, used on the caller thread).
    tokenizer: Option<Tokenizer>,
    loaded: bool,
}

enum EngineCmd {
    Load {
        model_path: PathBuf,
        reply: mpsc::Sender<Result<i32, ForgeError>>,
    },
    Generate {
        prompt: String,
        max_tokens: u32,
        temperature: f32,
        reply: mpsc::Sender<Result<Vec<(i32, Vec<u8>)>, ForgeError>>,
    },
    Shutdown,
}

impl LlamaCppEngine {
    pub fn new() -> Self {
        tracing::info!("Inference engine: llama.cpp (thread-safe)");

        let (cmd_tx, cmd_rx) = mpsc::channel::<EngineCmd>();

        // Spawn a dedicated thread for all llama.cpp operations.
        // This thread owns all non-Send llama.cpp types.
        std::thread::Builder::new()
            .name("llama-engine".to_string())
            .spawn(move || {
                engine_thread(cmd_rx);
            })
            .expect("failed to spawn llama engine thread");

        Self {
            cmd_tx: Some(cmd_tx),
            tokenizer: None,
            loaded: false,
        }
    }
}

impl Default for LlamaCppEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LlamaCppEngine {
    fn drop(&mut self) {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(EngineCmd::Shutdown);
        }
    }
}

impl InferenceEngine for LlamaCppEngine {
    fn load(
        &mut self,
        model_path: &Path,
        tokenizer_path: &Path,
        _layer_range: Option<LayerRange>,
    ) -> Result<(), ForgeError> {
        // Validate paths
        let model_path = model_path.canonicalize().map_err(|e| {
            ForgeError::ModelLoadError(format!("invalid model path: {e}"))
        })?;
        let tokenizer_path = tokenizer_path.canonicalize().map_err(|e| {
            ForgeError::ModelLoadError(format!("invalid tokenizer path: {e}"))
        })?;

        tracing::info!("Loading GGUF model from {:?}", model_path);

        // Load on the engine thread
        let (reply_tx, reply_rx) = mpsc::channel();
        self.cmd_tx
            .as_ref()
            .ok_or_else(|| ForgeError::InferenceError("engine shut down".to_string()))?
            .send(EngineCmd::Load {
                model_path: model_path.clone(),
                reply: reply_tx,
            })
            .map_err(|_| ForgeError::InferenceError("engine thread died".to_string()))?;

        let eos = reply_rx
            .recv()
            .map_err(|_| ForgeError::InferenceError("engine thread died".to_string()))??;

        tracing::info!("Model loaded (llama.cpp), EOS={}", eos);

        // Load HF tokenizer on the caller thread (it is Send+Sync)
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| ForgeError::ModelLoadError(format!("load tokenizer: {e}")))?;
        tracing::info!("HF tokenizer loaded");

        self.tokenizer = Some(tokenizer);
        self.loaded = true;
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    fn generate(
        &mut self,
        prompt: &str,
        max_tokens: u32,
        temperature: f32,
        _top_p: Option<f64>,
    ) -> Result<Vec<String>, ForgeError> {
        let tokenizer = self
            .tokenizer
            .as_ref()
            .ok_or_else(|| ForgeError::InferenceError("tokenizer not loaded".to_string()))?;

        let (reply_tx, reply_rx) = mpsc::channel();
        self.cmd_tx
            .as_ref()
            .ok_or_else(|| ForgeError::InferenceError("engine shut down".to_string()))?
            .send(EngineCmd::Generate {
                prompt: prompt.to_string(),
                max_tokens,
                temperature,
                reply: reply_tx,
            })
            .map_err(|_| ForgeError::InferenceError("engine thread died".to_string()))?;

        let raw_tokens = reply_rx
            .recv()
            .map_err(|_| ForgeError::InferenceError("engine thread died".to_string()))??;

        // Decode tokens to text fragments via HF tokenizer on the caller thread
        let mut token_stream = TokenOutputStream::new(tokenizer.clone());
        let mut generated: Vec<String> = Vec::new();

        for (_token_id, _bytes) in &raw_tokens {
            if let Ok(Some(text)) = token_stream.next_token(*_token_id as u32) {
                generated.push(text);
            }
        }

        if let Ok(Some(text)) = token_stream.flush() {
            generated.push(text);
        }

        Ok(generated)
    }

    fn tokenize(&self, prompt: &str) -> Result<Vec<u32>, ForgeError> {
        let tokenizer = self.tokenizer.as_ref()
            .ok_or_else(|| ForgeError::InferenceError("tokenizer not loaded".to_string()))?;
        let encoding = tokenizer.encode(prompt, true)
            .map_err(|e| ForgeError::InferenceError(format!("tokenize: {e}")))?;
        Ok(encoding.get_ids().to_vec())
    }

    fn decode(&self, tokens: &[u32]) -> Result<String, ForgeError> {
        let tokenizer = self.tokenizer.as_ref()
            .ok_or_else(|| ForgeError::InferenceError("tokenizer not loaded".to_string()))?;
        tokenizer.decode(tokens, true)
            .map_err(|e| ForgeError::InferenceError(format!("decode: {e}")))
    }

    fn forward_tokens(
        &mut self,
        _tokens: &[u32],
        _pos: usize,
    ) -> Result<Vec<f32>, ForgeError> {
        Err(ForgeError::InferenceError(
            "forward_tokens not yet implemented for llama.cpp backend".to_string(),
        ))
    }

    fn sample_token(
        &mut self,
        _logits: &[f32],
        _temperature: f32,
        _top_p: Option<f64>,
    ) -> Result<u32, ForgeError> {
        Err(ForgeError::InferenceError(
            "sample_token not yet implemented for llama.cpp backend".to_string(),
        ))
    }
}

/// Dedicated thread that owns all non-Send llama.cpp types.
fn engine_thread(rx: mpsc::Receiver<EngineCmd>) {
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::{AddBos, LlamaModel};
    use llama_cpp_2::token::data_array::LlamaTokenDataArray;
    use std::num::NonZeroU32;

    let mut backend: Option<LlamaBackend> = None;
    let mut model: Option<LlamaModel> = None;
    let mut eos_token_id: i32 = -1;

    loop {
        let cmd = match rx.recv() {
            Ok(cmd) => cmd,
            Err(_) => break, // channel closed
        };

        match cmd {
            EngineCmd::Load { model_path, reply } => {
                let result = (|| -> Result<i32, ForgeError> {
                    let be = LlamaBackend::init()
                        .map_err(|e| ForgeError::ModelLoadError(format!("backend: {e}")))?;

                    let params = LlamaModelParams::default();
                    let m = LlamaModel::load_from_file(&be, &model_path, &params)
                        .map_err(|e| ForgeError::ModelLoadError(format!("load: {e}")))?;

                    let eos = m.token_eos().0;
                    eos_token_id = eos;
                    model = Some(m);
                    backend = Some(be);
                    Ok(eos)
                })();
                let _ = reply.send(result);
            }

            EngineCmd::Generate {
                prompt,
                max_tokens,
                temperature,
                reply,
            } => {
                let result = (|| -> Result<Vec<(i32, Vec<u8>)>, ForgeError> {
                    let be = backend.as_ref()
                        .ok_or_else(|| ForgeError::InferenceError("not loaded".to_string()))?;
                    let m = model.as_ref()
                        .ok_or_else(|| ForgeError::InferenceError("not loaded".to_string()))?;

                    let tokens = m.str_to_token(&prompt, AddBos::Always)
                        .map_err(|e| ForgeError::InferenceError(format!("tokenize: {e}")))?;

                    if tokens.is_empty() {
                        return Err(ForgeError::InferenceError("empty prompt".to_string()));
                    }

                    let ctx_params = LlamaContextParams::default()
                        .with_n_ctx(NonZeroU32::new(4096));
                    let mut ctx = m.new_context(be, ctx_params)
                        .map_err(|e| ForgeError::InferenceError(format!("context: {e}")))?;

                    let mut batch = LlamaBatch::new(4096, 1);
                    let last_idx = tokens.len() as i32 - 1;
                    for (i, &token) in tokens.iter().enumerate() {
                        batch.add(token, i as i32, &[0], i as i32 == last_idx)
                            .map_err(|e| ForgeError::InferenceError(format!("batch: {e}")))?;
                    }
                    ctx.decode(&mut batch)
                        .map_err(|e| ForgeError::InferenceError(format!("decode: {e}")))?;

                    let mut result = Vec::new();
                    let mut n_decoded = tokens.len();

                    for _ in 0..max_tokens {
                        let candidates = ctx.candidates_ith(batch.n_tokens() - 1);
                        let mut candidates_p =
                            LlamaTokenDataArray::from_iter(candidates, false);

                        let new_token = if temperature <= 0.0 {
                            candidates_p.sample_token_greedy()
                        } else {
                            candidates_p.sample_token(42)
                        };

                        if new_token.0 == eos_token_id {
                            break;
                        }

                        // Return raw token ID (decoded to text on caller thread)
                        result.push((new_token.0, Vec::new()));

                        batch.clear();
                        batch.add(new_token, n_decoded as i32, &[0], true)
                            .map_err(|e| ForgeError::InferenceError(format!("batch: {e}")))?;
                        ctx.decode(&mut batch)
                            .map_err(|e| ForgeError::InferenceError(format!("decode: {e}")))?;
                        n_decoded += 1;
                    }

                    Ok(result)
                })();
                let _ = reply.send(result);
            }

            EngineCmd::Shutdown => break,
        }
    }
}
