use std::sync::OnceLock;

use hyper::body::Bytes;
use pyo3::prelude::*;

use super::NATIVE_HELPER;
/// Response from a native Python handler.
pub enum NativeBody {
    Bytes(Vec<u8>),
    Stream(tokio::sync::mpsc::UnboundedReceiver<Result<Bytes, anyhow::Error>>),
}

pub struct NativeResponse {
    pub status: u16,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
    pub body: NativeBody,
}

#[pyclass(name = "_StreamSender")]
pub struct StreamSender {
    pub(crate) tx: tokio::sync::mpsc::UnboundedSender<Result<Bytes, anyhow::Error>>,
}

#[pymethods]
impl StreamSender {
    fn send(&self, data: &[u8]) {
        let _ = self.tx.send(Ok(Bytes::copy_from_slice(data)));
    }

    fn send_error(&self, msg: String) {
        let _ = self.tx.send(Err(anyhow::anyhow!("{}", msg)));
    }

    fn close(&self) {
        // Channel closes automatically when all senders are dropped.
        // This method exists for Starlette API compatibility but is a no-op
        // since the channel lifecycle is managed by Rust's ownership system.
    }
}

#[pyclass(name = "TokenStreamResponse", subclass)]
pub struct TokenStreamResponse {
    pub generator: Py<PyAny>,
    pub status: u16,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
}

#[pymethods]
impl TokenStreamResponse {
    #[new]
    #[pyo3(signature = (generator, status=200, headers=None))]
    pub fn new(
        generator: Py<PyAny>,
        status: u16,
        headers: Option<Vec<(Vec<u8>, Vec<u8>)>>,
    ) -> Self {
        let headers = headers
            .unwrap_or_else(|| vec![(b"content-type".to_vec(), b"text/event-stream".to_vec())]);
        Self { generator, status, headers }
    }
}

#[derive(Clone)]
pub struct BatchedReq {
    pub path: String,
    pub method: String,
    pub path_params: Vec<(String, String)>,
    pub query_string: Vec<u8>,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
    pub body: Vec<u8>,
}

pub(crate) struct HelperFunctions {
    pub(crate) call_handler: Py<PyAny>,
    pub(crate) call_batch_handler: Py<PyAny>,
    pub(crate) validate_body: Py<PyAny>,
    pub(crate) parse_body: Py<PyAny>,
    pub(crate) call_plugin_hook: Py<PyAny>,
    pub(crate) wrap_result: Py<PyAny>,
    pub(crate) pump_stream: Py<PyAny>,
    pub(crate) pump_validated_stream: Py<PyAny>,
    pub(crate) run_ws_handler: Py<PyAny>,
    pub(crate) set_trace_context: Py<PyAny>,
}

pub(crate) fn get_helper(py: Python<'_>) -> &HelperFunctions {
    static HELPER: OnceLock<HelperFunctions> = OnceLock::new();
    HELPER.get_or_init(|| {
        let code = std::ffi::CString::new(NATIVE_HELPER).expect("valid CString");
        let filename = std::ffi::CString::new("_native_helper.py").expect("valid CString");
        let name = std::ffi::CString::new("_native_helper").expect("valid CString");

        let helper = PyModule::from_code(py, code.as_c_str(), filename.as_c_str(), name.as_c_str())
            .expect("Native helper module should compile");

        let call_handler =
            helper.getattr("call_handler").expect("call_handler function should exist").unbind();
        let call_batch_handler = helper
            .getattr("call_batch_handler")
            .expect("call_batch_handler function should exist")
            .unbind();
        let validate_body =
            helper.getattr("validate_body").expect("validate_body function should exist").unbind();
        let parse_body =
            helper.getattr("parse_body").expect("parse_body function should exist").unbind();
        let call_plugin_hook = helper
            .getattr("call_plugin_hook")
            .expect("call_plugin_hook function should exist")
            .unbind();
        let wrap_result =
            helper.getattr("wrap_result").expect("wrap_result function should exist").unbind();
        let pump_stream =
            helper.getattr("_pump_stream").expect("_pump_stream function should exist").unbind();
        let pump_validated_stream = helper
            .getattr("_pump_validated_stream")
            .expect("_pump_validated_stream function should exist")
            .unbind();
        let run_ws_handler = helper
            .getattr("run_ws_handler")
            .expect("run_ws_handler function should exist")
            .unbind();
        let set_trace_context = helper
            .getattr("set_trace_context")
            .expect("set_trace_context function should exist")
            .unbind();

        HelperFunctions {
            call_handler,
            call_batch_handler,
            validate_body,
            parse_body,
            call_plugin_hook,
            wrap_result,
            pump_stream,
            pump_validated_stream,
            run_ws_handler,
            set_trace_context,
        }
    })
}
