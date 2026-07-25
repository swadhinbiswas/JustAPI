use std::sync::Arc;

use super::{smtp::SmtpSender, SmtpConfig};

/// Global mailer state shared across requests.
pub struct MailerState {
    pub config: SmtpConfig,
    pub sender: SmtpSender,
}

impl MailerState {
    pub fn new(config: SmtpConfig) -> Self {
        let sender = SmtpSender::new(config.clone());
        Self { config, sender }
    }
}

/// Thread-safe handle to the mailer.
pub type SharedMailer = Arc<MailerState>;
