use tirami_core::{TiramiError, ModelId, ModelManifest};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// GGUF metadata value types (subset we care about).
#[derive(Debug)]
enum GgufValue {
    U32(u32),
    I32(i32),
    String(String),
    U64(u64),
    Other,
}

/// Parse GGUF file header to extract model metadata.
/// Self-contained — no Candle dependency.
pub fn parse_gguf_metadata(path: &Path) -> Result<ModelManifest, TiramiError> {
    let mut file =
        std::fs::File::open(path).map_err(|e| TiramiError::ModelLoadError(format!("open: {e}")))?;

    // Magic: "GGUF" (4 bytes)
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|e| TiramiError::ModelLoadError(format!("read magic: {e}")))?;
    if &magic != b"GGUF" {
        return Err(TiramiError::ModelLoadError("not a GGUF file".to_string()));
    }

    // Version (u32 LE)
    let version = read_u32_le(&mut file)?;
    if version < 2 || version > 3 {
        return Err(TiramiError::ModelLoadError(format!(
            "unsupported GGUF version: {version}"
        )));
    }

    // Tensor count (u64 LE)
    let _tensor_count = read_u64_le(&mut file)?;

    // Metadata KV count (u64 LE)
    let kv_count = read_u64_le(&mut file)?;

    // Limit metadata count to prevent DoS from malicious GGUF files
    const MAX_METADATA_KEYS: u64 = 10_000;
    if kv_count > MAX_METADATA_KEYS {
        return Err(TiramiError::ModelLoadError(format!(
            "GGUF metadata count too large: {} (max {})",
            kv_count, MAX_METADATA_KEYS
        )));
    }

    // Read metadata KV pairs
    let mut metadata = std::collections::HashMap::new();
    for _ in 0..kv_count {
        match read_kv_pair(&mut file) {
            Ok((key, value)) => {
                metadata.insert(key, value);
            }
            Err(_) => break, // Stop on parse error, use what we have
        }
    }

    let filename = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    let arch = get_string(&metadata, "general.architecture").unwrap_or_default();

    let total_layers = get_u32(&metadata, &format!("{arch}.block_count"))
        .or_else(|| get_u32(&metadata, "block_count"))
        .unwrap_or(0);

    let hidden_dim = get_u32(&metadata, &format!("{arch}.embedding_length"))
        .or_else(|| get_u32(&metadata, "embedding_length"))
        .unwrap_or(0);

    let vocab_size = get_u32(&metadata, &format!("{arch}.vocab_size"))
        .or_else(|| get_u32(&metadata, "vocab_size"))
        .unwrap_or(0);

    let head_count = get_u32(&metadata, &format!("{arch}.attention.head_count"))
        .or_else(|| get_u32(&metadata, "attention.head_count"))
        .unwrap_or(0);

    let kv_head_count = get_u32(&metadata, &format!("{arch}.attention.head_count_kv"))
        .or_else(|| get_u32(&metadata, "attention.head_count_kv"))
        .unwrap_or(0);

    let context_length = get_u32(&metadata, &format!("{arch}.context_length"))
        .or_else(|| get_u32(&metadata, "context_length"))
        .unwrap_or(0);

    let quantization = get_u32(&metadata, "general.file_type")
        .map(|ft| match ft {
            0 => "F32",
            1 => "F16",
            2 => "Q4_0",
            3 => "Q4_1",
            7 => "Q8_0",
            15 => "Q4_K_M",
            17 => "Q5_K_M",
            18 => "Q6_K",
            _ => "unknown",
        })
        .unwrap_or("unknown")
        .to_string();

    Ok(ModelManifest {
        id: ModelId(filename.to_string()),
        total_layers,
        hidden_dim,
        vocab_size,
        head_count,
        kv_head_count,
        context_length,
        file_size_bytes: file_size,
        quantization,
    })
}

fn read_u32_le(r: &mut impl Read) -> Result<u32, TiramiError> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)
        .map_err(|e| TiramiError::ModelLoadError(format!("read u32: {e}")))?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64_le(r: &mut impl Read) -> Result<u64, TiramiError> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)
        .map_err(|e| TiramiError::ModelLoadError(format!("read u64: {e}")))?;
    Ok(u64::from_le_bytes(buf))
}

