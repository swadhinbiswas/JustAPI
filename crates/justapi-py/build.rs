//! Emit `cargo:rustc-cfg=Py_GIL_DISABLED` when building against a
//! free-threaded CPython (3.13t/3.14t).
//!
//! pyo3 emits this cfg only for its own crate; dependent crates must emit it
//! themselves to compile free-threaded-specific code paths (e.g. the GIL-pool
//! worker sizing in `gil_pool.rs`). Detection: ask the target interpreter
//! (`PYO3_PYTHON`, or the default `python3`) whether the GIL is disabled —
//! `sys._is_gil_enabled()` returns False exactly on free-threaded builds.

use std::process::Command;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(Py_GIL_DISABLED)");
    let python = std::env::var("PYO3_PYTHON").unwrap_or_else(|_| "python3".to_string());
    let output = Command::new(&python)
        .arg("-c")
        .arg(
            "import sys; print(sys._is_gil_enabled() if hasattr(sys, '_is_gil_enabled') else True)",
        )
        .output();

    let gil_enabled = match output {
        Ok(out) if out.status.success() => {
            std::str::from_utf8(&out.stdout).unwrap_or("true").trim() != "False"
        }
        _ => true, // cannot probe → assume GIL-locked (safe default)
    };

    if !gil_enabled {
        println!("cargo:rustc-cfg=Py_GIL_DISABLED");
        println!("cargo:warning=justapi-py: building for free-threaded CPython (Py_GIL_DISABLED)");
    }
}
