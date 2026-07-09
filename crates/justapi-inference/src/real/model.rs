//! Real model loading and forward pass — GGUF and safetensors.
//!
//! Only compiled under `#[cfg(feature = "real")]`. Pulls in `candle-transformers`
//! for the actual Llama forward pass and `tokenizers` for decoding.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use candle_core::quantized::gguf_file;
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::llama::{Cache, Config as LlamaConfig, Llama};
use candle_transformers::models::quantized_llama::ModelWeights;

use crate::model::{FinishReason, GeneratedToken, Model, ModelError, SamplingParams};
use crate::real::tokenizer::Tokenizer;

/// Returns true if `path` looks like a valid GGUF file (correct magic header).
fn is_valid_gguf(path: &Path) -> bool {
    let mut buf = [0u8; 4];
    match std::fs::File::open(path).and_then(|mut f| std::io::Read::read_exact(&mut f, &mut buf)) {
        Ok(()) => &buf == b"GGUF",
        Err(_) => false,
    }
}

/// Detect whether a model directory contains a valid `.gguf` file or safetensors.
pub fn detect_format(model_dir: &Path) -> Result<ModelFormat, ModelError> {
    let entries: Vec<_> = std::fs::read_dir(model_dir)
        .map_err(ModelError::Io)?
        .filter_map(|e| e.ok())
        .collect();

    let has_gguf = entries
        .iter()
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "gguf")
                .unwrap_or(false)
        })
        .any(|e| is_valid_gguf(&e.path()));

    let has_safetensors = entries.iter().any(|e| {
        e.path()
            .extension()
            .map(|ext| ext == "safetensors")
            .unwrap_or(false)
    });

    match (has_gguf, has_safetensors) {
        (true, _) => Ok(ModelFormat::Gguf),
        (_, true) => Ok(ModelFormat::Safetensors),
        _ => Err(ModelError::Generation(format!(
            "no .gguf or .safetensors files found in {}",
            model_dir.display()
        ))),
    }
}

/// Detected model weight format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelFormat {
    Gguf,
    Safetensors,
}

/// Raw Llama config.json structure (safetensors models ship a separate config).
#[derive(Debug, serde::Deserialize)]
struct RawLlamaConfig {
    hidden_size: usize,
    intermediate_size: usize,
    num_attention_heads: usize,
    num_hidden_layers: usize,
    num_key_value_heads: usize,
    vocab_size: usize,
    rms_norm_eps: Option<f64>,
    rope_theta: Option<f32>,
    max_position_embeddings: Option<usize>,
    #[serde(default)]
    tie_word_embeddings: bool,
}

impl RawLlamaConfig {
    fn to_candle_config(&self) -> LlamaConfig {
        LlamaConfig {
            hidden_size: self.hidden_size,
            intermediate_size: self.intermediate_size,
            num_attention_heads: self.num_attention_heads,
            num_hidden_layers: self.num_hidden_layers,
            num_key_value_heads: self.num_key_value_heads,
            vocab_size: self.vocab_size,
            rms_norm_eps: self.rms_norm_eps.unwrap_or(1e-5),
            rope_theta: self.rope_theta.unwrap_or(10000.0),
            max_position_embeddings: self.max_position_embeddings.unwrap_or(2048),
            tie_word_embeddings: self.tie_word_embeddings,
            bos_token_id: None,
            eos_token_id: None,
            rope_scaling: None,
            use_flash_attn: false,
        }
    }
}

/// Mutable forward-pass state. Wrapped in a `Mutex` so `RealModel::generate`
/// can hold a `&self` borrow (required by the [`Model`] trait) while driving the
/// autoregressive loop. The GGUF path keeps its KV cache inside [`ModelWeights`];
/// the safetensors path keeps an explicit [`Cache`].
enum ForwardState {
    Gguf(ModelWeights),
    Safetensors { model: Llama, cache: Cache },
}

