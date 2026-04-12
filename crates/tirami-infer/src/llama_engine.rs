use crate::engine::InferenceEngine;
use crate::token_stream::TokenOutputStream;
use tirami_core::{TiramiError, LayerRange};
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

/// Events sent from the engine thread back to the caller during streaming.
pub enum StreamEvent {
    /// A decoded text fragment (may be multiple chars if UTF-8 boundaries align).
    Token(String),
    /// Generation finished normally; carries the total token count.
    Done(u32),
    /// The engine encountered an error.
    Error(String),
}

enum EngineCmd {
    Load {
        model_path: PathBuf,
        reply: mpsc::Sender<Result<i32, TiramiError>>,
    },
    Generate {
        prompt: String,
        max_tokens: u32,
        temperature: f32,
        top_p: Option<f64>,
        top_k: Option<i32>,
        reply: mpsc::Sender<Result<Vec<(i32, Vec<u8>)>, TiramiError>>,
    },
    GenerateStreaming {
        prompt: String,
        max_tokens: u32,
        temperature: f32,
        top_p: Option<f64>,
        top_k: Option<i32>,
        token_tx: mpsc::SyncSender<StreamEvent>,
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
    ) -> Result<(), TiramiError> {
        // Validate paths
        let model_path = model_path
            .canonicalize()
            .map_err(|e| TiramiError::ModelLoadError(format!("invalid model path: {e}")))?;
        let tokenizer_path = tokenizer_path
            .canonicalize()
            .map_err(|e| TiramiError::ModelLoadError(format!("invalid tokenizer path: {e}")))?;

        tracing::info!("Loading GGUF model from {:?}", model_path);

        // Load on the engine thread
        let (reply_tx, reply_rx) = mpsc::channel();
        self.cmd_tx
            .as_ref()
            .ok_or_else(|| TiramiError::InferenceError("engine shut down".to_string()))?
            .send(EngineCmd::Load {
                model_path: model_path.clone(),
                reply: reply_tx,
            })
            .map_err(|_| TiramiError::InferenceError("engine thread died".to_string()))?;

        let eos = reply_rx
            .recv()
            .map_err(|_| TiramiError::InferenceError("engine thread died".to_string()))??;

        tracing::info!("Model loaded (llama.cpp), EOS={}", eos);

        // Load HF tokenizer on the caller thread (it is Send+Sync)
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| TiramiError::ModelLoadError(format!("load tokenizer: {e}")))?;
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
        top_p: Option<f64>,
        top_k: Option<i32>,
    ) -> Result<Vec<String>, TiramiError> {
        let tokenizer = self
            .tokenizer
            .as_ref()
            .ok_or_else(|| TiramiError::InferenceError("tokenizer not loaded".to_string()))?;

        let (reply_tx, reply_rx) = mpsc::channel();
        self.cmd_tx
            .as_ref()
            .ok_or_else(|| TiramiError::InferenceError("engine shut down".to_string()))?
            .send(EngineCmd::Generate {
                prompt: prompt.to_string(),
                max_tokens,
                temperature,
                top_p,
                top_k,
                reply: reply_tx,
            })
            .map_err(|_| TiramiError::InferenceError("engine thread died".to_string()))?;

        let raw_tokens = reply_rx
            .recv()
            .map_err(|_| TiramiError::InferenceError("engine thread died".to_string()))??;

        // Decode tokens to text fragments via HF tokenizer on the caller thread
        let mut token_stream = TokenOutputStream::new(tokenizer.clone());
        let mut generated: Vec<String> = Vec::new();

        for (token_id, _bytes) in &raw_tokens {
            if let Ok(Some(text)) = token_stream.next_token(*token_id as u32) {
                generated.push(text);
            }
        }

        if let Ok(Some(text)) = token_stream.flush() {
            generated.push(text);
        }

        Ok(generated)
    }

    fn generate_streaming(
        &mut self,
        prompt: &str,
        max_tokens: u32,
        temperature: f32,
        top_p: Option<f64>,
        top_k: Option<i32>,
        mut on_token: Box<dyn FnMut(&str) -> bool + Send>,
    ) -> Result<u32, TiramiError> {
        // Use a bounded sync channel so the engine thread can't run too far
        // ahead of the consumer without backpressure.
        let (token_tx, token_rx) = mpsc::sync_channel::<StreamEvent>(64);

        self.cmd_tx
            .as_ref()
            .ok_or_else(|| TiramiError::InferenceError("engine shut down".to_string()))?
            .send(EngineCmd::GenerateStreaming {
                prompt: prompt.to_string(),
                max_tokens,
                temperature,
                top_p,
                top_k,
                token_tx,
            })
            .map_err(|_| TiramiError::InferenceError("engine thread died".to_string()))?;

        // Drain the stream; call on_token for each fragment.
        loop {
            match token_rx.recv() {
                Ok(StreamEvent::Token(text)) => {
                    if !on_token(&text) {
                        // Caller signalled stop — drain remaining events so the
                        // engine thread doesn't block forever on a full channel.
                        while let Ok(ev) = token_rx.recv() {
                            if matches!(ev, StreamEvent::Done(_) | StreamEvent::Error(_)) {
                                break;
                            }
                        }
                        return Ok(0); // partial — count unknown
                    }
                }
                Ok(StreamEvent::Done(n)) => return Ok(n),
                Ok(StreamEvent::Error(msg)) => {
                    return Err(TiramiError::InferenceError(msg));
                }
                Err(_) => {
                    return Err(TiramiError::InferenceError(
                        "engine thread channel closed unexpectedly".to_string(),
                    ));
                }
            }
        }
    }

    fn tokenize(&self, prompt: &str) -> Result<Vec<u32>, TiramiError> {
        let tokenizer = self
            .tokenizer
            .as_ref()
            .ok_or_else(|| TiramiError::InferenceError("tokenizer not loaded".to_string()))?;
        let encoding = tokenizer
            .encode(prompt, true)
            .map_err(|e| TiramiError::InferenceError(format!("tokenize: {e}")))?;
        Ok(encoding.get_ids().to_vec())
    }

    fn decode(&self, tokens: &[u32]) -> Result<String, TiramiError> {
        let tokenizer = self
            .tokenizer
            .as_ref()
            .ok_or_else(|| TiramiError::InferenceError("tokenizer not loaded".to_string()))?;
        tokenizer
            .decode(tokens, true)
            .map_err(|e| TiramiError::InferenceError(format!("decode: {e}")))
    }

    fn forward_tokens(&mut self, _tokens: &[u32], _pos: usize) -> Result<Vec<f32>, TiramiError> {
        Err(TiramiError::InferenceError(
            "forward_tokens not yet implemented for llama.cpp backend".to_string(),
        ))
    }

    fn sample_token(
        &mut self,
        _logits: &[f32],
        _temperature: f32,
        _top_p: Option<f64>,
    ) -> Result<u32, TiramiError> {
        Err(TiramiError::InferenceError(
            "sample_token not yet implemented for llama.cpp backend".to_string(),
        ))
    }
}

