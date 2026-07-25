use tokio::sync::mpsc;

use super::{EmailMessage, smtp::SmtpSender};

/// Background email delivery via a tokio mpsc channel.
pub struct EmailQueue {
    sender: mpsc::UnboundedSender<QueuedEmail>,
}

struct QueuedEmail {
    msg: EmailMessage,
}

impl EmailQueue {
    /// Spawn a background worker that processes emails from the queue.
    /// The worker receives an SmtpSender.
    pub fn spawn(smtp: SmtpSender) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<QueuedEmail>();

        tokio::spawn(async move {
            while let Some(queued) = rx.recv().await {
                if let Err(e) = smtp.send(&queued.msg).await {
                    tracing::error!("Failed to send queued email: {}", e);
                }
            }
        });

        Self { sender: tx }
    }

    /// Enqueue an email for background delivery.
    pub fn enqueue(&self, msg: EmailMessage) -> Result<(), anyhow::Error> {
        self.sender
            .send(QueuedEmail { msg })
            .map_err(|_| anyhow::anyhow!("email queue is closed"))
    }

    /// Enqueue and return immediately. Errors are logged by the worker.
    pub fn enqueue_lossy(&self, msg: EmailMessage) {
        let _ = self.enqueue(msg);
    }
}
