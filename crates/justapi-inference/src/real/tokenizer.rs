//! Thin wrapper around the `tokenizers` crate for decoding token IDs.
//!
//! The real model forward pass produces `Vec<u32>` token ids; this module
//! converts them to text. Only compiled under the `real` feature.

use std::collections::HashMap;

use candle_core::quantized::gguf_file::Value;

use crate::model::ModelError;

/// Wrapper around a HuggingFace `Tokenizer` loaded from a file or built from
/// GGUF metadata.
pub struct Tokenizer {
    inner: tokenizers::Tokenizer,
}

/// Extract an array of strings from GGUF metadata by key.
fn extract_string_array(metadata: &HashMap<String, Value>, key: &str) -> Vec<String> {
    metadata
        .get(key)
        .and_then(|v| match v {
            Value::Array(arr) => {
                Some(arr.iter().filter_map(|x| x.to_string().ok()).cloned().collect())
            }
            _ => None,
        })
        .unwrap_or_default()
}

impl Tokenizer {
    /// Load a tokenizer from a `tokenizer.json` file on disk.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, ModelError> {
        let path = path.as_ref();
        let inner = tokenizers::Tokenizer::from_file(path)
            .map_err(|e| ModelError::Generation(format!("tokenizer load failed: {e}")))?;
        Ok(Self { inner })
    }

    /// Build a tokenizer from GGUF file metadata.
    ///
    /// GGUF files embed tokenizer data (vocab, merges, scores) in their
    /// metadata section under keys like `tokenizer.ggml.model`, `.tokens`,
    /// `.merges`, `.scores`. This function converts that metadata into a
    /// HuggingFace-compatible `tokenizer.json` and loads it in memory.
    pub fn from_gguf_metadata(metadata: &HashMap<String, Value>) -> Result<Self, ModelError> {
        // GGUF "gpt2" / "llama" tokenizers are GPT-2-style BPE.
        let tokens: Vec<String> = extract_string_array(metadata, "tokenizer.ggml.tokens");
        let merges: Vec<String> = extract_string_array(metadata, "tokenizer.ggml.merges");

        // Build the HuggingFace tokenizer JSON structure.
        let mut vocab = serde_json::Map::new();
        for (i, token) in tokens.iter().enumerate() {
            vocab.insert(token.clone(), serde_json::Value::Number(i.into()));
        }

        // GPT-2 / Llama BPE uses ByteLevel pre-tokenizer + decoder and the
        // `Ġ` sentinel for spaces in the vocab.
        let model_json = serde_json::json!({
            "type": "BPE",
            "dropout": null,
            "unk_token": null,
            "continuing_subword_prefix": null,
            "end_of_word_suffix": "",
            "fuse_unk": false,
            "byte_fallback": true,
            "vocab": vocab,
            "merges": merges,
        });

        let pre_tokenizer = serde_json::json!({
            "type": "ByteLevel",
            "add_prefix_space": false,
            "trim_offsets": false,
        });

        let decoder = serde_json::json!({
            "type": "ByteLevel",
            "add_prefix_space": false,
            "trim_offsets": false,
        });

        let _bos = metadata.get("tokenizer.ggml.bos_token_id").and_then(|v| v.to_u32().ok());
        let _eos = metadata.get("tokenizer.ggml.eos_token_id").and_then(|v| v.to_u32().ok());

        // BOS/EOS injection is handled by the model's generate path, not the
        // HuggingFace tokenizer post-processor, so we omit it here to keep the
        // GGUF-derived tokenizer minimal and loadable.
        let post_processor = serde_json::Value::Null;

        let tokenizer_json = serde_json::json!({
            "version": "1.0",
            "truncation": null,
            "padding": null,
            "added_tokens": [],
            "normalizer": null,
            "pre_tokenizer": pre_tokenizer,
            "post_processor": post_processor,
            "decoder": decoder,
            "model": model_json,
        });

        let bytes = serde_json::to_vec(&tokenizer_json)
            .map_err(|e| ModelError::Generation(format!("tokenizer json serialize: {e}")))?;

        let inner = tokenizers::Tokenizer::from_bytes(&bytes)
            .map_err(|e| ModelError::Generation(format!("tokenizer from gguf metadata: {e}")))?;

        Ok(Self { inner })
    }

    /// Encode text into token IDs (truncation/padding disabled).
    pub fn encode(&self, text: &str) -> Result<Vec<u32>, ModelError> {
        let encoding = self
            .inner
            .encode(text, true)
            .map_err(|e| ModelError::Generation(format!("tokenizer encode failed: {e}")))?;
        Ok(encoding.get_ids().to_vec())
    }

    /// Decode a sequence of token IDs to text.
    pub fn decode(&self, ids: &[u32]) -> Result<String, ModelError> {
        self.inner
            .decode(ids, true)
            .map_err(|e| ModelError::Generation(format!("tokenizer decode failed: {e}")))
    }

    /// Decode a single token ID to text.
    pub fn decode_token(&self, id: u32) -> Result<String, ModelError> {
        self.decode(&[id])
    }

    /// Vocabulary size.
    pub fn vocab_size(&self) -> usize {
        self.inner.get_vocab_size(true)
    }
}
