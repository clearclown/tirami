//! Model auto-download and resolution from HuggingFace Hub.
//!
//! Eliminates the manual GGUF + tokenizer download step.
//! `forge chat "Hello"` just works — model downloads automatically.

use tirami_core::TiramiError;
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

/// Resolved paths to model files on disk (returned by the legacy `resolve_model`).
pub struct ResolvedModel {
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub spec: ModelSpec,
}

/// Download (or find in cache) model files from HuggingFace.
pub fn resolve_model(spec: &ModelSpec) -> Result<ResolvedModel, TiramiError> {
    use hf_hub::api::sync::ApiBuilder;

    let api = ApiBuilder::new()
        .with_progress(true)
        .build()
        .map_err(|e| TiramiError::ModelLoadError(format!("HuggingFace API init: {e}")))?;

    eprintln!("Resolving model: {} (~{}MB)", spec.name, spec.size_mb);

    let model_path = api
        .model(spec.gguf_repo.clone())
        .get(&spec.gguf_file)
        .map_err(|e| TiramiError::ModelLoadError(format!("download GGUF: {e}")))?;

    let tokenizer_path = api
        .model(spec.tokenizer_repo.clone())
        .get(&spec.tokenizer_file)
        .map_err(|e| TiramiError::ModelLoadError(format!("download tokenizer: {e}")))?;

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

// ─── Unified resolver ────────────────────────────────────────────────────────

/// Where a resolved model originated from.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveSource {
    /// User supplied a path to an existing .gguf file on disk.
    LocalFile,
    /// Found by scanning `~/.models` or `$FORGE_MODELS_DIR`.
    ManagedDir,
    /// Looked up in the built-in catalog and downloaded via hf-hub.
    CatalogDownload,
    /// Downloaded directly from a full `https://huggingface.co/…` URL.
    HfUrl,
    /// Downloaded via the `org/repo/file.gguf` shorthand.
    HfShorthand,
}

/// Unified resolved model result from the high-level `resolve()` dispatcher.
#[derive(Debug)]
pub struct UnifiedResolvedModel {
    pub model_path: PathBuf,
    pub tokenizer_path: Option<PathBuf>,
    pub source: ResolveSource,
}

/// Resolve a user-provided model specifier to local file paths.
///
/// Supports (in priority order):
/// 1. Local file path — if the string points to an existing `.gguf` file.
/// 2. HuggingFace full URL — `https://huggingface.co/<org>/<repo>/resolve/<branch>/<file>.gguf`
/// 3. HF shorthand — `<org>/<repo>/<file>.gguf` or `<org>/<repo>` (uses first .gguf)
/// 4. Catalog name — `qwen2.5:0.5b`, `smollm2:135m`, etc. (auto-download)
/// 5. Fallback — scan `~/.models/` and `$FORGE_MODELS_DIR/` for a matching file
pub fn resolve(input: &str) -> Result<UnifiedResolvedModel, TiramiError> {
    let input = input.trim();

    // 1. Local file path
    let path = PathBuf::from(input);
    if path.exists() && path.extension().map(|e| e == "gguf").unwrap_or(false) {
        return Ok(UnifiedResolvedModel {
            model_path: path,
            tokenizer_path: None,
            source: ResolveSource::LocalFile,
        });
    }

    // 2. HF full URL
    if let Some((repo, file, branch)) = parse_hf_url(input) {
        let (model_path, tokenizer_path) = download_hf_file(&repo, &file, &branch)?;
        return Ok(UnifiedResolvedModel {
            model_path,
            tokenizer_path: Some(tokenizer_path),
            source: ResolveSource::HfUrl,
        });
    }

    // 3. HF shorthand
    if let Some((repo, file)) = parse_hf_shorthand(input) {
        let file = file.unwrap_or_else(|| "model.gguf".to_string());
        let (model_path, tokenizer_path) = download_hf_file(&repo, &file, "main")?;
        return Ok(UnifiedResolvedModel {
            model_path,
            tokenizer_path: Some(tokenizer_path),
            source: ResolveSource::HfShorthand,
        });
    }

    // 4. Catalog name (existing path)
    if let Some(spec) = find_model(input) {
        let resolved = resolve_model(&spec)?;
        return Ok(UnifiedResolvedModel {
            model_path: resolved.model_path,
            tokenizer_path: Some(resolved.tokenizer_path),
            source: ResolveSource::CatalogDownload,
        });
    }

    // 5. Managed dir scan (by partial filename match)
    if let Some(path) = scan_local_dirs(input) {
        return Ok(UnifiedResolvedModel {
            model_path: path,
            tokenizer_path: None,
            source: ResolveSource::ManagedDir,
        });
    }

    Err(TiramiError::ModelLoadError(format!(
        "could not resolve '{}' — not a local file, HF URL, HF shorthand, catalog name, or ~/.models match. Run 'forge models' for catalog.",
        input
    )))
}

