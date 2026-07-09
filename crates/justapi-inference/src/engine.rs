//! The streaming [`Engine`]: device selection, an in-memory [`ModelRegistry`],
//! and token streaming via a tokio channel. Generation runs on a dedicated OS
//! thread so the hot path stays GIL-free.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use candle_core::Device;
use tokio::sync::mpsc;

use crate::model::{GeneratedToken, MockModel, Model, ModelError, SamplingParams};
use crate::spec_decode::{SpeculativeConfig, SpeculativeModel};
use crate::spec_decode_tree::TreeSpeculativeModel;

/// Compute device a model runs on. Mirrors Candle's [`Device`] minus the ROCm
/// variant (candle 0.11 enables AMD GPUs via the `cuda` feature on a ROCm
/// toolchain).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineDevice {
    Cpu,
    Cuda(usize),
    Metal(usize),
}

impl EngineDevice {
    /// Convert to a Candle [`Device`]. GPU variants are constructed via
    /// `new_cuda`/`new_metal` (which validate the ordinal exists).
    pub fn to_candle(self) -> candle_core::Result<Device> {
        match self {
            EngineDevice::Cpu => Ok(Device::Cpu),
            EngineDevice::Cuda(i) => Device::new_cuda(i),
            EngineDevice::Metal(i) => Device::new_metal(i),
        }
    }

    /// Best-effort discovery of available devices.
    ///
    /// CPU is always present. GPUs are probed by attempting to construct the
    /// device (cheap on a properly configured host; errors are ignored).
    pub fn discover() -> Vec<EngineDevice> {
        let mut devices = vec![EngineDevice::Cpu];
        for i in 0..8 {
            if Device::new_cuda(i).is_ok() {
                devices.push(EngineDevice::Cuda(i));
            }
        }
        for i in 0..8 {
            if Device::new_metal(i).is_ok() {
                devices.push(EngineDevice::Metal(i));
            }
        }
        devices
    }
}

/// Where a registered model came from.
#[derive(Debug, Clone)]
pub enum ModelSource {
    /// Weight-free model (tests, demos).
    Mock,
    /// Loaded from a directory of weights (path recorded).
    Real(std::path::PathBuf),
}

struct RegistryEntry {
    model: Arc<dyn Model>,
    source: ModelSource,
}

/// In-memory collection of loaded models, keyed by name.
#[derive(Default)]
pub struct ModelRegistry {
    models: HashMap<String, RegistryEntry>,
}

impl ModelRegistry {
    /// Register a model under `name`, replacing any existing entry.
    pub fn register(&mut self, name: &str, model: Arc<dyn Model>, source: ModelSource) {
        self.models
            .insert(name.to_string(), RegistryEntry { model, source });
    }

    /// Fetch a shared handle to a registered model.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Model>> {
        self.models.get(name).map(|e| e.model.clone())
    }

    /// List registered model names.
    pub fn list(&self) -> Vec<String> {
        self.models.keys().cloned().collect()
    }

    /// Number of registered models.
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

/// The inference engine: owns the device and the model registry, and streams
/// generation output back to callers over a channel.
pub struct Engine {
    device: Device,
    registry: Arc<Mutex<ModelRegistry>>,
}

impl Engine {
    /// Create an engine bound to `device`.
    pub fn new(device: EngineDevice) -> Result<Self, ModelError> {
        let device = device
            .to_candle()
            .map_err(|e| ModelError::Generation(e.to_string()))?;
        Ok(Self {
            device,
            registry: Arc::new(Mutex::new(ModelRegistry::default())),
        })
    }

    /// The Candle device this engine runs models on.
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Register an arbitrary model implementation.
    pub fn register(&self, name: &str, model: Arc<dyn Model>) {
        self.registry
            .lock()
            .unwrap()
            .register(name, model, ModelSource::Mock);
    }

