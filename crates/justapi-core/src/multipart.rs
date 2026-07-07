use std::collections::HashMap;

use anyhow::Result;
use bytes::Bytes;
use http_body_util::BodyDataStream;
use multer::Multipart;

use std::path::PathBuf;

/// A file uploaded via `multipart/form-data`.
#[derive(Debug)]
pub struct UploadFile {
    pub field_name: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub temp_path: PathBuf,
}

/// Parsed multipart form data containing both text fields and uploaded files.
#[derive(Debug, Default)]
pub struct MultipartForm {
    pub fields: HashMap<String, String>,
    pub files: Vec<UploadFile>,
}

impl MultipartForm {
    pub fn file(&self, field_name: &str) -> Option<&UploadFile> {
        self.files.iter().find(|f| f.field_name == field_name)
    }

    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(|s| s.as_str())
    }
}

/// Parse a `multipart/form-data` request body.
///
/// `body` — the request body (any `http_body::Body<Data = Bytes>`).
/// `content_type` — the value of the `Content-Type` header (must include the
/// boundary, e.g. `multipart/form-data; boundary=----WebKitFormBoundaryxyz`).
///
/// Returns a [`MultipartForm`] with all text fields and file fields collected.
/// Text fields go into `fields`; files (fields with a filename) go into `files`.
pub async fn parse_multipart<B>(body: B, content_type: &str) -> Result<MultipartForm>
where
    B: http_body::Body<Data = Bytes> + Send + Unpin + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let boundary = extract_boundary(content_type)?;
    let stream = BodyDataStream::new(body);
    let mut multipart = Multipart::new(stream, &boundary);
    let mut form = MultipartForm::default();
    let mut size: usize = 0;
    let max_size: usize = 50 * 1024 * 1024; // 50 MB default

    while let Some(mut field) = multipart.next_field().await? {
        let name = field.name().unwrap_or("").to_string();
        let filename = field.file_name().map(|s| s.to_string());
        let ct = field.content_type().map(|m| m.to_string());

        if filename.is_some() {
            let mut temp_file = tempfile::NamedTempFile::new()?;
            let temp_path = temp_file.path().to_path_buf();

            use std::io::Write;
            while let Some(chunk) = field.chunk().await? {
                size += chunk.len();
                if size > max_size {
                    return Err(anyhow::anyhow!("file exceeds maximum size of 50 MB"));
                }
                temp_file.write_all(&chunk)?;
            }
            // Keep the temp file on disk by persisting it (or by just storing the path, but NamedTempFile deletes when dropped.
            // We should use `keep()` to persist it, and the Python wrapper will delete it when done).
            temp_file.keep()?;

            form.files.push(UploadFile {
                field_name: name,
                filename,
                content_type: ct,
                temp_path,
            });
        } else {
            let text = field.text().await?;
            form.fields.insert(name, text);
        }
    }

    Ok(form)
}

fn extract_boundary(content_type: &str) -> Result<String> {
    for part in content_type.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("boundary=") {
            let b = value.trim();
            // Strip optional quotes
            let b = b.strip_prefix('"').unwrap_or(b);
            let b = b.strip_suffix('"').unwrap_or(b);
            return Ok(b.to_string());
        }
    }
    Err(anyhow::anyhow!("missing boundary in Content-Type header"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::Full;

    #[test]
    fn extract_boundary_works() {
        let ct = "multipart/form-data; boundary=----WebKitFormBoundaryxyz";
        assert_eq!(extract_boundary(ct).unwrap(), "----WebKitFormBoundaryxyz");
    }

    #[test]
    fn extract_boundary_quoted() {
        let ct = r#"multipart/form-data; boundary="----Boundary123""#;
        assert_eq!(extract_boundary(ct).unwrap(), "----Boundary123");
    }

    #[test]
    fn extract_boundary_missing_errors() {
        let ct = "multipart/form-data";
        assert!(extract_boundary(ct).is_err());
    }

    fn build_multipart_body(
        fields: &[(&str, Option<&str>, &str)], // (name, filename, data)
    ) -> (Vec<u8>, String) {
        let boundary = "----TestBoundary123";
        let mut body = Vec::new();
        for (name, filename, data) in fields {
            body.extend_from_slice(b"--");
            body.extend_from_slice(boundary.as_bytes());
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(b"Content-Disposition: form-data; name=\"");
            body.extend_from_slice(name.as_bytes());
            body.extend_from_slice(b"\"");
            if let Some(fname) = filename {
                body.extend_from_slice(b"; filename=\"");
                body.extend_from_slice(fname.as_bytes());
                body.extend_from_slice(b"\"");
            }
            body.extend_from_slice(b"\r\n");
            if filename.is_some() {
                body.extend_from_slice(b"Content-Type: application/octet-stream\r\n");
                body.extend_from_slice(b"Content-Transfer-Encoding: binary\r\n");
            }
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(data.as_bytes());
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"--\r\n");
        let ct = format!("multipart/form-data; boundary={}", boundary);
        (body, ct)
    }

    #[tokio::test]
    async fn parse_multipart_text_fields() {
        let (body, ct) =
            build_multipart_body(&[("username", None, "alice"), ("role", None, "admin")]);
        let body = Full::new(Bytes::from(body));
        let form = parse_multipart(body, &ct).await.unwrap();
        assert_eq!(form.field("username"), Some("alice"));
        assert_eq!(form.field("role"), Some("admin"));
        assert!(form.files.is_empty());
    }

    #[tokio::test]
    async fn parse_multipart_file_upload() {
        let file_content = "hello, this is a file";
        let (body, ct) = build_multipart_body(&[
            ("description", None, "my file"),
            ("file", Some("test.txt"), file_content),
        ]);
        let body = Full::new(Bytes::from(body));
        let form = parse_multipart(body, &ct).await.unwrap();
        assert_eq!(form.field("description"), Some("my file"));
        assert_eq!(form.files.len(), 1);
        let f = form.file("file").unwrap();
        assert_eq!(f.filename.as_deref(), Some("test.txt"));
        assert_eq!(f.content_type.as_deref(), Some("application/octet-stream"));
        assert_eq!(std::fs::read_to_string(&f.temp_path).unwrap(), file_content);
    }

    #[tokio::test]
    async fn parse_multipart_multiple_files() {
        let (body, ct) = build_multipart_body(&[
            ("file1", Some("a.txt"), "content a"),
            ("file2", Some("b.txt"), "content b"),
        ]);
        let body = Full::new(Bytes::from(body));
        let form = parse_multipart(body, &ct).await.unwrap();
        assert_eq!(form.files.len(), 2);
        assert_eq!(
            form.file("file1").unwrap().filename.as_deref(),
            Some("a.txt")
        );
        assert_eq!(
            form.file("file2").unwrap().filename.as_deref(),
            Some("b.txt")
        );
    }

    #[tokio::test]
    async fn parse_multipart_missing_boundary_errors() {
        let body = Full::new(Bytes::from("some data"));
        let result = parse_multipart(body, "multipart/form-data").await;
        assert!(result.is_err());
    }
}
