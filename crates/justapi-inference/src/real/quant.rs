//! Quantization configuration and method selection.
//!
//! Pure data structures — no weight loading, no Candle tensors. These types are
//! always compiled so the scheduler can make quant-aware decisions (e.g. which
//! adapter to route to) without pulling in heavy dependencies.

use std::collections::HashMap;

/// Supported quantization backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuantMethod {
    /// No quantization (f16/bf16/f32).
    None,
    /// GPTQ (Marlin kernel on CUDA, CPU fallback via unpacking).
    Gptq,
    /// AWQ (activation-aware quantization).
    Awq,
    /// GGUF k-quants (Q2_K through Q8_0, IQ variants).
    Gguf,
    /// EXL2 (ExLlamaV2 kernel).
    Exl2,
    /// BitsAndBytes (8-bit / 4-bit, widely used in HuggingFace).
    BitsAndBytes,
}

impl QuantMethod {
    /// Human-readable label for logs and metrics.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gptq => "gptq",
            Self::Awq => "awq",
            Self::Gguf => "gguf",
            Self::Exl2 => "exl2",
            Self::BitsAndBytes => "bnb",
        }
    }

    /// Approximate bits-per-weight for display / heuristics.
    pub fn approx_bits(&self) -> f32 {
        match self {
            Self::None => 16.0,
            Self::Gptq => 4.0,
            Self::Awq => 4.0,
            Self::Gguf => 4.5,
            Self::Exl2 => 3.5,
            Self::BitsAndBytes => 8.0,
        }
    }
}

/// Per-layer quantization metadata (for mixed-precision models).
#[derive(Debug, Clone)]
pub struct LayerQuantConfig {
    pub method: QuantMethod,
    /// Group size used by the quantization kernel (0 = per-tensor).
    pub group_size: u32,
    /// Symmetric quantization flag.
    pub symmetric: bool,
}

/// Full quantization descriptor for a model or a specific adapter.
#[derive(Debug, Clone)]
pub struct QuantConfig {
    /// Global quantization method.
    pub method: QuantMethod,
    /// Per-layer overrides. Keys are layer name prefixes (e.g. "model.layers.0").
    pub layers: HashMap<String, LayerQuantConfig>,
    /// Bits of precision (informational; actual kernels depend on `method`).
    pub bits: f32,
}

impl QuantConfig {
    /// Build a uniform quant config (same method for all layers).
    pub fn uniform(method: QuantMethod, bits: f32) -> Self {
        Self {
            method,
            layers: HashMap::new(),
            bits,
        }
    }

    /// Effective method for a specific layer (falls back to global).
    pub fn method_for_layer(&self, layer_name: &str) -> QuantMethod {
        // Try exact match first, then prefix match (longest prefix wins).
        let mut best: Option<&LayerQuantConfig> = None;
        let mut best_len = 0usize;
        for (prefix, cfg) in &self.layers {
            if layer_name.starts_with(prefix.as_str()) && prefix.len() > best_len {
                best = Some(cfg);
                best_len = prefix.len();
            }
        }
        best.map(|c| c.method).unwrap_or(self.method)
    }

    /// Rough memory estimate in bytes for a model with `param_count` parameters.
    pub fn memory_bytes(&self, param_count: u64) -> u64 {
        let bits = self.bits.clamp(1.0, 32.0);
        param_count * bits as u64 / 8
    }
}

/// GGUF k-quant type mapping (used when loading .gguf files).
pub mod gguf {
    use super::QuantMethod;

    /// GGUF quantization type IDs (subset used by llama.cpp / candle).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(u32)]
    #[allow(non_camel_case_types)]
    pub enum GgufType {
        F32 = 0,
        F16 = 1,
        Q4_0 = 2,
        Q4_1 = 3,
        Q5_0 = 6,
        Q5_1 = 7,
        Q8_0 = 8,
        Q8_1 = 9,
        Q2_K = 10,
        Q3_K_S = 11,
        Q3_K_M = 12,
        Q3_K_L = 13,
        Q4_K_S = 14,
        Q4_K_M = 15,
        Q5_K_S = 16,
        Q5_K_M = 17,
        Q6_K = 18,
        IQ2_XXS = 19,
        IQ2_XS = 20,
        IQ3_XXS = 21,
        IQ1_S = 22,
        IQ4_NL = 23,
        IQ3_S = 24,
        IQ2_S = 25,
        IQ4_XS = 26,
    }