/// Parse a HuggingFace URL of the form
/// `https://huggingface.co/<org>/<repo>/resolve/<branch>/<file>`
/// into `(org/repo, file, branch)`.
pub fn parse_hf_url(url: &str) -> Option<(String, String, String)> {
    let url = url.trim();
    let prefix = "https://huggingface.co/";
    if !url.starts_with(prefix) {
        return None;
    }
    let rest = &url[prefix.len()..];
    let parts: Vec<&str> = rest.split('/').collect();
    // Need at least: org, repo, "resolve", branch, file
    if parts.len() < 5 || parts[2] != "resolve" {
        return None;
    }
    let org_repo = format!("{}/{}", parts[0], parts[1]);
    let branch = parts[3].to_string();
    let file = parts[4..].join("/");
    Some((org_repo, file, branch))
}

/// Parse a shorthand like `"org/repo/file.gguf"` or `"org/repo"` (uses first .gguf found).
///
/// Returns `(org/repo, Option<file>)`. Returns `None` for http(s) URLs or inputs
/// with anything other than 2–3 slash-separated parts.
pub fn parse_hf_shorthand(input: &str) -> Option<(String, Option<String>)> {
    if input.starts_with("http://") || input.starts_with("https://") {
        return None;
    }
    let parts: Vec<&str> = input.split('/').collect();
    match parts.len() {
        2 => Some((format!("{}/{}", parts[0], parts[1]), None)),
        3 => Some((
            format!("{}/{}", parts[0], parts[1]),
            Some(parts[2].to_string()),
        )),
        _ => None,
    }
}

