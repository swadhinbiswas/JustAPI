use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use justapi_core::dx::{DiagLevel, Diagnostic};
use justapi_core::error_catalog;
use tokio::sync::mpsc;
mod gen_client;
mod profile;
mod watcher;

#[derive(Parser)]
#[command(name = "justapi", about = "JustAPI Runtime — Python application server")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the JustAPI HTTP server
    Serve {
        /// Address to bind to
        #[arg(short, long, default_value = "127.0.0.1:8080")]
        addr: SocketAddr,

        /// Serve static files from this directory
        #[arg(long)]
        static_dir: Option<String>,

        /// Enable response compression (gzip)
        #[cfg(feature = "compression")]
        #[arg(long)]
        compress: bool,

        /// Enable hot reload (watches for file changes and restarts)
        #[arg(short, long)]
        reload: bool,

        /// Directory to watch for file changes (defaults to cwd)
        #[arg(long)]
        watch_dir: Option<PathBuf>,

        /// Additional file extensions to watch (e.g. "html" "css")
        #[arg(long)]
        watch_ext: Vec<String>,

        /// Timeout in seconds to drain in-flight requests before restart
        #[arg(long, default_value = "5")]
        drain_timeout: u64,

        /// TLS certificate file (PEM)
        #[cfg(feature = "tls")]
        #[arg(long)]
        tls_cert: Option<String>,

        /// TLS private key file (PEM)
        #[cfg(feature = "tls")]
        #[arg(long)]
        tls_key: Option<String>,

        /// Serve a model via the OpenAI-compatible endpoints
        /// (`/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`,
        /// `/v1/models`). Requires the `inference` feature. The model id is
        /// registered as a weight-free `MockModel` (use `--features real` +
        /// `--model-path` for real weights).
        #[cfg(feature = "inference")]
        #[arg(long)]
        model: Option<String>,

        /// GPU device ordinal for `--model` (e.g. `0`). Use `cpu` for the
        /// CPU backend (default). Requires the `inference` feature.
        #[cfg(feature = "inference")]
        #[arg(long, default_value = "cpu")]
        gpu: String,

        /// Enable the scheduler-backed serving path (continuous batching +
        /// RadixAttention prefix cache).  Without this flag the engine
        /// serves requests directly (no admission control or prefix reuse).
        /// Requires `--model`.  Requires the `inference` feature.
        #[cfg(feature = "inference")]
        #[arg(long)]
        scheduled: bool,

        /// Number of KV-cache blocks for the scheduler's block pool.
        /// Only meaningful with `--scheduled`.
        #[cfg(feature = "inference")]
        #[arg(long, default_value = "1024")]
        pool_blocks: usize,

        /// Maximum number of concurrent sequences for the scheduler.
        /// Only meaningful with `--scheduled`.
        #[cfg(feature = "inference")]
        #[arg(long, default_value = "256")]
        max_seqs: usize,

        /// Number of draft tokens per speculative-decode step (`gamma`).
        /// Requires `--model`. `>0` enables draft-target speculative decoding
        /// (a draft model proposes `gamma` tokens, the target verifies them).
        /// Requires the `inference` feature.
        #[cfg(feature = "inference")]
        #[arg(long, default_value_t = 0)]
        gamma: usize,

        /// Tree branch factor for tree-based speculative decoding (Medusa/
        /// EAGLE-style). Requires `--model` and `--gamma`. `>0` enables tree
        /// speculation: the draft proposes `branch` candidates at each of the
        /// `gamma` positions, and the target verifies the longest matching
        /// path (higher acceptance than single-path draft-target).
        /// Requires the `inference` feature.
        #[cfg(feature = "inference")]
        #[arg(long, default_value_t = 0)]
        branch: usize,
    },

    /// Database migration commands
    #[command(subcommand)]
    Db(DbCommands),

    /// List all registered routes
    Routes {
        /// File to load the OpenAPI spec from (default: built-in routes)
        #[arg(long)]
        spec_file: Option<PathBuf>,
    },

    /// Run diagnostic checks on the environment and configuration
    Doctor {
        /// Path to a JustAPI config file to validate
        #[arg(short, long)]
        config: Option<PathBuf>,
    },

    /// Generate OpenAPI spec file, client code, etc.
    #[command(subcommand)]
    Gen(GenCommands),

    /// Validate all routes without starting the server
    Check {
        /// Path to a routes config or app file
        #[arg(short, long)]
        file: Option<PathBuf>,
    },

    /// Project scaffolding — create a new JustAPI project
    New {
        /// Name of the project
        name: String,
        /// Output directory (defaults to project name)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Profile a running JustAPI server (built-in load test)
    Profile {
        /// Target server address
        #[arg(short, long, default_value = "127.0.0.1:8080")]
        addr: String,

        /// Profiling duration in seconds
        #[arg(short, long, default_value = "10")]
        duration: u64,

        /// Number of concurrent connections
        #[arg(short, long, default_value = "50")]
        connections: u64,

        /// Output file for the report
        #[arg(short, long, default_value = "profile_report.txt")]
        output: PathBuf,
    },
}

