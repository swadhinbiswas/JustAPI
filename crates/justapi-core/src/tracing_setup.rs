use std::path::Path;
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Log output format.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogFormat {
    Text,
    Json,
}

/// File rotation strategy.
#[derive(Debug, Clone)]
pub enum FileRotation {
    Daily,
    Hourly,
    Never,
}

/// OpenTelemetry exporter backend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OtelExporter {
    /// Print spans to stdout.
    Stdout,
    /// Export via OTLP gRPC.
    Oltp,
}

/// Configuration for JustAPI's logging and tracing subsystem.
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub format: LogFormat,
    pub level: String,
    pub file_path: Option<String>,
    pub file_rotation: FileRotation,
    pub otel_exporter: Option<OtelExporter>,
    pub otlp_endpoint: Option<String>,
    pub service_name: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            format: LogFormat::Text,
            level: "info".to_string(),
            file_path: None,
            file_rotation: FileRotation::Daily,
            otel_exporter: Some(OtelExporter::Stdout),
            otlp_endpoint: None,
            service_name: "justapi".to_string(),
        }
    }
}

/// Global guard for the non-blocking file appender.
/// Must be held for the program's lifetime to prevent log loss on shutdown.
static FILE_GUARD: OnceLock<Mutex<Option<WorkerGuard>>> = OnceLock::new();

fn set_guard(guard: WorkerGuard) {
    let lock = FILE_GUARD.get_or_init(|| Mutex::new(None));
    *lock.lock().unwrap() = Some(guard);
}

#[cfg(feature = "opentelemetry")]
static TRACER_GUARD: OnceLock<Mutex<Option<opentelemetry_sdk::trace::TracerProvider>>> =
    OnceLock::new();

/// Initialize tracing with the given configuration.
pub fn init_logging(config: &LoggingConfig) -> Result<()> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    let fmt_layer = match config.format {
        LogFormat::Json => tracing_subscriber::fmt::layer()
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .boxed(),
        LogFormat::Text => tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .boxed(),
    };

    let mut layers = Vec::new();

    #[cfg(feature = "opentelemetry")]
    if let Some(exporter) = &config.otel_exporter {
        match exporter {
            OtelExporter::Stdout => {
                let provider = opentelemetry_sdk::trace::TracerProvider::builder()
                    .with_simple_exporter(opentelemetry_stdout::SpanExporter::default())
                    .build();
                let service_name = config.service_name.clone();
                let tracer = provider.tracer(service_name);
                opentelemetry::global::set_tracer_provider(provider.clone());
                layers.push(tracing_opentelemetry::layer().with_tracer(tracer).boxed());
                set_tracer_guard(provider);
            }
            OtelExporter::Oltp => {
                let endpoint = config
                    .otlp_endpoint
                    .clone()
                    .unwrap_or_else(|| "http://localhost:4317".to_string());
                let provider = opentelemetry_otlp::new_pipeline()
                    .tracing()
                    .with_exporter(
                        opentelemetry_otlp::new_exporter().tonic().with_endpoint(endpoint),
                    )
                    .install_batch(opentelemetry_sdk::runtime::Tokio)
                    .map_err(|e| anyhow::anyhow!("Failed to build OTLP tracing: {}", e))?;
                let service_name = config.service_name.clone();
                let tracer = provider.tracer(service_name);
                opentelemetry::global::set_tracer_provider(provider.clone());
                layers.push(tracing_opentelemetry::layer().with_tracer(tracer).boxed());
                set_tracer_guard(provider);
            }
        }
    }

    if let Some(ref path) = config.file_path {
        let parent = Path::new(path).parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)?;
        let file_name =
            Path::new(path).file_name().unwrap_or_else(|| std::ffi::OsStr::new("justapi.log"));

        let file_appender = match config.file_rotation {
            FileRotation::Daily => tracing_appender::rolling::daily(parent, file_name),
            FileRotation::Hourly => tracing_appender::rolling::hourly(parent, file_name),
            FileRotation::Never => tracing_appender::rolling::never(parent, file_name),
        };

        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        set_guard(guard);

        let file_layer =
            tracing_subscriber::fmt::layer().with_writer(non_blocking).with_ansi(false).boxed();

        layers.push(file_layer);
    } else {
        layers.push(fmt_layer);
    }

    tracing_subscriber::registry().with(env_filter).with(layers).init();

    Ok(())
}

#[cfg(feature = "opentelemetry")]
use opentelemetry::trace::TracerProvider as _;
#[cfg(feature = "opentelemetry")]
use opentelemetry_otlp::WithExportConfig;

#[cfg(feature = "opentelemetry")]
fn set_tracer_guard(provider: opentelemetry_sdk::trace::TracerProvider) {
    let lock = TRACER_GUARD.get_or_init(|| Mutex::new(None));
    *lock.lock().unwrap() = Some(provider);
}

/// Initialize tracing with text format and env-filter (backward compatible).
pub fn init_tracing() -> Result<()> {
    init_logging(&LoggingConfig::default())
}

/// Initialize JSON-formatted tracing with env-filter.
pub fn init_json_logging() -> Result<()> {
    init_logging(&LoggingConfig { format: LogFormat::Json, ..Default::default() })
}

/// Initialize JSON logging to a rolling file.
pub fn init_file_logging(path: &str) -> Result<()> {
    init_logging(&LoggingConfig {
        format: LogFormat::Json,
        file_path: Some(path.to_string()),
        ..Default::default()
    })
}

/// Initialize tracing with an OTLP endpoint.
pub fn init_otlp_tracing(endpoint: &str, service_name: &str) -> Result<()> {
    init_logging(&LoggingConfig {
        otel_exporter: Some(OtelExporter::Oltp),
        otlp_endpoint: Some(endpoint.to_string()),
        service_name: service_name.to_string(),
        ..Default::default()
    })
}

/// Shutdown the tracing subscriber, flushing any pending spans.
pub fn shutdown_tracing() {
    #[cfg(feature = "opentelemetry")]
    {
        if let Some(guard_lock) = TRACER_GUARD.get() {
            if let Ok(mut guard) = guard_lock.lock() {
                if let Some(provider) = guard.take() {
                    if let Err(e) = provider.shutdown() {
                        tracing::error!("Error shutting down tracer provider: {}", e);
                    }
                }
            }
        }
        opentelemetry::global::shutdown_tracer_provider();
    }
    tracing::info!("Tracing shut down");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shutdown_does_not_panic() {
        shutdown_tracing();
    }

    #[test]
    fn test_config_defaults() {
        let cfg = LoggingConfig::default();
        assert_eq!(cfg.format, LogFormat::Text);
        assert_eq!(cfg.level, "info");
        assert!(cfg.file_path.is_none());
    }
}
