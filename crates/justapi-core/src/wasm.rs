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
        linker.func_wrap(
            "env",
            "log",
            |_caller: Caller<'_, ()>, _ptr: i32, _len: i32| {
                // Placeholder for host logging from WASM
            },
        )?;

        let instance = linker.instantiate_async(&mut store, &self.module).await?;

        // We assume the WASM module exports:
        // - `allocate(size: i32) -> i32`
        // - `deallocate(ptr: i32, size: i32)`
        // - `process_request(ptr: i32, len: i32) -> i64` (returns (ptr << 32) | len)

        let memory = instance
            .get_memory(&mut store, "memory")
            .context("Failed to find memory")?;

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
        let out_len = (result & 0xFFFFFFFF) as i32;

        let mut out_bytes = vec![0u8; out_len as usize];
        memory.read(&mut store, out_ptr as usize, &mut out_bytes)?;

        let out_str = String::from_utf8(out_bytes)?;

        Ok(out_str)
    }
}
