pub mod message;
pub mod otp;
pub mod queue;
pub mod smtp;
pub mod state;
pub mod template;
pub mod types;
pub mod verify;

pub use message::EmailMessage;
pub use state::{MailerState, SharedMailer};
pub use types::{Attachment, EmailAddress, SmtpConfig};