/// Dedicated thread that owns all non-Send llama.cpp types.
fn engine_thread(rx: mpsc::Receiver<EngineCmd>) {
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::LlamaModel;

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
                let result = (|| -> Result<i32, TiramiError> {
                    let be = LlamaBackend::init()
                        .map_err(|e| TiramiError::ModelLoadError(format!("backend: {e}")))?;

                    let params = LlamaModelParams::default();
                    let m = LlamaModel::load_from_file(&be, &model_path, &params)
                        .map_err(|e| TiramiError::ModelLoadError(format!("load: {e}")))?;

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
                top_p,
                top_k,
                reply,
            } => {
                let result = run_generate(
                    backend.as_ref(),
                    model.as_ref(),
                    eos_token_id,
                    &prompt,
                    max_tokens,
                    temperature,
                    top_p,
                    top_k,
                );
                let _ = reply.send(result);
            }

            EngineCmd::GenerateStreaming {
                prompt,
                max_tokens,
                temperature,
                top_p,
                top_k,
                token_tx,
            } => {
                // Reuse run_generate but we need per-token callbacks here.
                // We call run_generate_streaming which sends tokens as they arrive.
                run_generate_streaming(
                    backend.as_ref(),
                    model.as_ref(),
                    eos_token_id,
                    &prompt,
                    max_tokens,
                    temperature,
                    top_p,
                    top_k,
                    &token_tx,
                );
            }

            EngineCmd::Shutdown => break,
        }
    }
}