impl ForwardState {
    /// Run one forward step over `tokens` at absolute `index_pos`.
    fn forward(
        &mut self,
        tokens: &[u32],
        index_pos: usize,
        device: &Device,
    ) -> Result<Tensor, ModelError> {
        let t = Tensor::from_slice(tokens, (1, tokens.len()), device)
            .map_err(|e| ModelError::Generation(e.to_string()))?;
        match self {
            ForwardState::Gguf(m) => m
                .forward(&t, index_pos)
                .map_err(|e| ModelError::Generation(e.to_string())),
            ForwardState::Safetensors { model, cache } => model
                .forward(&t, index_pos, cache)
                .map_err(|e| ModelError::Generation(e.to_string())),
        }
    }

    /// Reset KV state before a fresh sequence.
    fn reset_cache(&mut self, config: &LlamaConfig, device: &Device) -> Result<(), ModelError> {
        match self {
            ForwardState::Gguf(m) => m.clear_kv_cache(),
            ForwardState::Safetensors { cache, .. } => {
                *cache = Cache::new(true, DType::F32, config, device)
                    .map_err(|e| ModelError::Generation(e.to_string()))?;
            }
        }
        Ok(())
    }
}

/// A loaded real model (GGUF or safetensors) implementing the `Model` trait.
pub struct RealModel {
    format: ModelFormat,
    config: LlamaConfig,
    device: Device,
    /// Tokenizer for decoding generated token IDs to text.
    tokenizer: Tokenizer,
    /// Forward-pass state (mutated during generation).
    state: Mutex<ForwardState>,
}

impl RealModel {
    /// Load a real model from `model_dir`.
    ///
    /// Expects:
    /// - `tokenizer.json` — HuggingFace tokenizer file
    /// - Either `*.gguf` (GGUF format) or `*.safetensors` + `config.json`
    pub fn load(model_dir: &Path, device: Device) -> Result<Self, ModelError> {
        let format = detect_format(model_dir)?;
        let tokenizer_path = model_dir.join("tokenizer.json");
        let tokenizer = if tokenizer_path.exists() {
            Tokenizer::from_file(&tokenizer_path)?
        } else {
            // For GGUF files, try to extract the tokenizer from metadata.
            if format == ModelFormat::Gguf {
                // First load the GGUF metadata.
                let entries: Vec<_> = std::fs::read_dir(model_dir)
                    .map_err(ModelError::Io)?
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .collect();

                let gguf_path = entries
                    .iter()
                    .find(|p| {
                        p.extension().map(|e| e == "gguf").unwrap_or(false) && is_valid_gguf(p)
                    })
                    .ok_or_else(|| {
                        ModelError::Generation("no valid .gguf file found".to_string())
                    })?;

                let mut file = File::open(gguf_path).map_err(ModelError::Io)?;
                let ct = gguf_file::Content::read(&mut file)
                    .map_err(|e| ModelError::Generation(format!("gguf read failed: {e}")))?;

                Tokenizer::from_gguf_metadata(&ct.metadata)?
            } else {
                return Err(ModelError::Generation(
                    "tokenizer.json not found — safetensors models require tokenizer.json"
                        .to_string(),
                ));
            }
        };

        let entries: Vec<_> = std::fs::read_dir(model_dir)
            .map_err(ModelError::Io)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();

        // Build the Llama config (needed by both formats for the safetensors
        // forward pass and as a fallback for GGUF).
        let config = load_config(model_dir, &entries)?;

        let state = match format {
            ModelFormat::Gguf => {
                let gguf_path = entries
                    .iter()
                    .find(|p| {
                        p.extension().map(|e| e == "gguf").unwrap_or(false) && is_valid_gguf(p)
                    })
                    .ok_or_else(|| {
                        ModelError::Generation("no valid .gguf file found".to_string())
                    })?;
                let mut file = File::open(gguf_path).map_err(ModelError::Io)?;
                let ct = gguf_file::Content::read(&mut file)
                    .map_err(|e| ModelError::Generation(format!("gguf read failed: {e}")))?;
                let model = ModelWeights::from_gguf(ct, &mut file, &device)
                    .map_err(|e| ModelError::Generation(format!("gguf load failed: {e}")))?;
                ForwardState::Gguf(model)
            }
            ModelFormat::Safetensors => {
                let st_paths: Vec<PathBuf> = entries
                    .iter()
                    .filter(|p| p.extension().map(|e| e == "safetensors").unwrap_or(false))
                    .cloned()
                    .collect();
                let vb =
                    unsafe { VarBuilder::from_mmaped_safetensors(&st_paths, DType::F32, &device) }
                        .map_err(|e| {
                            ModelError::Generation(format!("safetensors load failed: {e}"))
                        })?;
                let model = Llama::load(vb, &config)
                    .map_err(|e| ModelError::Generation(format!("llama load failed: {e}")))?;
                let cache = Cache::new(true, DType::F32, &config, &device)
                    .map_err(|e| ModelError::Generation(format!("cache init failed: {e}")))?;
                ForwardState::Safetensors { model, cache }
            }
        };

        tracing::info!(
            "loaded {:?} model: hidden={}, layers={}, heads={}, kv_heads={}, vocab={}",
            format,
            config.hidden_size,
            config.num_hidden_layers,
            config.num_attention_heads,
            config.num_key_value_heads,
            config.vocab_size,
        );

        Ok(Self {
            format,
            config,
            device,
            tokenizer,
            state: Mutex::new(state),
        })
    }