/// Scan `$FORGE_MODELS_DIR` and `~/.models` for a `.gguf` file whose name contains `query`.
pub fn scan_local_dirs(query: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(env) = std::env::var("FORGE_MODELS_DIR") {
        dirs.push(PathBuf::from(env));
    }
    // Use $HOME rather than the `dirs` crate to avoid a new dependency.
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".models"));
    }
    for dir in dirs {
        if !dir.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name() {
                    let name = name.to_string_lossy();
                    if name.contains(query) && name.ends_with(".gguf") {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

/// Download a specific GGUF file + tokenizer from HuggingFace by repo + file + branch.
///
/// Tokenizer resolution strategy:
/// 1. Try `tokenizer.json` from the same repo.
/// 2. Strip `-GGUF` / `-gguf` suffix from the repo name and try there.
/// 3. If neither works, return an error.
fn download_hf_file(
    repo: &str,
    file: &str,
    _branch: &str,
) -> Result<(PathBuf, PathBuf), TiramiError> {
    use hf_hub::api::sync::ApiBuilder;

    let api = ApiBuilder::new()
        .with_progress(true)
        .build()
        .map_err(|e| TiramiError::ModelLoadError(format!("hf-hub init: {e}")))?;

    let model_repo = api.model(repo.to_string());
    let model_path = model_repo
        .get(file)
        .map_err(|e| TiramiError::ModelLoadError(format!("download {repo}/{file}: {e}")))?;

    // Try tokenizer.json from the GGUF repo first.
    let tokenizer_path = match model_repo.get("tokenizer.json") {
        Ok(p) => p,
        Err(_) => {
            // Fallback: derive the non-GGUF repo name (strip -GGUF suffix).
            let non_gguf_repo = repo
                .strip_suffix("-GGUF")
                .or_else(|| repo.strip_suffix("-gguf"))
                .unwrap_or(repo)
                .to_string();
            if non_gguf_repo != repo {
                let alt_repo = api.model(non_gguf_repo);
                alt_repo.get("tokenizer.json").map_err(|e| {
                    TiramiError::ModelLoadError(format!("download tokenizer: {e}"))
                })?
            } else {
                return Err(TiramiError::ModelLoadError(format!(
                    "tokenizer.json not found in {repo}"
                )));
            }
        }
    };

    Ok((model_path, tokenizer_path))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialise all tests that mutate environment variables.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // --- parse_hf_url ---

    #[test]
    fn test_parse_hf_url_valid() {
        let url = "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf";
        let (repo, file, branch) = parse_hf_url(url).unwrap();
        assert_eq!(repo, "Qwen/Qwen2.5-0.5B-Instruct-GGUF");
        assert_eq!(file, "qwen2.5-0.5b-instruct-q4_k_m.gguf");
        assert_eq!(branch, "main");
    }

    #[test]
    fn test_parse_hf_url_rejects_non_hf_url() {
        assert!(parse_hf_url("https://example.com/foo/bar.gguf").is_none());
    }

    #[test]
    fn test_parse_hf_url_rejects_missing_resolve() {
        assert!(
            parse_hf_url("https://huggingface.co/foo/bar/main/file.gguf").is_none()
        );
    }

    #[test]
    fn test_parse_hf_url_multi_segment_file() {
        let url = "https://huggingface.co/org/repo/resolve/main/subdir/model.gguf";
        let (repo, file, branch) = parse_hf_url(url).unwrap();
        assert_eq!(repo, "org/repo");
        assert_eq!(file, "subdir/model.gguf");
        assert_eq!(branch, "main");
    }

    // --- parse_hf_shorthand ---

    #[test]
    fn test_parse_hf_shorthand_three_parts() {
        let (repo, file) = parse_hf_shorthand(
            "Qwen/Qwen2.5-0.5B-Instruct-GGUF/qwen2.5-0.5b-instruct-q4_k_m.gguf",
        )
        .unwrap();
        assert_eq!(repo, "Qwen/Qwen2.5-0.5B-Instruct-GGUF");
        assert_eq!(
            file,
            Some("qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string())
        );
    }

    #[test]
    fn test_parse_hf_shorthand_two_parts() {
        let (repo, file) =
            parse_hf_shorthand("Qwen/Qwen2.5-0.5B-Instruct-GGUF").unwrap();
        assert_eq!(repo, "Qwen/Qwen2.5-0.5B-Instruct-GGUF");
        assert_eq!(file, None);
    }

    #[test]
    fn test_parse_hf_shorthand_rejects_https_url() {
        assert!(parse_hf_shorthand("https://foo/bar").is_none());
    }

    #[test]
    fn test_parse_hf_shorthand_rejects_http_url() {
        assert!(parse_hf_shorthand("http://huggingface.co/org/repo/file.gguf").is_none());
    }

    #[test]
    fn test_parse_hf_shorthand_rejects_single_part() {
        assert!(parse_hf_shorthand("justanamenoSlash").is_none());
    }

    // --- resolve() local file dispatch ---

    #[test]
    fn test_resolve_local_file() {
        let tmp = std::env::temp_dir().join("test_forge_resolve.gguf");
        std::fs::write(&tmp, b"dummy").unwrap();
        let result = resolve(tmp.to_str().unwrap()).unwrap();
        assert_eq!(result.source, ResolveSource::LocalFile);
        assert_eq!(result.model_path, tmp);
        assert!(result.tokenizer_path.is_none());
        std::fs::remove_file(&tmp).ok();
    }

    // --- resolve() unknown model falls through to error ---

    #[test]
    fn test_resolve_unknown_name_returns_error() {
        let result = resolve("nonexistent-model-name-xyz");
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("could not resolve"),
            "unexpected error: {}",
            err_msg
        );
    }

    // --- scan_local_dirs ---

    #[test]
    fn test_scan_local_dirs_finds_matching_file() {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp_dir = std::env::temp_dir()
            .join(format!("forge_test_scan_{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let gguf = tmp_dir.join("test-model-xyz.gguf");
        std::fs::write(&gguf, b"dummy").unwrap();
        // SAFETY: protected by ENV_LOCK; no concurrent env mutations.
        unsafe { std::env::set_var("FORGE_MODELS_DIR", tmp_dir.to_str().unwrap()); }
        let result = scan_local_dirs("test-model-xyz");
        unsafe { std::env::remove_var("FORGE_MODELS_DIR"); }
        std::fs::remove_dir_all(&tmp_dir).ok();
        assert_eq!(result, Some(gguf));
    }

    #[test]
    fn test_scan_local_dirs_returns_none_for_missing() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Point at a nonexistent dir — should return None, not panic.
        // SAFETY: protected by ENV_LOCK; no concurrent env mutations.
        unsafe { std::env::set_var("FORGE_MODELS_DIR", "/tmp/forge_no_such_dir_xyzzy"); }
        let result = scan_local_dirs("some-model");
        unsafe { std::env::remove_var("FORGE_MODELS_DIR"); }
        assert!(result.is_none());
    }
}
