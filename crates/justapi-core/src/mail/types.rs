/// Email address with optional display name.
#[derive(Debug, Clone)]
pub struct EmailAddress {
    pub address: String,
    pub name: Option<String>,
}

impl EmailAddress {
    pub fn new(address: impl Into<String>) -> Self {
        Self { address: address.into(), name: None }
    }

    pub fn named(address: impl Into<String>, name: impl Into<String>) -> Self {
        Self { address: address.into(), name: Some(name.into()) }
    }
}

impl From<String> for EmailAddress {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for EmailAddress {
    fn from(s: &str) -> Self {
        Self::new(s.to_string())
    }
}

/// A file attachment for an email.
#[derive(Debug, Clone)]
pub struct Attachment {
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
    pub inline: bool,
}

impl Attachment {
    pub fn new(filename: impl Into<String>, data: Vec<u8>) -> Self {
        let filename = filename.into();
        let content_type = mime_guess(&filename).unwrap_or("application/octet-stream");
        Self { filename, content_type: content_type.to_string(), data, inline: false }
    }

    pub fn with_content_type(
        filename: impl Into<String>,
        content_type: impl Into<String>,
        data: Vec<u8>,
    ) -> Self {
        Self { filename: filename.into(), content_type: content_type.into(), data, inline: false }
    }

    pub fn inline(mut self) -> Self {
        self.inline = true;
        self
    }
}

fn mime_guess(filename: &str) -> Option<&'static str> {
    let ext = filename.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "txt" => Some("text/plain"),
        "html" => Some("text/html"),
        "json" => Some("application/json"),
        "pdf" => Some("application/pdf"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "svg" => Some("image/svg+xml"),
        "csv" => Some("text/csv"),
        "zip" => Some("application/zip"),
        "xml" => Some("application/xml"),
        _ => None,
    }
}

/// SMTP connection configuration.
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub use_tls: bool,
    pub default_from: EmailAddress,
    pub default_from_name: Option<String>,
}

impl SmtpConfig {
    pub fn new(host: impl Into<String>, port: u16, default_from: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port,
            username: None,
            password: None,
            use_tls: true,
            default_from: EmailAddress::new(default_from),
            default_from_name: None,
        }
    }

    pub fn credentials(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }

    pub fn no_tls(mut self) -> Self {
        self.use_tls = false;
        self
    }
}
