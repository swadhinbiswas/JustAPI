use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use justapi_core::mail::{Attachment, EmailAddress, EmailMessage};

pub fn build_message(
    to: String,
    subject: String,
    body: Option<String>,
    html: Option<String>,
    cc: Option<Vec<String>>,
    bcc: Option<Vec<String>>,
    reply_to: Option<String>,
    attachments: Option<Vec<Py<PyDict>>>,
    py: Python<'_>,
) -> PyResult<EmailMessage> {
    let to_addr = EmailAddress::new(to);
    let mut msg = EmailMessage::new(vec![to_addr], subject);

    if let Some(text) = body {
        msg = msg.text(text);
    }
    if let Some(h) = html {
        msg = msg.html(h);
    }
    if let Some(cc_list) = cc {
        let addrs: Vec<EmailAddress> = cc_list.into_iter().map(EmailAddress::new).collect();
        msg = msg.cc(addrs);
    }
    if let Some(bcc_list) = bcc {
        let addrs: Vec<EmailAddress> = bcc_list.into_iter().map(EmailAddress::new).collect();
        msg = msg.bcc(addrs);
    }
    if let Some(reply) = reply_to {
        msg = msg.reply_to(EmailAddress::new(reply));
    }
    if let Some(att_list) = attachments {
        for att_py in att_list {
            let dict: &Bound<'_, PyDict> = att_py.bind(py);
            let filename: String = dict
                .get_item("filename")?
                .ok_or_else(|| PyValueError::new_err("attachment missing 'filename'"))?
                .extract()
                .map_err(|_| PyValueError::new_err("attachment 'filename' must be a string"))?;
            let content_type: Option<String> = dict
                .get_item("content_type")?
                .and_then(|v| v.extract::<String>().ok());
            let data: Vec<u8> = dict
                .get_item("data")?
                .ok_or_else(|| PyValueError::new_err("attachment missing 'data'"))?
                .extract()
                .map_err(|_| PyValueError::new_err("attachment 'data' must be bytes"))?;
            let attachment = if let Some(ct) = content_type {
                Attachment::with_content_type(filename, ct, data)
            } else {
                Attachment::new(filename, data)
            };
            msg = msg.attach(attachment);
        }
    }

    Ok(msg)
}