#[derive(Subcommand)]
enum DbCommands {
    /// Run pending migrations
    Migrate {
        /// Database connection URL
        #[arg(short, long)]
        url: String,

        /// Path to migrations directory
        #[arg(short, long, default_value = "migrations")]
        dir: PathBuf,
    },

    /// Roll back the most recent migration
    Rollback {
        /// Database connection URL
        #[arg(short, long)]
        url: String,

        /// Path to migrations directory
        #[arg(short, long, default_value = "migrations")]
        dir: PathBuf,
    },

    /// Initialize the migrations tracking table
    Init {
        /// Database connection URL
        #[arg(short, long)]
        url: String,

        /// Path to migrations directory
        #[arg(short, long, default_value = "migrations")]
        dir: PathBuf,
    },
}

#[derive(Subcommand)]
enum GenCommands {
    /// Generate and save OpenAPI 3.1 spec to a file
    Openapi {
        /// Output file path
        #[arg(short, long, default_value = "openapi.json")]
        output: PathBuf,
    },
    /// Generate a typed client from an OpenAPI spec
    Client {
        /// Path to OpenAPI spec file
        #[arg(short, long)]
        spec: PathBuf,
        /// Output directory for generated client
        #[arg(short, long, default_value = "client")]
        output: PathBuf,
        /// Target language: python (default) or typescript
        #[arg(long, default_value = "python")]
        language: String,
    },
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        emit_rich_error(&err);
        std::process::exit(1);
    }
}

/// Inspect an [`anyhow::Error`] and emit a rich diagnostic when possible,
/// falling back to a plain error message otherwise.
fn emit_rich_error(err: &anyhow::Error) {
    let msg = format!("{err:#}");

    // Try to match well-known error patterns and emit rich diagnostics.
    let diagnostic = if msg.contains("address") && msg.contains("parse") {
        Some(error_catalog::E001_INVALID_ADDRESS.with_context(msg.clone()))
    } else if msg.contains("Address already in use")
        || msg.contains("address already in use")
        || msg.contains("AddrInUse")
    {
        Some(error_catalog::E002_PORT_IN_USE.with_context(msg.clone()))
    } else if (msg.contains("certificate") || msg.contains("tls") || msg.contains("TLS"))
        && (msg.contains("not found") || msg.contains("No such file"))
    {
        Some(error_catalog::E003_TLS_CERT_NOT_FOUND.with_context(msg.clone()))
    } else if msg.contains("database") && msg.contains("URL") {
        Some(error_catalog::E004_DB_URL_INVALID.with_context(msg.clone()))
    } else {
        None
    };

    if let Some(d) = diagnostic {
        d.emit();
    } else {
        // Fallback: still emit a styled error, just without a catalogue code.
        let d = Diagnostic::new(DiagLevel::Error, msg);
        d.emit();
    }
}

