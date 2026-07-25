use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use justapi_core::mail::smtp::SmtpSender as CoreSmtpSender;
use justapi_core::mail::SmtpConfig;

use crate::message_builder;

/// SMTP mail sender with optional template rendering.
///
/// Usage:
///
/// ```python
/// from justapi import Mailer
/// mailer = Mailer(host="smtp.example.com", port=587, username="user",
///                 password="pass", default_from="noreply@example.com")
/// mailer.send(to="user@example.com", subject="Hello", body="World")
/// ```
#[pyclass(name = "Mailer")]
pub struct PyMailer {
    sender: CoreSmtpSender,
}

#[pymethods]
impl PyMailer {
    #[new]
    #[pyo3(signature = (host, port=587, username=None, password=None, use_tls=true, default_from=None, default_from_name=None))]
    fn py_new(
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
        use_tls: bool,
        default_from: Option<String>,
        default_from_name: Option<String>,
    ) -> Self {
        let from = default_from.unwrap_or_else(|| format!("noreply@{}.invalid", host));
        let mut config = SmtpConfig::new(host, port, from);
        if let (Some(user), Some(pass)) = (&username, &password) {
            config = config.credentials(user.clone(), pass.clone());
        }
        if !use_tls {
            config = config.no_tls();
        }
        config.default_from_name = default_from_name;
        let sender = CoreSmtpSender::new(config);
        Self { sender }
    }

    /// Send an email synchronously. Blocks the current thread.
    #[pyo3(signature = (to, subject, body=None, html=None, cc=None, bcc=None, reply_to=None, attachments=None))]
    fn send(
        &self,
        to: String,
        subject: String,
        body: Option<String>,
        html: Option<String>,
        cc: Option<Vec<String>>,
        bcc: Option<Vec<String>>,
        reply_to: Option<String>,
        attachments: Option<Vec<Py<PyDict>>>,
        py: Python<'_>,
    ) -> PyResult<()> {
        let msg = message_builder::build_message(
            to,
            subject,
            body,
            html,
            cc,
            bcc,
            reply_to,
            attachments,
            py,
        )?;
        let sender = self.sender.clone();
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| PyValueError::new_err(format!("Failed to create runtime: {e}")))?;
        rt.block_on(async {
            sender
                .send(&msg)
                .await
                .map_err(|e| PyValueError::new_err(format!("Failed to send email: {e}")))
        })
    }

    /// Send an email asynchronously. Returns a coroutine.
    #[pyo3(signature = (to, subject, body=None, html=None, cc=None, bcc=None, reply_to=None, attachments=None))]
    fn send_async<'py>(
        &'py self,
        to: String,
        subject: String,
        body: Option<String>,
        html: Option<String>,
        cc: Option<Vec<String>>,
        bcc: Option<Vec<String>>,
        reply_to: Option<String>,
        attachments: Option<Vec<Py<PyDict>>>,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let msg = message_builder::build_message(
            to,
            subject,
            body,
            html,
            cc,
            bcc,
            reply_to,
            attachments,
            py,
        )?;
        let sender = self.sender.clone();
        let fut = async move {
            sender.send(&msg).await.map_err(|e| anyhow::anyhow!("Failed to send email: {e}"))
        };
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            fut.await.map_err(|e| PyValueError::new_err(e.to_string()))
        })
    }
}
