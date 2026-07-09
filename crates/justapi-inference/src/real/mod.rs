//! Real-model support: quantization configs, LoRA adapters, weight loading.
//!
//! ## Compilation strategy
//!
//! - **`quant.rs` + `lora.rs`** are always compiled and tested in the default
//!   gate (pure data structures, zero weight dependencies).
//! - **`model.rs` + `tokenizer.rs`** are gated behind `#[cfg(feature = "real")]`
//!   and pull in `candle-transformers` + `tokenizers`.

pub mod lora;
pub mod quant;

#[cfg(feature = "real")]
pub mod model;

#[cfg(feature = "real")]
pub mod tokenizer;
