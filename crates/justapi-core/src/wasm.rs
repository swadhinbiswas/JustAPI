use anyhow::{Context, Result};
use wasmtime::*;

pub struct WasmEngine {
    engine: Engine,
    module: Module,
}

impl WasmEngine {
    pub fn new(wasm_bytes: &[u8]) -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);

        let engine = Engine::new(&config)?;
        let module = Module::new(&engine, wasm_bytes)?;

        Ok(Self { engine, module })
    }

    /// Evaluates the middleware on the request headers/metadata.
    /// In a real implementation this would use WIT (Wasm Interface Types) or a structured memory exchange.
    /// For this phase, we simply pass a JSON string of headers into WASM memory and read a JSON response.
    pub async fn execute_middleware(&self, request_json: &str) -> Result<String> {
        let mut store = Store::new(&self.engine, ());
        store.set_fuel(10_000_000)?; // Prevent infinite loops

        let mut linker = Linker::new(&self.engine);
        // Link host functions if needed (e.g., logging)
        linker.func_wrap("env", "log", |_caller: Caller<'_, ()>, _ptr: i32, _len: i32| {
            // Placeholder for host logging from WASM
        })?;

        let instance = linker.instantiate_async(&mut store, &self.module).await?;

        // We assume the WASM module exports:
        // - `allocate(size: i32) -> i32`
        // - `deallocate(ptr: i32, size: i32)`
        // - `process_request(ptr: i32, len: i32) -> i64` (returns (ptr << 32) | len)

        let memory = instance.get_memory(&mut store, "memory").context("Failed to find memory")?;

        let allocate = instance
            .get_typed_func::<i32, i32>(&mut store, "allocate")
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context("Missing allocate")?;

        let process_request = instance
            .get_typed_func::<(i32, i32), i64>(&mut store, "process_request")
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context("Missing process_request")?;

        // Write the request JSON into WASM memory
        let bytes = request_json.as_bytes();
        let len = bytes.len() as i32;
        let ptr = allocate.call_async(&mut store, len).await?;

        memory.write(&mut store, ptr as usize, bytes)?;

        // Call the processing function
        let result = process_request.call_async(&mut store, (ptr, len)).await?;

        let out_ptr = (result >> 32) as i32;
        let out_len_signed = (result & 0xFFFFFFFF) as i32;

        if out_ptr < 0 || out_len_signed < 0 {
            anyhow::bail!("WASM returned negative pointer or length");
        }
        let out_ptr = out_ptr as usize;
        let out_len = out_len_signed as usize;

        let memory_size: u64 = memory.size(&store);
        let wasm_page_size: u64 = 65536;
        let max_safe_len =
            (memory_size * wasm_page_size).saturating_sub(out_ptr as u64).min(1_048_576) as usize;
        if out_len > max_safe_len {
            anyhow::bail!("WASM output length {} exceeds max safe {}", out_len, max_safe_len);
        }

        let mut out_bytes = vec![0u8; out_len];
        memory.read(&mut store, out_ptr, &mut out_bytes)?;

        let out_str = String::from_utf8(out_bytes)?;

        Ok(out_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal middleware module: `allocate` hands out a fixed buffer,
    /// `process_request` returns a pointer/length pair into a preloaded
    /// static response — enough to exercise the whole host<->wasm protocol.
    const STATIC_RESPONSE: &str = r#"(module
      (memory (export "memory") 1)
      (data (i32.const 16) "{\"ok\":true}")
      (func (export "allocate") (param i32) (result i32) i32.const 64)
      (func (export "process_request") (param i32 i32) (result i64)
        ;; (16 << 32) | 11  -> pointer 16, length 11
        i64.const 68719476747)
    )"#;

    #[tokio::test]
    async fn executes_wat_module_and_reads_response() {
        let engine = WasmEngine::new(STATIC_RESPONSE.as_bytes()).unwrap();
        let out = engine.execute_middleware("{\"path\":\"/x\"}").await.unwrap();
        assert_eq!(out, r#"{"ok":true}"#);
    }

    #[tokio::test]
    async fn rejects_oversized_output_length() {
        // Returns pointer 0 / length 1_000_000 — far past the memory size.
        let wat = r#"(module
          (memory (export "memory") 1)
          (func (export "allocate") (param i32) (result i32) i32.const 64)
          (func (export "process_request") (param i32 i32) (result i64)
            i64.const 1000000)
        )"#;
        let engine = WasmEngine::new(wat.as_bytes()).unwrap();
        let err = engine.execute_middleware("{}").await.unwrap_err();
        assert!(err.to_string().contains("exceeds max safe"), "{err}");
    }

    #[tokio::test]
    async fn rejects_module_without_memory_export() {
        let wat = r#"(module
          (func (export "allocate") (param i32) (result i32) i32.const 0)
          (func (export "process_request") (param i32 i32) (result i64) i64.const 0)
        )"#;
        let engine = WasmEngine::new(wat.as_bytes()).unwrap();
        let err = engine.execute_middleware("{}").await.unwrap_err();
        assert!(err.to_string().contains("Failed to find memory"), "{err}");
    }

    #[test]
    fn rejects_invalid_wasm_bytes() {
        assert!(WasmEngine::new(b"not wasm").is_err());
    }
}