/// Sample the next token from a candidates array, applying temperature, top-k,
/// and top-p (nucleus) in that order — matching the de-facto llama-server order.
fn sample_next_token(
    candidates_p: &mut llama_cpp_2::token::data_array::LlamaTokenDataArray,
    temperature: f32,
    top_p: Option<f64>,
    top_k: Option<i32>,
) -> llama_cpp_2::token::LlamaToken {
    use llama_cpp_2::sampling::LlamaSampler;

    if temperature <= 0.0 {
        return candidates_p.sample_token_greedy();
    }

    // Build a sampler chain: top-k → top-p → temperature → dist (random sample)
    // This matches the standard llama-server sampling order.
    let mut samplers: Vec<LlamaSampler> = Vec::new();

    if let Some(k) = top_k {
        if k > 0 {
            samplers.push(LlamaSampler::top_k(k));
        }
    }

    if let Some(p) = top_p {
        if p > 0.0 && p < 1.0 {
            samplers.push(LlamaSampler::top_p(p as f32, 1));
        }
    }

    samplers.push(LlamaSampler::temp(temperature));
    // Final dist sampler selects from the filtered/scaled distribution.
    samplers.push(LlamaSampler::dist(42));

    let chain = LlamaSampler::chain_simple(samplers);
    chain.apply(candidates_p);

    // After applying the chain, pick the selected token (highest after filtering).
    candidates_p
        .selected_token()
        .unwrap_or_else(|| candidates_p.sample_token_greedy())
}

/// Generate all tokens for a prompt, returning them as raw (token_id, bytes) pairs.
/// Runs on the engine thread.
fn run_generate(
    backend: Option<&llama_cpp_2::llama_backend::LlamaBackend>,
    model: Option<&llama_cpp_2::model::LlamaModel>,
    eos_token_id: i32,
    prompt: &str,
    max_tokens: u32,
    temperature: f32,
    top_p: Option<f64>,
    top_k: Option<i32>,
) -> Result<Vec<(i32, Vec<u8>)>, TiramiError> {
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::AddBos;
    use llama_cpp_2::token::data_array::LlamaTokenDataArray;
    use std::num::NonZeroU32;

    let be = backend.ok_or_else(|| TiramiError::InferenceError("not loaded".to_string()))?;
    let m = model.ok_or_else(|| TiramiError::InferenceError("not loaded".to_string()))?;

    let tokens = m
        .str_to_token(prompt, AddBos::Always)
        .map_err(|e| TiramiError::InferenceError(format!("tokenize: {e}")))?;

    if tokens.is_empty() {
        return Err(TiramiError::InferenceError("empty prompt".to_string()));
    }

    let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(4096));
    let mut ctx = m
        .new_context(be, ctx_params)
        .map_err(|e| TiramiError::InferenceError(format!("context: {e}")))?;

    let mut batch = LlamaBatch::new(4096, 1);
    let last_idx = tokens.len() as i32 - 1;
    for (i, &token) in tokens.iter().enumerate() {
        batch
            .add(token, i as i32, &[0], i as i32 == last_idx)
            .map_err(|e| TiramiError::InferenceError(format!("batch: {e}")))?;
    }
    ctx.decode(&mut batch)
        .map_err(|e| TiramiError::InferenceError(format!("decode: {e}")))?;

    let mut result = Vec::new();
    let mut n_decoded = tokens.len();

    for _ in 0..max_tokens {
        let candidates = ctx.candidates_ith(batch.n_tokens() - 1);
        let mut candidates_p = LlamaTokenDataArray::from_iter(candidates, false);

        let new_token = sample_next_token(&mut candidates_p, temperature, top_p, top_k);

        if new_token.0 == eos_token_id {
            break;
        }

        result.push((new_token.0, Vec::new()));

        batch.clear();
        batch
            .add(new_token, n_decoded as i32, &[0], true)
            .map_err(|e| TiramiError::InferenceError(format!("batch: {e}")))?;
        ctx.decode(&mut batch)
            .map_err(|e| TiramiError::InferenceError(format!("decode: {e}")))?;
        n_decoded += 1;
    }

    Ok(result)
}

