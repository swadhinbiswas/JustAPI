use pyo3::ffi;
use pyo3::prelude::*;
use std::os::raw::c_int;

#[pyclass]
pub struct ZeroCopyBuffer {
    data: Vec<u8>,
}

#[pymethods]
impl ZeroCopyBuffer {
    #[new]
    pub fn new(data: Vec<u8>) -> Self {
        ZeroCopyBuffer { data }
    }

    unsafe fn __getbuffer__(
        slf: PyRefMut<'_, Self>,
        view: *mut ffi::Py_buffer,
        _flags: c_int,
    ) -> PyResult<()> {
        // SAFETY: The `view` pointer is checked for null before any
        // dereference (line 22-23).  `Py_NewRef` is a safe FFI call that
        // increments the reference count of the Python object.  The `buf`
        // pointer points into `slf.data` which is a `Vec<u8>` and remains
        // valid for the lifetime of the `ZeroCopyBuffer`.  All other fields
        // (`readonly`, `itemsize`, `ndim`, `format`, `shape`, `strides`,
        // `suboffsets`, `internal`) are set to safe defaults that indicate
        // a flat, read-only, one-dimensional buffer of bytes.  The `unsafe`
        // qualifier is required by PyO3's buffer-protocol trait.
        if view.is_null() {
            return Err(pyo3::exceptions::PyBufferError::new_err("View is null"));
        }

        (*view).obj = ffi::Py_NewRef(slf.as_ptr());
        (*view).buf = slf.data.as_ptr() as *mut _;
        (*view).len = slf.data.len() as isize;
        (*view).readonly = 1;
        (*view).itemsize = 1;

        (*view).format = std::ptr::null_mut();
        (*view).ndim = 0;
        (*view).shape = std::ptr::null_mut();
        (*view).strides = std::ptr::null_mut();
        (*view).suboffsets = std::ptr::null_mut();
        (*view).internal = std::ptr::null_mut();

        Ok(())
    }

    unsafe fn __releasebuffer__(&self, _view: *mut ffi::Py_buffer) {
        // SAFETY: The `__releasebuffer__` is a no-op because no heap
        // allocations were made inside `__getbuffer__` that require
        // explicit cleanup (the buffer view directly references the
        // `Vec<u8>` owned by `ZeroCopyBuffer` which is dropped by Rust's
        // normal destructor).  The `unsafe` qualifier is required by
        // PyO3's buffer-protocol trait.
    }
}
