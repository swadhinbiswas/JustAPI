/// Alert severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertSeverity {
    Info = 0,
    Warning = 1,
    Critical = 2,
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Info => write!(f, "info"),
            AlertSeverity::Warning => write!(f, "warning"),
            AlertSeverity::Critical => write!(f, "critical"),
        }
    }
}

/// Supported alert notification channel types.
#[derive(Debug, Clone)]
pub enum AlertChannel {
    Slack,
    PagerDuty,
    Generic,
}

/// Configuration for alerting.
#[derive(Debug, Clone)]
pub struct AlertingConfig {
    pub webhook_url: Option<String>,
    pub min_severity: AlertSeverity,
    pub channel: AlertChannel,
    pub app_name: String,
}

impl Default for AlertingConfig {
    fn default() -> Self {
        Self {
            webhook_url: None,
            min_severity: AlertSeverity::Warning,
            channel: AlertChannel::Generic,
            app_name: "justapi".to_string(),
        }
    }
}

impl AlertingConfig {
    pub fn new(webhook_url: &str, _channel: AlertChannel) -> Self {
        Self { webhook_url: Some(webhook_url.to_string()), ..Default::default() }
    }

    /// Send an alert. When a webhook_url is configured, dispatches to the
    /// appropriate channel. Otherwise, logs the alert at the appropriate level.
    pub async fn send_alert(&self, severity: AlertSeverity, message: &str) {
        if (severity as u8) < (self.min_severity as u8) {
            return;
        }

        let msg = format!("[{}] {}: {}", self.app_name, severity, message);

        match severity {
            AlertSeverity::Info => tracing::info!("{}", msg),
            AlertSeverity::Warning => tracing::warn!("{}", msg),
            AlertSeverity::Critical => tracing::error!("{}", msg),
        }

        // If a webhook is configured, attempt to deliver
        if let Some(ref url) = self.webhook_url {
            let payload = match self.channel {
                AlertChannel::Slack => build_slack_payload(&self.app_name, &severity, message),
                AlertChannel::PagerDuty => {
                    build_pagerduty_payload(&self.app_name, &severity, message)
                }
                AlertChannel::Generic => build_generic_payload(&self.app_name, &severity, message),
            };

            if let Err(e) = post_webhook(url, &payload).await {
                tracing::warn!("Failed to send webhook alert: {}", e);
            }
        }
    }
}

fn build_slack_payload(app: &str, severity: &AlertSeverity, message: &str) -> String {
    let color = match severity {
        AlertSeverity::Info => "#36a64f",
        AlertSeverity::Warning => "#ffcc00",
        AlertSeverity::Critical => "#ff0000",
    };
    serde_json::json!({
        "attachments": [{
            "color": color,
            "title": format!("[{}] {} Alert", app, severity),
            "text": message,
            "ts": unix_timestamp(),
        }]
    })
    .to_string()
}

fn build_pagerduty_payload(app: &str, severity: &AlertSeverity, message: &str) -> String {
    let sev = match severity {
        AlertSeverity::Info => "info",
        AlertSeverity::Warning => "warning",
        AlertSeverity::Critical => "critical",
    };
    serde_json::json!({
        "routing_key": "webhook",
        "event_action": "trigger",
        "payload": {
            "summary": message,
            "source": app,
            "severity": sev,
            "custom_details": {
                "app": app,
                "message": message,
            }
        }
    })
    .to_string()
}

fn build_generic_payload(app: &str, severity: &AlertSeverity, message: &str) -> String {
    let mut m = serde_json::Map::new();
    m.insert("app".into(), serde_json::Value::String(app.into()));
    m.insert("severity".into(), serde_json::Value::String(severity.to_string()));
    m.insert("message".into(), serde_json::Value::String(message.into()));
    m.insert("timestamp".into(), serde_json::Value::Number(unix_timestamp().into()));
    serde_json::to_string(&m).unwrap_or_else(|_| "{}".to_string())
}

/// Simple HTTP POST to a webhook URL. Supports HTTP only (not HTTPS).
/// For HTTPS webhooks, use a reverse proxy or local HTTP gateway.
async fn post_webhook(url: &str, body: &str) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;

    let uri: hyper::http::Uri = url.parse()?;
    let scheme = uri.scheme_str().unwrap_or("http");
    if scheme != "http" {
        return Err(anyhow::anyhow!("webhook URL scheme '{}' not supported (use HTTP)", scheme));
    }
    let host = uri.host().ok_or_else(|| anyhow::anyhow!("webhook URL missing host"))?;
    let port = uri.port_u16().unwrap_or(80);
    let path = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

    let mut stream = tokio::net::TcpStream::connect((host, port)).await?;

    let request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        path = path,
        host = host,
        len = body.len(),
        body = body,
    );

    stream.write_all(request.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

fn unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_slack_payload() {
        let payload = build_slack_payload("test-app", &AlertSeverity::Critical, "something broke");
        assert!(payload.contains("test-app"));
        assert!(payload.contains("critical"));
        assert!(payload.contains("something broke"));
    }

    #[test]
    fn test_build_pagerduty_payload() {
        let payload = build_pagerduty_payload("test-app", &AlertSeverity::Warning, "high latency");
        assert!(payload.contains("warning"));
        assert!(payload.contains("high latency"));
    }

    #[test]
    fn test_severity_display() {
        assert_eq!(AlertSeverity::Info.to_string(), "info");
        assert_eq!(AlertSeverity::Critical.to_string(), "critical");
    }

    #[test]
    fn test_severity_filtering() {
        let config = AlertingConfig::default();
        // Info should be filtered out since default min is Warning
        assert!((AlertSeverity::Info as u8) < (config.min_severity as u8));
        assert!((AlertSeverity::Warning as u8) >= (config.min_severity as u8));
        assert!((AlertSeverity::Critical as u8) >= (config.min_severity as u8));
    }
}