/// Like `run_generate` but sends each decoded fragment immediately through
/// `token_tx` so the caller can stream output to the client.
fn run_generate_streaming(
    backend: Option<&llama_cpp_2::llama_backend::LlamaBackend>,
    model: Option<&llama_cpp_2::model::LlamaModel>,
    eos_token_id: i32,
    prompt: &str,
    max_tokens: u32,
    temperature: f32,
    top_p: Option<f64>,
    top_k: Option<i32>,
    token_tx: &mpsc::SyncSender<StreamEvent>,
) {
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::AddBos;
    use llama_cpp_2::token::data_array::LlamaTokenDataArray;
    use std::num::NonZeroU32;

    let inner = move || -> Result<(), TiramiError> {
        let be = backend.ok_or_else(|| TiramiError::InferenceError("not loaded".to_string()))?;
        let m = model.ok_or_else(|| TiramiError::InferenceError("not loaded".to_string()))?;

        let tokens = m
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| TiramiError::InferenceError(format!("tokenize: {e}")))?;

        if tokens.is_empty() {
            return Err(TiramiError::InferenceError("empty prompt".to_string()));
        }

        let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(4096));
        let mut ctx = m
            .new_context(be, ctx_params)
            .map_err(|e| TiramiError::InferenceError(format!("context: {e}")))?;

        let mut batch = LlamaBatch::new(4096, 1);
        let last_idx = tokens.len() as i32 - 1;
        for (i, &token) in tokens.iter().enumerate() {
            batch
                .add(token, i as i32, &[0], i as i32 == last_idx)
                .map_err(|e| TiramiError::InferenceError(format!("batch: {e}")))?;
        }
        ctx.decode(&mut batch)
            .map_err(|e| TiramiError::InferenceError(format!("decode: {e}")))?;

        // We need a TokenOutputStream to handle partial UTF-8, but we don't
        // have the HF tokenizer on this thread.  Instead we use the llama.cpp
        // model's own token_to_piece() for on-the-fly decoding — this is
        // equivalent and avoids cross-thread tokenizer cloning.
        let mut n_decoded = tokens.len();
        let mut total = 0u32;

        for _ in 0..max_tokens {
            let candidates = ctx.candidates_ith(batch.n_tokens() - 1);
            let mut candidates_p = LlamaTokenDataArray::from_iter(candidates, false);

            let new_token = sample_next_token(&mut candidates_p, temperature, top_p, top_k);

            if new_token.0 == eos_token_id {
                break;
            }

            // Decode this single token to UTF-8 using the model's vocabulary.
            // `token_to_str` is the simplest API available in llama-cpp-2 for
            // decoding a single token without an external encoding_rs::Decoder.
            #[allow(deprecated)]
            let piece = m.token_to_str(new_token, llama_cpp_2::model::Special::Tokenize)
                .unwrap_or_default();

            // Send to caller — if channel is closed, stop early.
            if token_tx.send(StreamEvent::Token(piece)).is_err() {
                return Ok(());
            }
            total += 1;

            batch.clear();
            batch
                .add(new_token, n_decoded as i32, &[0], true)
                .map_err(|e| TiramiError::InferenceError(format!("batch: {e}")))?;
            ctx.decode(&mut batch)
                .map_err(|e| TiramiError::InferenceError(format!("decode: {e}")))?;
            n_decoded += 1;
        }

        let _ = token_tx.send(StreamEvent::Done(total));
        Ok(())
    };

    if let Err(e) = inner() {
        let _ = token_tx.send(StreamEvent::Error(e.to_string()));
    }
}