    /// Approximate bits-per-weight for a GGUF type.
    pub fn bits_for_type(t: GgufType) -> f32 {
        match t {
            GgufType::F32 => 32.0,
            GgufType::F16 => 16.0,
            GgufType::Q4_0 | GgufType::Q4_1 => 4.5,
            GgufType::Q5_0 | GgufType::Q5_1 => 5.5,
            GgufType::Q8_0 | GgufType::Q8_1 => 8.5,
            GgufType::Q2_K => 2.5,
            GgufType::Q3_K_S | GgufType::Q3_K_M | GgufType::Q3_K_L => 3.5,
            GgufType::Q4_K_S | GgufType::Q4_K_M => 4.5,
            GgufType::Q5_K_S | GgufType::Q5_K_M => 5.5,
            GgufType::Q6_K => 6.5,
            GgufType::IQ2_XXS | GgufType::IQ2_XS | GgufType::IQ2_S => 2.0,
            GgufType::IQ3_XXS | GgufType::IQ3_S => 3.0,
            GgufType::IQ1_S => 1.5,
            GgufType::IQ4_NL | GgufType::IQ4_XS => 4.0,
        }
    }

    /// All supported quant type IDs (for validation).
    pub fn supported_ids() -> &'static [u32] {
        &[
            0, 1, 2, 3, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
            26,
        ]
    }

    /// Whether a GGUF type ID is supported.
    pub fn is_supported(id: u32) -> bool {
        supported_ids().contains(&id)
    }

    /// Map a GGUF type to our QuantMethod.
    pub fn to_quant_method(t: GgufType) -> QuantMethod {
        match t {
            GgufType::F32 | GgufType::F16 => QuantMethod::None,
            _ => QuantMethod::Gguf,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quant_method_as_str_and_bits() {
        assert_eq!(QuantMethod::None.as_str(), "none");
        assert_eq!(QuantMethod::None.approx_bits(), 16.0);
        assert_eq!(QuantMethod::Gptq.approx_bits(), 4.0);
    }

    #[test]
    fn quant_config_uniform() {
        let cfg = QuantConfig::uniform(QuantMethod::Gptq, 4.0);
        assert_eq!(cfg.method, QuantMethod::Gptq);
        assert!(cfg.layers.is_empty());
        assert_eq!(cfg.method_for_layer("anything"), QuantMethod::Gptq);
    }

    #[test]
    fn quant_config_layer_override() {
        let mut cfg = QuantConfig::uniform(QuantMethod::Gptq, 4.0);
        cfg.layers.insert(
            "model.layers.0".to_string(),
            LayerQuantConfig {
                method: QuantMethod::None,
                group_size: 128,
                symmetric: true,
            },
        );
        assert_eq!(cfg.method_for_layer("model.layers.0"), QuantMethod::None);
        assert_eq!(cfg.method_for_layer("model.layers.1"), QuantMethod::Gptq);
    }

    #[test]
    fn quant_config_longest_prefix_wins() {
        let mut cfg = QuantConfig::uniform(QuantMethod::Awq, 4.0);
        cfg.layers.insert(
            "model.layers".to_string(),
            LayerQuantConfig {
                method: QuantMethod::Gptq,
                group_size: 0,
                symmetric: false,
            },
        );
        cfg.layers.insert(
            "model.layers.0.self_attn".to_string(),
            LayerQuantConfig {
                method: QuantMethod::None,
                group_size: 128,
                symmetric: true,
            },
        );
        assert_eq!(
            cfg.method_for_layer("model.layers.0.self_attn.q_proj"),
            QuantMethod::None
        );
        assert_eq!(
            cfg.method_for_layer("model.layers.1.ffn"),
            QuantMethod::Gptq
        );
    }

    #[test]
    fn memory_estimate() {
        let cfg = QuantConfig::uniform(QuantMethod::Gptq, 4.0);
        let bytes = cfg.memory_bytes(7_000_000_000); // 7B params
        assert_eq!(bytes, 3_500_000_000); // 4 bits = 0.5 bytes
    }

    #[test]
    fn gguf_bits_and_support() {
        assert_eq!(gguf::bits_for_type(gguf::GgufType::F32), 32.0);
        assert!(gguf::is_supported(gguf::GgufType::Q4_K_M as u32));
        assert!(!gguf::is_supported(999));
    }

    #[test]
    fn gguf_to_quant_method() {
        assert_eq!(
            gguf::to_quant_method(gguf::GgufType::F16),
            QuantMethod::None
        );
        assert_eq!(
            gguf::to_quant_method(gguf::GgufType::Q4_K_M),
            QuantMethod::Gguf
        );
    }
}
