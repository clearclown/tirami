//! Model auto-download and resolution from HuggingFace Hub.
//!
//! Eliminates the manual GGUF + tokenizer download step.
//! `forge chat "Hello"` just works — model downloads automatically.

use forge_core::ForgeError;
use std::path::PathBuf;

/// A model specification pointing to HuggingFace repos.
#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub name: String,
    pub gguf_repo: String,
    pub gguf_file: String,
    pub tokenizer_repo: String,
    pub tokenizer_file: String,
    pub size_mb: u64,
}

/// Built-in model registry.
pub fn builtin_models() -> Vec<ModelSpec> {
    vec![
        ModelSpec {
            name: "qwen2.5:0.5b".to_string(),
            gguf_repo: "Qwen/Qwen2.5-0.5B-Instruct-GGUF".to_string(),
            gguf_file: "qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string(),
            tokenizer_repo: "Qwen/Qwen2.5-0.5B-Instruct".to_string(),
            tokenizer_file: "tokenizer.json".to_string(),
            size_mb: 491,
        },
        ModelSpec {
            name: "qwen2.5:1.5b".to_string(),
            gguf_repo: "Qwen/Qwen2.5-1.5B-Instruct-GGUF".to_string(),
            gguf_file: "qwen2.5-1.5b-instruct-q4_k_m.gguf".to_string(),
            tokenizer_repo: "Qwen/Qwen2.5-1.5B-Instruct".to_string(),
            tokenizer_file: "tokenizer.json".to_string(),
            size_mb: 1100,
        },
        ModelSpec {
            name: "qwen2.5:3b".to_string(),
            gguf_repo: "Qwen/Qwen2.5-3B-Instruct-GGUF".to_string(),
            gguf_file: "qwen2.5-3b-instruct-q4_k_m.gguf".to_string(),
            tokenizer_repo: "Qwen/Qwen2.5-3B-Instruct".to_string(),
            tokenizer_file: "tokenizer.json".to_string(),
            size_mb: 2000,
        },
        ModelSpec {
            name: "qwen2.5:7b".to_string(),
            gguf_repo: "Qwen/Qwen2.5-7B-Instruct-GGUF".to_string(),
            gguf_file: "qwen2.5-7b-instruct-q4_k_m.gguf".to_string(),
            tokenizer_repo: "Qwen/Qwen2.5-7B-Instruct".to_string(),
            tokenizer_file: "tokenizer.json".to_string(),
            size_mb: 4700,
        },
        ModelSpec {
            name: "smollm2:135m".to_string(),
            gguf_repo: "bartowski/SmolLM2-135M-Instruct-GGUF".to_string(),
            gguf_file: "SmolLM2-135M-Instruct-Q4_K_M.gguf".to_string(),
            tokenizer_repo: "HuggingFaceTB/SmolLM2-135M-Instruct".to_string(),
            tokenizer_file: "tokenizer.json".to_string(),
            size_mb: 100,
        },
    ]
}

/// Default model for new users.
pub fn default_model() -> ModelSpec {
    builtin_models()
        .into_iter()
        .find(|m| m.name == "qwen2.5:0.5b")
        .unwrap()
}

/// Lookup a model by name (e.g., "qwen2.5:0.5b").
pub fn find_model(name: &str) -> Option<ModelSpec> {
    builtin_models().into_iter().find(|m| m.name == name)
}

/// Resolved paths to model files on disk.
pub struct ResolvedModel {
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub spec: ModelSpec,
}

/// Download (or find in cache) model files from HuggingFace.
pub fn resolve_model(spec: &ModelSpec) -> Result<ResolvedModel, ForgeError> {
    use hf_hub::api::sync::ApiBuilder;

    let api = ApiBuilder::new()
        .with_progress(true)
        .build()
        .map_err(|e| ForgeError::ModelLoadError(format!("HuggingFace API init: {e}")))?;

    eprintln!("Resolving model: {} (~{}MB)", spec.name, spec.size_mb);

    let model_path = api
        .model(spec.gguf_repo.clone())
        .get(&spec.gguf_file)
        .map_err(|e| ForgeError::ModelLoadError(format!("download GGUF: {e}")))?;

    let tokenizer_path = api
        .model(spec.tokenizer_repo.clone())
        .get(&spec.tokenizer_file)
        .map_err(|e| ForgeError::ModelLoadError(format!("download tokenizer: {e}")))?;

    Ok(ResolvedModel {
        model_path,
        tokenizer_path,
        spec: spec.clone(),
    })
}

/// List all available models.
pub fn list_models() {
    println!("Available models:");
    for m in builtin_models() {
        println!("  {:20} ~{}MB  {}/{}", m.name, m.size_mb, m.gguf_repo, m.gguf_file);
    }
}
