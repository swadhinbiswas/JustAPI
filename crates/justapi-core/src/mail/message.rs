use super::{Attachment, EmailAddress};

/// An email message ready to be sent.
#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub to: Vec<EmailAddress>,
    pub cc: Vec<EmailAddress>,
    pub bcc: Vec<EmailAddress>,
    pub reply_to: Option<EmailAddress>,
    pub subject: String,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
    pub attachments: Vec<Attachment>,
    pub headers: Vec<(String, String)>,
}

impl EmailMessage {
    pub fn new(to: Vec<EmailAddress>, subject: impl Into<String>) -> Self {
        Self {
            to,
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: None,
            subject: subject.into(),
            text_body: None,
            html_body: None,
            attachments: Vec::new(),
            headers: Vec::new(),
        }
    }

    pub fn text(mut self, body: impl Into<String>) -> Self {
        self.text_body = Some(body.into());
        self
    }

    pub fn html(mut self, body: impl Into<String>) -> Self {
        self.html_body = Some(body.into());
        self
    }

    pub fn attach(mut self, attachment: Attachment) -> Self {
        self.attachments.push(attachment);
        self
    }

    pub fn cc(mut self, addresses: Vec<EmailAddress>) -> Self {
        self.cc = addresses;
        self
    }

    pub fn bcc(mut self, addresses: Vec<EmailAddress>) -> Self {
        self.bcc = addresses;
        self
    }

    pub fn reply_to(mut self, address: EmailAddress) -> Self {
        self.reply_to = Some(address);
        self
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}