    /// The detected format.
    pub fn format(&self) -> ModelFormat {
        self.format
    }

    /// The loaded Llama config.
    pub fn config(&self) -> &LlamaConfig {
        &self.config
    }

    /// Tokenize text into token IDs using the model's tokenizer.
    pub fn encode(&self, text: &str) -> Result<Vec<u32>, ModelError> {
        self.tokenizer.encode(text)
    }
}

/// Load the Llama config: from `config.json` if present, else a sensible default.
fn load_config(model_dir: &Path, entries: &[PathBuf]) -> Result<LlamaConfig, ModelError> {
    let has_config = entries
        .iter()
        .any(|p| p.file_name().map(|n| n == "config.json").unwrap_or(false));
    if has_config {
        let config_str =
            std::fs::read_to_string(model_dir.join("config.json")).map_err(ModelError::Io)?;
        let raw: RawLlamaConfig = serde_json::from_str(&config_str)
            .map_err(|e| ModelError::Generation(format!("config.json parse error: {e}")))?;
        Ok(raw.to_candle_config())
    } else {
        // Minimal default — GGUF metadata overrides tensor dims at load time.
        Ok(LlamaConfig {
            hidden_size: 4096,
            intermediate_size: 11008,
            num_attention_heads: 32,
            num_hidden_layers: 32,
            num_key_value_heads: 32,
            vocab_size: 32000,
            rms_norm_eps: 1e-5,
            rope_theta: 10000.0,
            max_position_embeddings: 2048,
            tie_word_embeddings: false,
            bos_token_id: None,
            eos_token_id: None,
            rope_scaling: None,
            use_flash_attn: false,
        })
    }
}

/// Build a [`LogitsProcessor`] honoring temperature / top_p / top_k.
fn build_processor(seed: u64, params: &SamplingParams) -> LogitsProcessor {
    if params.top_k > 0 {
        LogitsProcessor::from_sampling(
            seed,
            Sampling::TopKThenTopP {
                k: params.top_k,
                p: params.top_p as f64,
                temperature: params.temperature as f64,
            },
        )
    } else {
        let temperature = if params.temperature < 1e-7 {
            None
        } else {
            Some(params.temperature as f64)
        };
        let top_p = if params.top_p >= 1.0 {
            None
        } else {
            Some(params.top_p as f64)
        };
        LogitsProcessor::new(seed, temperature, top_p)
    }
}

impl Model for RealModel {
    fn vocab_size(&self) -> usize {
        self.config.vocab_size
    }

    fn forward_logits(&self, context: &[u32]) -> Result<Vec<f32>, ModelError> {
        let mut state = self.state.lock().unwrap();
        state.reset_cache(&self.config, &self.device)?;
        let logits = state.forward(context, 0, &self.device)?;
        // `to_vec1` returns the last position's logits (shape [vocab]).
        let vec = logits
            .to_vec1::<f32>()
            .map_err(|e| ModelError::Generation(e.to_string()))?;
        Ok(vec)
    }