fn read_i32_le(r: &mut impl Read) -> Result<i32, TiramiError> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)
        .map_err(|e| TiramiError::ModelLoadError(format!("read i32: {e}")))?;
    Ok(i32::from_le_bytes(buf))
}

fn read_string(r: &mut impl Read) -> Result<String, TiramiError> {
    let len = read_u64_le(r)? as usize;
    if len > 1024 * 1024 {
        return Err(TiramiError::ModelLoadError("string too long".to_string()));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)
        .map_err(|e| TiramiError::ModelLoadError(format!("read string: {e}")))?;
    String::from_utf8(buf).map_err(|e| TiramiError::ModelLoadError(format!("utf8: {e}")))
}

fn read_kv_pair(r: &mut (impl Read + Seek)) -> Result<(String, GgufValue), TiramiError> {
    let key = read_string(r)?;
    let value_type = read_u32_le(r)?;

    let value = match value_type {
        0 => {
            // UINT8
            let mut b = [0u8; 1];
            r.read_exact(&mut b)
                .map_err(|e| TiramiError::ModelLoadError(e.to_string()))?;
            GgufValue::U32(b[0] as u32)
        }
        1 => GgufValue::I32(read_i32_le(r)?), // INT8 (read as i32 for simplicity)
        2 => {
            // UINT16
            let mut buf = [0u8; 2];
            r.read_exact(&mut buf)
                .map_err(|e| TiramiError::ModelLoadError(e.to_string()))?;
            GgufValue::U32(u16::from_le_bytes(buf) as u32)
        }
        3 => {
            // INT16
            let mut buf = [0u8; 2];
            r.read_exact(&mut buf)
                .map_err(|e| TiramiError::ModelLoadError(e.to_string()))?;
            GgufValue::I32(i16::from_le_bytes(buf) as i32)
        }
        4 => GgufValue::U32(read_u32_le(r)?), // UINT32
        5 => GgufValue::I32(read_i32_le(r)?), // INT32
        6 => {
            // FLOAT32
            let _ = read_u32_le(r)?;
            GgufValue::Other
        }
        7 => {
            // BOOL
            let mut b = [0u8; 1];
            r.read_exact(&mut b)
                .map_err(|e| TiramiError::ModelLoadError(e.to_string()))?;
            GgufValue::U32(b[0] as u32)
        }
        8 => GgufValue::String(read_string(r)?), // STRING
        9 => {
            // ARRAY — skip
            let elem_type = read_u32_le(r)?;
            let count = read_u64_le(r)?;
            let elem_size = match elem_type {
                0 | 7 => 1,     // u8, bool
                2 | 3 => 2,     // u16, i16
                4 | 5 | 6 => 4, // u32, i32, f32
                10 | 12 => 8,   // u64, f64
                8 => {
                    // array of strings — skip each
                    for _ in 0..count {
                        let _ = read_string(r)?;
                    }
                    return Ok((key, GgufValue::Other));
                }
                _ => 4,
            };
            let skip = count * elem_size as u64;
            r.seek(SeekFrom::Current(skip as i64))
                .map_err(|e| TiramiError::ModelLoadError(format!("seek: {e}")))?;
            GgufValue::Other
        }
        10 => GgufValue::U64(read_u64_le(r)?), // UINT64
        11 => {
            // INT64
            let _ = read_u64_le(r)?;
            GgufValue::Other
        }
        12 => {
            // FLOAT64
            let _ = read_u64_le(r)?;
            GgufValue::Other
        }
        _ => {
            return Err(TiramiError::ModelLoadError(format!(
                "unknown GGUF value type: {value_type}"
            )));
        }
    };

    Ok((key, value))
}

fn get_u32(metadata: &std::collections::HashMap<String, GgufValue>, key: &str) -> Option<u32> {
    match metadata.get(key)? {
        GgufValue::U32(v) => Some(*v),
        GgufValue::U64(v) => Some(*v as u32),
        GgufValue::I32(v) => Some(*v as u32),
        _ => None,
    }
}

fn get_string(
    metadata: &std::collections::HashMap<String, GgufValue>,
    key: &str,
) -> Option<String> {
    match metadata.get(key)? {
        GgufValue::String(s) => Some(s.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_non_gguf() {
        let result = parse_gguf_metadata(Path::new("/dev/null"));
        assert!(result.is_err());
    }
}