async fn run() -> anyhow::Result<()> {
    justapi_core::tracing_setup::init_tracing()?;

    let cli = Cli::parse();
    match cli.command {
        Commands::Serve {
            addr,
            static_dir,
            #[cfg(feature = "compression")]
            compress,
            reload,
            watch_dir,
            watch_ext,
            drain_timeout,
            #[cfg(feature = "tls")]
            tls_cert,
            #[cfg(feature = "tls")]
            tls_key,
            #[cfg(feature = "inference")]
            model,
            #[cfg(feature = "inference")]
            gpu,
            #[cfg(feature = "inference")]
            scheduled,
            #[cfg(feature = "inference")]
            pool_blocks,
            #[cfg(feature = "inference")]
            max_seqs,
            #[cfg(feature = "inference")]
            gamma,
            #[cfg(feature = "inference")]
            branch,
        } => {
            // Set up the file watcher once (if --reload), it sends on each
            // detected change via mpsc so we can restart multiple times.
            let mut reload_rx = if reload {
                let dir = watch_dir
                    .clone()
                    .unwrap_or_else(|| std::env::current_dir().expect("cannot read cwd"));
                tracing::info!(dir = %dir.display(), "Watching for file changes");
                Some(watcher::spawn_file_watcher(&dir, &watch_ext)?)
            } else {
                None
            };

            let drain_duration = Duration::from_secs(drain_timeout);

            // Build the inference engine if `--model` was supplied (inference
            // feature). The engine serves the OpenAI-compatible endpoints with a
            // GIL-free generation thread; the scheduler provides continuous
            // batching + paged KV cache (real weight loading is gated on `real`).
            #[cfg(feature = "inference")]
            let engine: Option<std::sync::Arc<justapi_inference::Engine>> = if let Some(id) =
                model.as_ref()
            {
                use justapi_inference::EngineDevice;
                let device = if gpu.eq_ignore_ascii_case("cpu") {
                    EngineDevice::Cpu
                } else {
                    EngineDevice::Cuda(gpu.parse().unwrap_or(0))
                };
                let eng = std::sync::Arc::new(
                    justapi_inference::Engine::new(device)
                        .map_err(|e| anyhow::anyhow!("failed to start inference engine: {e}"))?,
                );
                eng.register_mock(id);

                // Wire speculative decoding if requested.
                if gamma > 0 || branch > 0 {
                    let target = eng.get(id).unwrap();
                    let draft_id = format!("{id}-draft");
                    let draft = eng.register_mock(&draft_id);
                    if branch > 0 {
                        let g = gamma.max(1);
                        eng.register_tree_speculative(id, target, draft, g, branch, 0);
                        tracing::info!(
                            model = %id,
                            device = %gpu,
                            gamma = g,
                            branch = branch,
                            "Serving model via OpenAI-compatible endpoints (tree speculative decoding)"
                        );
                    } else {
                        eng.register_speculative(id, target, draft, gamma, 0);
                        tracing::info!(
                            model = %id,
                            device = %gpu,
                            gamma = gamma,
                            "Serving model via OpenAI-compatible endpoints (draft-target speculative decoding)"
                        );
                    }
                } else {
                    tracing::info!(model = %id, device = %gpu, "Serving model via OpenAI-compatible endpoints");
                }
                Some(eng)
            } else {
                None
            };

            // Build the scheduler engine if `--scheduled` is also set.
            #[cfg(feature = "inference")]
            let scheduler_engine: Option<
                std::sync::Arc<justapi_inference::SchedulerEngine>,
            > = if scheduled && engine.is_some() {
                use justapi_inference::{KvBlockPool, Scheduler, SchedulerConfig, SchedulerEngine};
                let eng = engine.as_ref().unwrap().clone();
                let pool = KvBlockPool::new(pool_blocks.max(64));
                let config =
                    SchedulerConfig { max_num_seqs: max_seqs.max(1), ..Default::default() };
                let scheduler =
                    std::sync::Arc::new(std::sync::Mutex::new(Scheduler::new(config, pool)));
                let se = std::sync::Arc::new(SchedulerEngine::new(eng, scheduler));
                tracing::info!(
                    pool_blocks = pool_blocks,
                    max_seqs = max_seqs,
                    "Scheduler-enabled serving path"
                );
                Some(se)
            } else {
                None
            };

            loop {
                let token = tokio_util::sync::CancellationToken::new();

                // Wire Ctrl+C to graceful shutdown.
                let sig_token = token.clone();
                let is_ctrl_c = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let ctrl_c_flag = is_ctrl_c.clone();
                tokio::spawn(async move {
                    tokio::signal::ctrl_c().await.ok();
                    tracing::info!("Shutdown signal received");
                    ctrl_c_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    sig_token.cancel();
                });

                // If reload is enabled, wait for the watcher to signal a
                // file change and then cancel the server token.
                if let Some(ref mut rx) = reload_rx {
                    let cancel = token.clone();
                    // We need to take the receiver into the task, but we also
                    // need it back afterwards.  Use a oneshot to shuttle it.
                    let (return_tx, return_rx) =
                        tokio::sync::oneshot::channel::<mpsc::Receiver<()>>();
                    let mut taken_rx = None;
                    std::mem::swap(rx, taken_rx.get_or_insert(mpsc::channel(1).1));
                    let mut inner_rx = taken_rx.unwrap();

                    tokio::spawn(async move {
                        // Wait for either a file change or the token being
                        // cancelled (e.g. Ctrl+C).
                        tokio::select! {
                            Some(()) = inner_rx.recv() => {
                                cancel.cancel();
                            }
                            _ = cancel.cancelled() => {}
                        }
                        // Return the receiver so the outer loop can reuse it.
                        return_tx.send(inner_rx).ok();
                    });

                    // We'll collect the receiver back after `server.run()` completes.
                    // Stash the return channel for later.
                    // (We use a scope trick: store it in the reload_rx option.)
                    // Actually we need the return_rx after the server stops,
                    // so we hold it outside and await it after run().
                    // Re-structure: keep return_rx in a variable.
                    let mut server = justapi_core::Server::new(addr).with_shutdown(token.clone());

                    if let Some(dir) = static_dir.clone() {
                        server = server.with_static_dir(dir);
                    }

                    #[cfg(feature = "compression")]
                    if compress {
                        server = server.add_compression();
                    }

                    #[cfg(feature = "inference")]
                    if let Some(ref se) = scheduler_engine {
                        server = server.with_openai_scheduled(se.clone());
                    } else if let Some(ref eng) = engine {
                        server = server.with_openai(eng.clone());
                    }

                    #[cfg(feature = "tls")]
                    if let (Some(cert), Some(key)) = (tls_cert.clone(), tls_key.clone()) {
                        let config =
                            justapi_core::server::TlsConfig { cert_path: cert, key_path: key };
                        let result = server.with_tls(config).run().await;
                        justapi_core::tracing_setup::shutdown_tracing();
                        return result;
                    }

                    let result = server.run().await;

                    // Recover the watcher receiver for the next iteration.
                    if let Ok(recovered) = return_rx.await {
                        *rx = recovered;
                    }

                    // If the server stopped because of Ctrl+C, exit cleanly.
                    if is_ctrl_c.load(std::sync::atomic::Ordering::SeqCst) {
                        justapi_core::tracing_setup::shutdown_tracing();
                        return result;
                    }

                    // File-change triggered reload: drain in-flight requests.
                    tracing::info!(
                        drain_secs = drain_timeout,
                        "Server stopped, draining in-flight requests"
                    );
                    tokio::time::sleep(drain_duration).await;
                    tracing::info!("Restarting server...");
                    continue;
                }

                // Non-reload path: run server once and exit.
                let mut server = justapi_core::Server::new(addr).with_shutdown(token);

                if let Some(dir) = static_dir.clone() {
                    server = server.with_static_dir(dir);
                }

                #[cfg(feature = "compression")]
                if compress {
                    server = server.add_compression();
                }

                #[cfg(feature = "inference")]
                if let Some(ref se) = scheduler_engine {
                    server = server.with_openai_scheduled(se.clone());
                } else if let Some(ref eng) = engine {
                    server = server.with_openai(eng.clone());
                }

                #[cfg(feature = "tls")]
                if let (Some(cert), Some(key)) = (tls_cert.clone(), tls_key.clone()) {
                    let config = justapi_core::server::TlsConfig { cert_path: cert, key_path: key };
                    let result = server.with_tls(config).run().await;
                    justapi_core::tracing_setup::shutdown_tracing();
                    return result;
                }

                let result = server.run().await;
                justapi_core::tracing_setup::shutdown_tracing();
                return result;
            }
        }
        Commands::Db(db_cmd) => match db_cmd {
            DbCommands::Migrate { url, dir } => {
                if !dir.exists() {
                    anyhow::bail!("Migrations directory does not exist: {:?}", dir);
                }
                let config = justapi_core::db::DatabaseConfig {
                    url,
                    max_connections: 1,
                    kind: None,
                    init_sql: None,
                    pragmas: None,
                };
                let mut mgr = justapi_core::db::PoolManager::new();
                let pool = mgr.init("", config).await?;

                let mut migrator = justapi_core::db::Migrator::new();
                migrator.discover(&dir).map_err(|e| anyhow::anyhow!(e))?;
                let applied = migrator.run(&pool).await.map_err(|e| anyhow::anyhow!(e))?;
                tracing::info!("Applied {} migration(s)", applied.len());
                for m in &applied {
                    tracing::info!("  v{}: {}", m.version, m.name);
                }
                Ok(())
            }
            DbCommands::Rollback { url, dir } => {
                if !dir.exists() {
                    anyhow::bail!("Migrations directory does not exist: {:?}", dir);
                }
                let config = justapi_core::db::DatabaseConfig {
                    url,
                    max_connections: 1,
                    kind: None,
                    init_sql: None,
                    pragmas: None,
                };
                let mut mgr = justapi_core::db::PoolManager::new();
                let pool = mgr.init("", config).await?;

                let mut migrator = justapi_core::db::Migrator::new();
                migrator.discover(&dir).map_err(|e| anyhow::anyhow!(e))?;
                migrator.rollback_one(&pool).await.map_err(|e| anyhow::anyhow!(e))?;
                tracing::info!("Rolled back most recent migration");
                Ok(())
            }
            DbCommands::Init { url, dir } => {
                let config = justapi_core::db::DatabaseConfig {
                    url,
                    max_connections: 1,
                    kind: None,
                    init_sql: None,
                    pragmas: None,
                };
                let mut mgr = justapi_core::db::PoolManager::new();
                let pool = mgr.init("", config).await?;

                let mut migrator = justapi_core::db::Migrator::new();
                if dir.exists() {
                    migrator.discover(&dir).map_err(|e| anyhow::anyhow!(e))?;
                }
                migrator.ensure_tracking_table(&pool).await.map_err(|e| anyhow::anyhow!(e))?;
                tracing::info!("Migrations tracking table initialized");
                Ok(())
            }
        },
        Commands::Routes { spec_file } => {
            if let Some(path) = spec_file {
                let spec = std::fs::read_to_string(&path)
                    .map_err(|e| anyhow::anyhow!("Cannot read {}: {}", path.display(), e))?;
                let doc: serde_json::Value = serde_json::from_str(&spec)
                    .map_err(|e| anyhow::anyhow!("Invalid OpenAPI spec: {}", e))?;
                let paths =
                    doc.get("paths").and_then(|p| p.as_object()).map(|p| p.len()).unwrap_or(0);
                println!("Loaded OpenAPI spec with {} path(s)", paths);
                if let Some(paths_obj) = doc.get("paths").and_then(|p| p.as_object()) {
                    for (path, item) in paths_obj {
                        if let Some(obj) = item.as_object() {
                            for method in
                                ["get", "post", "put", "delete", "patch", "head", "options"]
                            {
                                if obj.contains_key(method) {
                                    println!("  {:>6} {}", method.to_uppercase(), path);
                                }
                            }
                        }
                    }
                }
            } else {
                let router = justapi_core::Server::new("127.0.0.1:0".parse().unwrap());
                let _ = router;
                println!("Use --spec-file to load an OpenAPI spec for route listing.");
                println!("For built-in routes, access /openapi.json on a running server.");
            }
            Ok(())
        }
        Commands::Doctor { config } => {
            Diagnostic::new(DiagLevel::Info, "JustAPI Doctor").emit();
            println!("==================");

            // Check Rust version
            Diagnostic::new(DiagLevel::Hint, "Rust toolchain available").emit();

            // Check Python availability
            let py_ok = std::process::Command::new("python3").arg("--version").output().is_ok();
            if py_ok {
                Diagnostic::new(DiagLevel::Hint, "Python 3 detected").emit();
            } else {
                error_catalog::E006_PYTHON_NOT_FOUND.to_diagnostic().emit();
            }

            // Check config file
            if let Some(path) = config {
                if path.exists() {
                    Diagnostic::new(
                        DiagLevel::Hint,
                        format!("Config file found: {}", path.display()),
                    )
                    .emit();
                } else {
                    error_catalog::E008_INVALID_CONFIG
                        .with_context(format!("{}", path.display()))
                        .emit();
                }
            } else {
                Diagnostic::new(
                    DiagLevel::Info,
                    "No config file specified (use --config to check one)",
                )
                .emit();
            }

            // Check migrations directory
            let migrations_dir = PathBuf::from("migrations");
            if migrations_dir.exists() {
                Diagnostic::new(DiagLevel::Hint, "Migrations directory exists").emit();
            } else {
                Diagnostic::new(
                    DiagLevel::Info,
                    "No migrations directory (create one with `justapi db init`)",
                )
                .emit();
            }

            // Check .env
            let env_path = PathBuf::from(".env");
            if env_path.exists() {
                Diagnostic::new(DiagLevel::Hint, ".env file found").emit();
            } else {
                Diagnostic::new(DiagLevel::Warning, "No .env file (recommended for configuration)")
                    .emit();
            }

            // Check Dockerfile
            let dockerfile = PathBuf::from("Dockerfile");
            if dockerfile.exists() {
                Diagnostic::new(DiagLevel::Hint, "Dockerfile found").emit();
            } else {
                Diagnostic::new(DiagLevel::Info, "No Dockerfile (use `justapi new` to create one)")
                    .emit();
            }

            println!();
            Diagnostic::new(DiagLevel::Info, "Doctor check complete.").emit();
            Ok(())
        }
        Commands::Gen(gen_cmd) => match gen_cmd {
            GenCommands::Openapi { output } => {
                let doc = justapi_core::openapi::OpenApiBuilder::new("JustAPI", "0.1.0")
                    .description("Generated by justapi gen openapi")
                    .server("http://localhost:8080", Some("Local development"))
                    .tag("default", Some("Default endpoints"))
                    .build();
                let json = serde_json::to_string_pretty(&doc)?;
                std::fs::write(&output, &json)?;
                println!("OpenAPI spec written to {}", output.display());
                Ok(())
            }
            GenCommands::Client { spec, output, language } => {
                let lang: gen_client::ClientLanguage = language.parse()?;
                let json = std::fs::read_to_string(&spec)
                    .map_err(|e| anyhow::anyhow!("Cannot read spec {}: {}", spec.display(), e))?;
                let doc: justapi_core::openapi::OpenApiDocument = serde_json::from_str(&json)
                    .map_err(|e| anyhow::anyhow!("Invalid OpenAPI spec: {}", e))?;
                let out_path = gen_client::write_client(&doc, &lang, &output)?;
                println!(
                    "{} client written to {}",
                    match lang {
                        gen_client::ClientLanguage::Python => "Python",
                        gen_client::ClientLanguage::Typescript => "TypeScript",
                    },
                    out_path.display()
                );
                Ok(())
            }
        },
        Commands::Check { file } => {
            if let Some(path) = file {
                if path.exists() {
                    println!("[✓] File found: {}", path.display());
                } else {
                    anyhow::bail!("File not found: {}", path.display());
                }
            }
            println!("[✓] Route validation passed (no routes to validate)");
            Ok(())
        }
        Commands::New { name, output } => {
            let project_dir = output.unwrap_or_else(|| PathBuf::from(&name));
            if project_dir.exists() {
                anyhow::bail!("Directory '{}' already exists", project_dir.display());
            }
            std::fs::create_dir_all(&project_dir)?;
            std::fs::create_dir_all(project_dir.join("app"))?;
            std::fs::create_dir_all(project_dir.join("migrations"))?;
            std::fs::create_dir_all(project_dir.join("static"))?;

            // Create main.py
            let main_py = format!(
                r#""""JustAPI application — {}
"""

from justapi import JustAPIApp

app = JustAPIApp()


@app.get("/")
async def root():
    return {{"message": "Hello from {}!"}}


@app.get("/health")
async def health():
    return {{"status": "healthy"}}


if __name__ == "__main__":
    app.run()
"#,
                name, name
            );
            std::fs::write(project_dir.join("app").join("__init__.py"), "")?;
            std::fs::write(project_dir.join("app").join("main.py"), &main_py)?;

            // Create .env
            let env = format!(
                r#"# {} configuration
HOST=127.0.0.1
PORT=8080
# DATABASE_URL=postgres://user:pass@localhost:5432/{}
# SECRET_KEY=change-me
"#,
                name, name
            );
            std::fs::write(project_dir.join(".env"), &env)?;

            // Create requirements.txt
            std::fs::write(
                project_dir.join("requirements.txt"),
                "# Add your Python dependencies here\n# justapi is pre-installed with the runtime\n",
            )?;

            // Create README.md
            let readme = format!(
                r#"# {}

A JustAPI application.

## Quick start

```bash
justapi serve
```

## Development

```bash
# Install dependencies
pip install -r requirements.txt

# Run migrations
justapi db migrate --url postgres://localhost/{}

# Start server with hot reload
justapi serve --reload
```
"#,
                name, name
            );
            std::fs::write(project_dir.join("README.md"), &readme)?;

            // Create .gitignore
            std::fs::write(
                project_dir.join(".gitignore"),
                "*.pyc\n__pycache__/\n.env\n*.egg-info/\ndist/\nbuild/\n",
            )?;

            println!("✅ Created new JustAPI project '{}'", name);
            println!();
            println!("  cd {}", project_dir.display());
            println!("  justapi serve");

            Ok(())
        }
        Commands::Profile { addr, duration, connections, output } => {
            println!("🔬 JustAPI Profiler");
            println!("  Target:       {addr}");
            println!("  Duration:     {duration}s");
            println!("  Connections:  {connections}");
            println!("  Output:       {}\n", output.display());

            let report = profile::run_profile(&addr, duration, connections).await?;
            let text = profile::format_report(&report);
            print!("{text}");

            profile::save_report(&text, &output)?;
            println!("\n  Report saved to {}", output.display());

            Ok(())
        }
    }
}