    #[allow(clippy::explicit_counter_loop)]
    fn generate(
        &self,
        prompt: &[u32],
        params: &SamplingParams,
        sink: &dyn Fn(GeneratedToken) -> bool,
    ) -> Result<FinishReason, ModelError> {
        // Non-deterministic seed so repeated calls vary (temperature/top_p).
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(299792458);
        let mut lp = build_processor(seed, params);

        let mut state = self.state.lock().unwrap();
        state.reset_cache(&self.config, &self.device)?;

        // Prefill: feed the whole prompt at once, get logits for the last position.
        // `index_pos` tracks the absolute position in the KV cache (prompt length
        // + number of tokens already decoded) — it is NOT a redundant copy of the
        // `step` loop counter, so the explicit counter is intentional.
        let logits = state.forward(prompt, 0, &self.device)?;
        let mut token = lp
            .sample(&logits)
            .map_err(|e| ModelError::Generation(e.to_string()))?;
        let mut index_pos = prompt.len();

        for step in 0..params.max_tokens {
            let text = self
                .tokenizer
                .decode(&[token])
                .map_err(|e| ModelError::Generation(e.to_string()))?;
            let is_stop = params.stop_tokens.contains(&token);
            let finish = if is_stop {
                Some(FinishReason::Stop)
            } else {
                None
            };
            let produced = GeneratedToken {
                id: token,
                text,
                logprob: 0.0,
                finish_reason: finish,
            };
            if !sink(produced) {
                return Ok(FinishReason::Cancelled);
            }
            if is_stop {
                return Ok(FinishReason::Stop);
            }
            // Produced the final token → done.
            if step + 1 >= params.max_tokens {
                return Ok(FinishReason::Length);
            }
            // Decode the next token, extending the KV cache.
            let next_logits = state.forward(&[token], index_pos, &self.device)?;
            token = lp
                .sample(&next_logits)
                .map_err(|e| ModelError::Generation(e.to_string()))?;
            index_pos += 1;
        }
        Ok(FinishReason::Length)
    }
}

// ---------------------------------------------------------------------------
// Tests (feature-gated `real`)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_format_gguf() {
        let dir = std::env::temp_dir().join("justapi_test_gguf");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("model.gguf"), b"GGUF");
        let fmt = detect_format(&dir).unwrap();
        assert_eq!(fmt, ModelFormat::Gguf);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_format_safetensors() {
        let dir = std::env::temp_dir().join("justapi_test_st");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("model.safetensors"), b"{}");
        let _ = std::fs::write(dir.join("config.json"), b"{}");
        let fmt = detect_format(&dir).unwrap();
        assert_eq!(fmt, ModelFormat::Safetensors);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_format_empty_errors() {
        let dir = std::env::temp_dir().join("justapi_test_empty");
        let _ = std::fs::create_dir_all(&dir);
        assert!(detect_format(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn raw_config_parses_and_maps() {
        let json = r#"{
            "hidden_size": 768,
            "intermediate_size": 2048,
            "num_attention_heads": 12,
            "num_hidden_layers": 12,
            "num_key_value_heads": 12,
            "vocab_size": 50257,
            "rms_norm_eps": 1e-6,
            "rope_theta": 10000.0,
            "max_position_embeddings": 1024
        }"#;
        let raw: RawLlamaConfig = serde_json::from_str(json).unwrap();
        let cfg = raw.to_candle_config();
        assert_eq!(cfg.hidden_size, 768);
        assert_eq!(cfg.num_hidden_layers, 12);
        assert_eq!(cfg.vocab_size, 50257);
        assert_eq!(cfg.max_position_embeddings, 1024);
        assert!(cfg.rope_scaling.is_none());
        assert!(!cfg.use_flash_attn);
    }

    #[test]
    fn raw_config_defaults_for_optional_fields() {
        let json = r#"{
            "hidden_size": 4096,
            "intermediate_size": 11008,
            "num_attention_heads": 32,
            "num_hidden_layers": 32,
            "num_key_value_heads": 32,
            "vocab_size": 32000
        }"#;
        let raw: RawLlamaConfig = serde_json::from_str(json).unwrap();
        let cfg = raw.to_candle_config();
        assert_eq!(cfg.rms_norm_eps, 1e-5);
        assert_eq!(cfg.rope_theta, 10000.0);
        assert_eq!(cfg.max_position_embeddings, 2048);
        assert!(!cfg.tie_word_embeddings);
    }
}
