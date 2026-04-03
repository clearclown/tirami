pub mod distributed;
pub mod engine;
pub mod gguf;
pub mod llama_engine;
pub mod model_registry;
pub mod rpc_manager;
pub mod token_stream;

pub use engine::InferenceEngine;
pub use gguf::parse_gguf_metadata;
pub use llama_engine::LlamaCppEngine;

/// Backward-compatible alias during migration.
pub type CandleEngine = LlamaCppEngine;
