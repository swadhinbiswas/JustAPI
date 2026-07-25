use lettre::{
    message::{header::ContentType, Mailbox, Message, MessageBuilder, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
};
use tokio::sync::OnceCell;

use super::{EmailMessage, SmtpConfig};

/// Wraps a lettre SMTP transport behind a OnceCell for lazy initialization.
/// Clone is cheap (just Arc-like via OnceCell).
#[derive(Clone)]
pub struct SmtpSender {
    config: SmtpConfig,
    transport: std::sync::Arc<OnceCell<AsyncSmtpTransport<Tokio1Executor>>>,
}

impl std::fmt::Debug for SmtpSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpSender").field("host", &self.config.host).finish()
    }
}

impl SmtpSender {
    pub fn new(config: SmtpConfig) -> Self {
        Self { config, transport: std::sync::Arc::new(OnceCell::new()) }
    }

    async fn get_transport(
        &self,
    ) -> Result<AsyncSmtpTransport<Tokio1Executor>, lettre::transport::smtp::Error> {
        self.transport
            .get_or_try_init(|| async {
                let builder = if self.config.use_tls {
                    AsyncSmtpTransport::<Tokio1Executor>::relay(&self.config.host)?
                } else {
                    AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.config.host)
                        .port(self.config.port)
                };
                let mut builder = builder;
                if let (Some(user), Some(pass)) = (&self.config.username, &self.config.password) {
                    builder = builder.credentials(Credentials::new(user.clone(), pass.clone()));
                }
                Ok::<_, lettre::transport::smtp::Error>(builder.build())
            })
            .await
            .cloned()
    }

    fn mailbox(addr: &super::EmailAddress) -> Mailbox {
        addr.name
            .clone()
            .map(|n| Mailbox::new(Some(n), addr.address.parse().unwrap()))
            .unwrap_or_else(|| Mailbox::new(None, addr.address.parse().unwrap()))
    }

    fn add_recipients(
        builder: MessageBuilder,
        addrs: &[super::EmailAddress],
        kind: RecipientKind,
    ) -> MessageBuilder {
        let mut b = builder;
        for addr in addrs {
            let mailbox = Self::mailbox(addr);
            b = match kind {
                RecipientKind::To => b.to(mailbox),
                RecipientKind::Cc => b.cc(mailbox),
                RecipientKind::Bcc => b.bcc(mailbox),
            };
        }
        b
    }

    fn build_lettre_message(&self, msg: &EmailMessage) -> Result<Message, anyhow::Error> {
        let from = Self::mailbox(&self.config.default_from);

        let builder = Message::builder().from(from);
        let builder = Self::add_recipients(builder, &msg.to, RecipientKind::To);
        let builder = Self::add_recipients(builder, &msg.cc, RecipientKind::Cc);
        let builder = Self::add_recipients(builder, &msg.bcc, RecipientKind::Bcc);
        let builder = if let Some(reply) = &msg.reply_to {
            builder.reply_to(Self::mailbox(reply))
        } else {
            builder
        };
        let builder = builder.subject(&msg.subject);

        let has_attachments = !msg.attachments.is_empty();
        let has_both = msg.text_body.is_some() && msg.html_body.is_some();

        if has_attachments || has_both {
            let mut multipart = MultiPart::mixed().build();

            if has_both {
                let alt = MultiPart::alternative()
                    .singlepart(SinglePart::plain(
                        msg.text_body.as_ref().unwrap().clone(),
                    ))
                    .singlepart(SinglePart::html(
                        msg.html_body.as_ref().unwrap().clone(),
                    ));
                multipart = multipart.multipart(alt);
            } else if let Some(text) = &msg.text_body {
                multipart = multipart.singlepart(SinglePart::plain(text.clone()));
            } else if let Some(html) = &msg.html_body {
                multipart = multipart.singlepart(SinglePart::html(html.clone()));
            }

            for att in &msg.attachments {
                let ct: ContentType = att
                    .content_type
                    .parse()
                    .unwrap_or_else(|_| ContentType::parse("application/octet-stream").unwrap());
                let part = SinglePart::builder()
                    .header(ct)
                    .body(att.data.clone());
                multipart = multipart.singlepart(part);
            }

            builder.multipart(multipart)
        } else if let Some(html) = &msg.html_body {
            builder.body(html.clone())
        } else {
            builder.body(msg.text_body.clone().unwrap_or_default())
        }
        .map_err(|e| anyhow::anyhow!("Failed to build email: {e}"))
    }

    pub async fn send(&self, msg: &EmailMessage) -> Result<(), anyhow::Error> {
        let lettre_msg = self.build_lettre_message(msg)?;
        let transport = self.get_transport().await?;
        transport.send(lettre_msg).await?;
        Ok(())
    }
}

enum RecipientKind {
    To,
    Cc,
    Bcc,
}
