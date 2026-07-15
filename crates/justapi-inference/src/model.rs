//! Model trait, sampling parameters, generated-token stream items, and the
//! CPU-runnable [`MockModel`] used for tests and offline demos.

/// Stopping / sampling configuration for a generation request.
///
/// Mirrors the knobs vLLM/SGLang expose; the real forward pass (Phase 42) will
/// honor `temperature`/`top_p`/`top_k` when sampling logits.
#[derive(Debug, Clone)]
pub struct SamplingParams {
    /// Maximum number of *new* tokens to generate.
    pub max_tokens: usize,
    /// Sampling temperature (0.0 = greedy). Real models only; MockModel ignores it.
    pub temperature: f32,
    /// Nucleus sampling cutoff.
    pub top_p: f32,
    /// Top-k cutoff (0 = disabled).
    pub top_k: usize,
    /// Token ids that, when produced, end generation with `FinishReason::Stop`.
    pub stop_tokens: Vec<u32>,
    /// Substrings that, when produced, end generation (real tokenizers only).
    pub stop_sequences: Vec<String>,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            max_tokens: 256,
            temperature: 1.0,
            top_p: 1.0,
            top_k: 0,
            stop_tokens: Vec::new(),
            stop_sequences: Vec::new(),
        }
    }
}

/// A single streamed token produced during generation.
#[derive(Debug, Clone)]
pub struct GeneratedToken {
    /// Vocabulary id of the token.
    pub id: u32,
    /// Decoded text for the token (may be empty/partial for sub-word pieces).
    pub text: String,
    /// Log-probability of the token (0.0 for [`MockModel`]).
    pub logprob: f32,
    /// Set on the final token of the stream.
    pub finish_reason: Option<FinishReason>,
}

/// Why a generation stream ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    /// Hit `max_tokens`.
    Length,
    /// Produced a stop token / sequence.
    Stop,
    /// Downstream consumer stopped reading (e.g. client disconnected).
    Cancelled,
}

/// Errors produced by the inference engine and models.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("model '{0}' not found in registry")]
    NotFound(String),
    #[error("feature '{0}' is required for this operation (enable it in the crate features)")]
    FeatureRequired(&'static str),
    #[error("real model loading is not implemented yet (lands with the KV-cache in Phase 42)")]
    NotImplemented,
    #[error("generation failed: {0}")]
    Generation(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// A loadable / runnable model behind the engine.
///
/// Implementors must be `Send + Sync` so they can be shared across the registry
/// and driven from a generation thread.
pub trait Model: Send + Sync {
    /// Size of the model vocabulary.
    fn vocab_size(&self) -> usize;

    /// Run generation for `prompt` (token ids), streaming each produced token to
    /// `sink`. Return `false` from `sink` to abort early (e.g. client gone).
    /// Returns the reason the stream ended.
    fn generate(
        &self,
        prompt: &[u32],
        params: &SamplingParams,
        sink: &dyn Fn(GeneratedToken) -> bool,
    ) -> Result<FinishReason, ModelError>;

    /// Return the unnormalized next-token logits for `context` (the raw score
    /// vector over the vocabulary at the position after `context`).
    ///
    /// This is the single-step primitive required for speculative decoding: a
    /// draft model proposes tokens and the target model scores them to accept
    /// or reject. The default implementation derives a degenerate distribution
    /// from [`Model::generate`]'s behavior, but real models should compute true
    /// logits from a single forward pass.
    fn forward_logits(&self, context: &[u32]) -> Result<Vec<f32>, ModelError>;
}

/// Deterministic, weight-free model for tests, demos, and CI on CPU-only boxes.
///
/// It echoes the last prompt token and then walks the vocabulary one id at a
/// time (`(prev + 1) % vocab`), decoding each id to a lowercase letter so the
/// stream is human-readable. It is NOT a real LLM — it exists to exercise the
/// full engine/streaming/registry path end-to-end.
pub struct MockModel {
    vocab_size: usize,
}

impl MockModel {
    /// Create a mock model with the given vocabulary size (clamped to >= 1).
    pub fn new(vocab_size: usize) -> Self {
        Self { vocab_size: vocab_size.max(1) }
    }

    /// Trivial tokenization: each byte of `text` becomes a token id.
    pub fn tokenize(text: &str) -> Vec<u32> {
        text.bytes().map(|b| b as u32).collect()
    }

    /// Trivial detokenization: low byte of each id -> char (lossy for >255).
    pub fn detokenize(ids: &[u32]) -> String {
        ids.iter().map(|&id| (id as u8) as char).collect()
    }
}

impl Model for MockModel {
    fn vocab_size(&self) -> usize {
        self.vocab_size
    }

    fn forward_logits(&self, context: &[u32]) -> Result<Vec<f32>, ModelError> {
        let vocab = self.vocab_size;
        let prev = context.last().copied().unwrap_or(0) % vocab as u32;
        let next = (prev.wrapping_add(1)) % vocab as u32;
        // Degenerate-but-smooth distribution: a sharp peak at the next token so
        // greedy decoding reproduces `generate`'s `(prev + 1) % vocab` rule,
        // with a small uniform tail so temperature/top_p sampling still works.
        let mut logits = vec![-5.0f32; vocab];
        logits[next as usize] = 5.0;
        let r = (prev.wrapping_add(3)) % vocab as u32;
        logits[r as usize] = 0.0;
        Ok(logits)
    }

    fn generate(
        &self,
        prompt: &[u32],
        params: &SamplingParams,
        sink: &dyn Fn(GeneratedToken) -> bool,
    ) -> Result<FinishReason, ModelError> {
        let mut prev = prompt.last().copied().unwrap_or(0);
        for _ in 0..params.max_tokens {
            let next = (prev.wrapping_add(1)) % self.vocab_size as u32;
            let text = decode_mock(next);
            let stop = params.stop_tokens.contains(&next);
            let finish_reason = if stop { Some(FinishReason::Stop) } else { None };
            let token = GeneratedToken { id: next, text, logprob: 0.0, finish_reason };
            if !sink(token) {
                return Ok(FinishReason::Cancelled);
            }
            if stop {
                return Ok(FinishReason::Stop);
            }
            prev = next;
        }
        Ok(FinishReason::Length)
    }
}

/// Map a token id to a single lowercase letter so mock streams are readable.
fn decode_mock(id: u32) -> String {
    let c = (b'a' + (id % 26) as u8) as char;
    c.to_string()
}