    /// Fetch a shared handle to a registered model by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Model>> {
        self.registry.lock().unwrap().get(name)
    }

    /// Register a [`MockModel`] and return a handle to it.
    pub fn register_mock(&self, name: &str) -> Arc<MockModel> {
        let model = Arc::new(MockModel::new(crate::DEFAULT_MOCK_VOCAB));
        self.registry
            .lock()
            .unwrap()
            .register(name, model.clone(), ModelSource::Mock);
        model
    }

    /// Register a speculative-decode model: a `target` model served with a
    /// `draft` model proposing `gamma` candidate tokens per step. The wrapped
    /// [`SpeculativeModel`] is registered under `name` and served through the
    /// normal [`Engine::generate`] path transparently.
    pub fn register_speculative(
        &self,
        name: &str,
        target: Arc<dyn Model>,
        draft: Arc<dyn Model>,
        gamma: usize,
        seed: u64,
    ) {
        let spec = SpeculativeModel::new(target, SpeculativeConfig::new(gamma, draft, seed));
        self.registry
            .lock()
            .unwrap()
            .register(name, Arc::new(spec), ModelSource::Mock);
    }

    /// Register a tree-based speculative-decode model (Medusa/EAGLE-style): a
    /// `draft` model proposes a tree of `branch` candidates at each of `gamma`
    /// positions, and the `target` verifies the longest matching path. The
    /// wrapped [`TreeSpeculativeModel`] is registered under `name` and served
    /// through the normal [`Engine::generate`] path transparently — callers get
    /// the same token stream as plain target decode, just faster.
    pub fn register_tree_speculative(
        &self,
        name: &str,
        target: Arc<dyn Model>,
        draft: Arc<dyn Model>,
        gamma: usize,
        branch: usize,
        seed: u64,
    ) {
        let spec = TreeSpeculativeModel::new(target, draft, gamma, branch, seed);
        self.registry
            .lock()
            .unwrap()
            .register(name, Arc::new(spec), ModelSource::Mock);
    }

    /// Load a real model from `model_dir`.
    ///
    /// Behind the `real` feature (and with weights + a GPU toolkit) this will
    /// build the candle-transformers forward pass + KV cache (Phase 42). Until
    /// then it returns a clear error so callers don't silently get a no-op.
    pub fn load(&self, name: &str, model_dir: &Path) -> Result<(), ModelError> {
        #[cfg(feature = "real")]
        {
            use crate::real::model::RealModel;
            let model = RealModel::load(model_dir, self.device.clone())
                .map_err(|e| ModelError::Generation(e.to_string()))?;
            self.registry.lock().unwrap().register(
                name,
                Arc::new(model),
                ModelSource::Real(model_dir.to_path_buf()),
            );
            Ok(())
        }
        #[cfg(not(feature = "real"))]
        {
            let _ = (name, model_dir);
            Err(ModelError::FeatureRequired("real"))
        }
    }

    /// Generate tokens for `prompt` with `params`, streaming them to the
    /// returned receiver as they are produced.
    ///
    /// Generation runs on a dedicated OS thread (Candle compute is synchronous);
    /// the caller receives tokens asynchronously with zero GIL involvement.
    pub fn generate(
        &self,
        name: &str,
        prompt: &[u32],
        params: SamplingParams,
    ) -> Result<mpsc::UnboundedReceiver<GeneratedToken>, ModelError> {
        let model = {
            let reg = self.registry.lock().unwrap();
            reg.get(name)
                .ok_or_else(|| ModelError::NotFound(name.to_string()))?
        };

        let (tx, rx) = mpsc::unbounded_channel();
        let prompt = prompt.to_vec();
        std::thread::spawn(move || {
            let _ = model.generate(&prompt, &params, &|token| tx.send(token).is_ok());
        });
        Ok(rx)
    }

    /// List registered model names.
    pub fn list_models(&self) -> Vec<String> {
        self.registry.lock().unwrap().list()
    }

    /// Return the [`ModelSource`] a registered model was loaded from, if known.
    pub fn model_source(&self, name: &str) -> Option<ModelSource> {
        self.registry
            .lock()
            .unwrap()
            .models
            .get(name)
            .map(|e| e.source.clone())
    }
}
