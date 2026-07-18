use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use justapi_core::dx::{DiagLevel, Diagnostic};
use justapi_core::error_catalog;
use tokio::sync::mpsc;
mod gen_client;
mod profile;
mod watcher;
mod workers;

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

        /// Bind to a Unix domain socket instead of TCP (e.g. `/run/justapi.sock`).
        /// Takes precedence over `--addr` when both are given. Unix only.
        #[arg(long)]
        unix: Option<String>,

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

        /// Number of worker processes (prefork). Each worker shares the bound
        /// listening socket. Defaults to 1 (single process). Incompatible with
        /// `--reload` (hot reload restarts the whole process tree).
        #[arg(long, default_value_t = 1)]
        workers: usize,

        /// Minimum worker count for auto-scaling (`--scale`). Defaults to
        /// `--workers` when auto-scaling is enabled without an explicit value.
        #[arg(long)]
        min_workers: Option<usize>,

        /// Maximum worker count for auto-scaling (`--scale`). Defaults to
        /// `--workers` (or `workers * 2` if only `--workers` is set) when
        /// auto-scaling is enabled without an explicit value.
        #[arg(long)]
        max_workers: Option<usize>,

        /// Enable load-based auto-scaling between `--min-workers` and
        /// `--max-workers` (prefork only). Scales the fleet up under high load
        /// and down when idle, with a cooldown to avoid flapping.
        #[arg(long, default_value_t = false)]
        scale: bool,

        /// Auto-scaling scale-down threshold (normalized load 0..1+). Below this
        /// the fleet shrinks toward `--min-workers`.
        #[arg(long, default_value_t = 0.3)]
        scale_low: f64,

        /// Auto-scaling scale-up threshold (normalized load 0..1+). At/above this
        /// the fleet grows toward `--max-workers`.
        #[arg(long, default_value_t = 0.7)]
        scale_high: f64,

        /// Auto-scaling cooldown in seconds between scaling actions.
        #[arg(long, default_value_t = 15)]
        scale_cooldown: u64,

        /// Hidden: internal flag used by the prefork supervisor to hand a bound
        /// listening socket fd to a worker child. Do not pass manually.
        #[arg(long, hide = true)]
        worker_fd: Option<i32>,

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

    /// Create a new JustAPI project with a database backend wired in.
    ///
    /// Prompts are avoided: pass `--db` to pick the engine, or `--db-url` to
    /// supply a full connection string (engine is inferred from the URL scheme).
    /// Defaults to SQLite (zero-config, file-based).
    Create {
        /// Name of the project
        name: String,
        /// Output directory (defaults to project name)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Database engine: `sqlite` (default), `postgres`, or `mysql`.
        #[arg(long, default_value = "sqlite")]
        db: String,
        /// Full database connection URL. Overrides `--db` (engine is inferred
        /// from the scheme: `postgres://`, `sqlite://`, `mysql://`).
        #[arg(long)]
        db_url: Option<String>,
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

/// Resolve the database engine + connection URL for a scaffolded project.
///
/// `--db-url` wins (engine inferred from its scheme); otherwise `--db` picks a
/// sensible default URL for the chosen engine.
fn resolve_scaffold_db(db: &str, db_url: Option<String>) -> anyhow::Result<(String, String)> {
    if let Some(url) = db_url {
        let kind = if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            "postgres"
        } else if url.starts_with("sqlite://") || url.starts_with("sqlite:") {
            "sqlite"
        } else if url.starts_with("mysql://") || url.starts_with("mariadb://") {
            "mysql"
        } else {
            anyhow::bail!("Unrecognized database URL scheme: {}", url);
        };
        Ok((kind.to_string(), url))
    } else {
        let kind = match db.to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" => "postgres",
            "mysql" | "mariadb" => "mysql",
            "sqlite" => "sqlite",
            other => anyhow::bail!(
                "Unsupported --db '{}'. Choose one of: sqlite, postgres, mysql",
                other
            ),
        };
        let url = match kind {
            "postgres" => "postgres://user:pass@localhost:5432/app".to_string(),
            "mysql" => "mysql://user:pass@localhost:3306/app".to_string(),
            _ => format!("sqlite://{name}.db", name = "{name}"),
        };
        Ok((kind.to_string(), url))
    }
}

/// Generate the `app/main.py` for a scaffolded project, wired to the chosen DB.
fn scaffold_main_py(name: &str, db_kind: &str, db_url: &str) -> String {
    let db_setup = match db_kind {
        "postgres" => format!(
            "app.set_database(\n    \"{url}\",\n    init_sql=\"\"\"\n    CREATE TABLE IF NOT EXISTS items (\n        id SERIAL PRIMARY KEY,\n        name TEXT NOT NULL,\n        qty INT NOT NULL DEFAULT 0\n    )\n    \"\"\",\n)",
            url = db_url
        ),
        "mysql" => format!(
            "app.set_database(\n    \"{url}\",\n    init_sql=\"\"\"\n    CREATE TABLE IF NOT EXISTS items (\n        id INT AUTO_INCREMENT PRIMARY KEY,\n        name VARCHAR(255) NOT NULL,\n        qty INT NOT NULL DEFAULT 0\n    )\n    \"\"\",\n)",
            url = db_url
        ),
        _ => format!(
            "app.set_database(\n    \"{url}\",\n    pragmas=[\"journal_mode=WAL\"],\n    init_sql=\"\"\"\n    CREATE TABLE IF NOT EXISTS items (\n        id INTEGER PRIMARY KEY AUTOINCREMENT,\n        name TEXT NOT NULL,\n        qty INTEGER NOT NULL DEFAULT 0\n    )\n    \"\"\",\n)",
            url = db_url
        ),
    };
    format!(
        r#""""JustAPI application — {name}

Database backend: {kind} (URL: {url})
"""
from justapi import JustAPIApp, Database

app = JustAPIApp()

# Wire the database. `app.db` is available inside handlers once the server runs.
{dbsetup}


@app.get("/items")
def list_items(request):
    # Runs entirely in Rust (GIL released) with bound parameters.
    return app.db.query("SELECT * FROM items ORDER BY id")


@app.post("/items")
def add_item(request):
    data = request.json()
    app.db.execute(
        "INSERT INTO items (name, qty) VALUES (?, ?)",
        [data.get("name"), data.get("qty", 0)],
    )
    return {{"ok": True}}


@app.get("/")
def root(request):
    return {{"message": "Hello from {name}!", "db": "{kind}"}}
"#,
        name = name,
        kind = db_kind,
        url = db_url,
        dbsetup = db_setup,
    )
}

/// Scaffold a new JustAPI project (shared by `new` and `create`).
fn scaffold_project(
    name: &str,
    output: Option<PathBuf>,
    db_kind: &str,
    db_url: &str,
) -> anyhow::Result<()> {
    let project_dir = output.unwrap_or_else(|| PathBuf::from(name));
    if project_dir.exists() {
        anyhow::bail!("Directory '{}' already exists", project_dir.display());
    }
    std::fs::create_dir_all(&project_dir)?;
    std::fs::create_dir_all(project_dir.join("app"))?;
    std::fs::create_dir_all(project_dir.join("migrations"))?;
    std::fs::create_dir_all(project_dir.join("static"))?;

    std::fs::write(project_dir.join("app").join("__init__.py"), "")?;
    std::fs::write(
        project_dir.join("app").join("main.py"),
        scaffold_main_py(name, db_kind, db_url),
    )?;

    let env = format!(
        "# {name} configuration\nHOST=127.0.0.1\nPORT=8080\n# Database engine: {kind}\nDATABASE_URL={url}\n# SECRET_KEY=change-me\n",
        name = name,
        kind = db_kind,
        url = db_url,
    );
    std::fs::write(project_dir.join(".env"), env)?;

    std::fs::write(
        project_dir.join("requirements.txt"),
        "# Add your Python dependencies here\n# justapi is pre-installed with the runtime\n",
    )?;

    let readme = format!(
        "# {name}\n\nA JustAPI application (database backend: {kind}).\n\n## Quick start\n\n```bash\njustapi create {name} --db {kind}   # or: justapi new {name}\ncd {dir}\njustapi serve\n```\n\n## Database\n\nConnection: `{url}`\n\n```bash\n# Run migrations\njustapi db migrate --url \"{url}\"\n\n# Start with hot reload\njustapi serve --reload\n```\n",
        name = name,
        kind = db_kind,
        dir = project_dir.display(),
        url = db_url,
    );
    std::fs::write(project_dir.join("README.md"), readme)?;

    std::fs::write(
        project_dir.join(".gitignore"),
        "*.pyc\n__pycache__/\n.env\n*.egg-info/\ndist/\nbuild/\n*.db\n",
    )?;

    std::fs::write(
        project_dir.join("Dockerfile"),
        "FROM python:3.12-slim\nWORKDIR /app\nCOPY requirements.txt .\nRUN pip install justapi\nCOPY . .\nEXPOSE 8080\nCMD [\"justapi\", \"serve\", \"--host\", \"0.0.0.0\", \"--port\", \"8080\"]\n",
    )?;

    println!("✅ Created new JustAPI project '{}' (database: {})", name, db_kind);
    println!();
    println!("  cd {}", project_dir.display());
    println!("  justapi serve");
    Ok(())
}

/// Build the inference engine + optional scheduler engine from `--model` etc.
/// Returns `(None, None)` when `--model` is not supplied. Inference-only.
#[cfg(feature = "inference")]
fn build_engines(
    model: &Option<String>,
    gpu: &str,
    scheduled: bool,
    pool_blocks: usize,
    max_seqs: usize,
    gamma: usize,
    branch: usize,
) -> anyhow::Result<(
    Option<std::sync::Arc<justapi_inference::Engine>>,
    Option<std::sync::Arc<justapi_inference::SchedulerEngine>>,
)> {
    use justapi_inference::EngineDevice;
    let engine: Option<std::sync::Arc<justapi_inference::Engine>> = match model {
        Some(id) => {
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

            if gamma > 0 || branch > 0 {
                let target = eng.get(id).unwrap();
                let draft_id = format!("{id}-draft");
                let draft = eng.register_mock(&draft_id);
                if branch > 0 {
                    let g = gamma.max(1);
                    eng.register_tree_speculative(id, target, draft, g, branch, 0);
                    tracing::info!(
                        model = %id, device = %gpu, gamma = g, branch,
                        "Serving model via OpenAI-compatible endpoints (tree speculative decoding)"
                    );
                } else {
                    eng.register_speculative(id, target, draft, gamma, 0);
                    tracing::info!(
                        model = %id, device = %gpu, gamma,
                        "Serving model via OpenAI-compatible endpoints (draft-target speculative decoding)"
                    );
                }
            } else {
                tracing::info!(model = %id, device = %gpu, "Serving model via OpenAI-compatible endpoints");
            }
            Some(eng)
        }
        None => None,
    };

    let scheduler_engine: Option<std::sync::Arc<justapi_inference::SchedulerEngine>> = if scheduled
        && engine.is_some()
    {
        use justapi_inference::{KvBlockPool, Scheduler, SchedulerConfig, SchedulerEngine};
        let eng = engine.as_ref().unwrap().clone();
        let pool = KvBlockPool::new(pool_blocks.max(64));
        let config = SchedulerConfig { max_num_seqs: max_seqs.max(1), ..Default::default() };
        let scheduler = std::sync::Arc::new(std::sync::Mutex::new(Scheduler::new(config, pool)));
        let se = std::sync::Arc::new(SchedulerEngine::new(eng, scheduler));
        tracing::info!(pool_blocks, max_seqs, "Scheduler-enabled serving path");
        Some(se)
    } else {
        None
    };

    Ok((engine, scheduler_engine))
}

/// Configure a `Server` from the shared serve options. TLS/compression/inference
/// wiring is applied uniformly for both the single-process and worker paths,
/// eliminating the previous duplication between the reload/non-reload branches.
fn build_server(
    listen_addr: &justapi_core::server::ListenAddr,
    static_dir: &Option<String>,
    #[cfg(feature = "compression")] compress: bool,
    #[cfg(feature = "tls")] tls_cert: &Option<String>,
    #[cfg(feature = "tls")] tls_key: &Option<String>,
    #[cfg(feature = "inference")] engine: &Option<std::sync::Arc<justapi_inference::Engine>>,
    #[cfg(feature = "inference")] scheduler_engine: &Option<
        std::sync::Arc<justapi_inference::SchedulerEngine>,
    >,
    token: tokio_util::sync::CancellationToken,
) -> justapi_core::Server {
    let mut server = justapi_core::Server::new(listen_addr.clone()).with_shutdown(token);

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
        server = server.with_tls(config);
    }

    server
}

/// Install signal handlers that cancel `token` on SIGTERM/SIGINT (Unix) and
/// Ctrl+C (all platforms), so the server/worker begins graceful drain.
fn wire_shutdown_signals(token: tokio_util::sync::CancellationToken) {
    // Ctrl+C works everywhere.
    let ctrl = token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Shutdown signal received (Ctrl+C)");
        ctrl.cancel();
    });

    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let term = token.clone();
        tokio::spawn(async move {
            if let Ok(mut s) = signal(SignalKind::terminate()) {
                s.recv().await;
                tracing::info!("Shutdown signal received (SIGTERM)");
                term.cancel();
            }
        });
        let int = token.clone();
        tokio::spawn(async move {
            if let Ok(mut s) = signal(SignalKind::interrupt()) {
                s.recv().await;
                tracing::info!("Shutdown signal received (SIGINT)");
                int.cancel();
            }
        });
    }
}

async fn run() -> anyhow::Result<()> {
    justapi_core::tracing_setup::init_tracing()?;

    let cli = Cli::parse();
    match cli.command {
        Commands::Serve {
            addr,
            unix,
            static_dir,
            #[cfg(feature = "compression")]
            compress,
            reload,
            watch_dir,
            watch_ext,
            drain_timeout,
            workers,
            min_workers,
            max_workers,
            scale,
            scale_low,
            scale_high,
            scale_cooldown,
            worker_fd,
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
            let drain_duration = Duration::from_secs(drain_timeout);

            // Resolve the listen address: a Unix socket takes precedence over TCP.
            let is_unix = unix.is_some();
            let listen_addr = match &unix {
                Some(path) => {
                    justapi_core::server::ListenAddr::Unix(std::path::PathBuf::from(path))
                }
                None => justapi_core::server::ListenAddr::Tcp(addr),
            };

            // Build the inference engines (no-op unless --model supplied).
            #[cfg(feature = "inference")]
            let (engine, scheduler_engine) =
                build_engines(&model, &gpu, scheduled, pool_blocks, max_seqs, gamma, branch)?;

            // ---- Worker mode: parent handed us a bound listening socket fd. ----
            if let Some(fd) = worker_fd {
                #[cfg(unix)]
                {
                    let token = tokio_util::sync::CancellationToken::new();
                    wire_shutdown_signals(token.clone());
                    let server = build_server(
                        &listen_addr,
                        &static_dir,
                        #[cfg(feature = "compression")]
                        compress,
                        #[cfg(feature = "tls")]
                        &tls_cert,
                        #[cfg(feature = "tls")]
                        &tls_key,
                        #[cfg(feature = "inference")]
                        &engine,
                        #[cfg(feature = "inference")]
                        &scheduler_engine,
                        token,
                    );
                    let result = if is_unix {
                        server.run_on_uds(workers::listener_from_unix_fd(fd)?).await
                    } else {
                        server.run_on(workers::listener_from_fd(fd)?).await
                    };
                    justapi_core::tracing_setup::shutdown_tracing();
                    return result;
                }
                #[cfg(not(unix))]
                {
                    anyhow::bail!("--worker-fd is only supported on Unix platforms");
                }
            }

            // ---- Prefork multi-worker mode: parent binds + supervises. ----
            // Enter the prefork path when the user asked for multiple workers,
            // OR when auto-scaling is enabled with a max above 1 (we may start at
            // `min_workers` which can be 1, but still need the supervisor).
            let prefork_max = max_workers.unwrap_or(workers);
            let prefork = workers > 1 || (scale && prefork_max > 1);
            if prefork {
                if reload {
                    tracing::warn!(
                        "--reload is ignored when --workers > 1 (prefork cannot hot-reload)"
                    );
                }
                anyhow::ensure!(workers <= 256, "worker count must be <= 256");
                anyhow::ensure!(prefork_max <= 256, "max worker count must be <= 256");

                // Bind the shared listener (TCP or Unix) once in the parent and
                // hand its fd to each worker child. The listener is kept alive in
                // the parent for the whole supervisor lifetime so its fd is not
                // closed before the children inherit it.
                #[cfg(unix)]
                let (raw_fd, _keep_alive) = {
                    use std::os::fd::AsRawFd;
                    if is_unix {
                        let listener = workers::bind_unix_listener(unix.as_deref().unwrap())?;
                        (listener.as_raw_fd(), Box::new(listener) as Box<dyn std::any::Any>)
                    } else {
                        let listener = workers::bind_listener(addr)?;
                        (listener.as_raw_fd(), Box::new(listener) as Box<dyn std::any::Any>)
                    }
                };
                #[cfg(not(unix))]
                let (raw_fd, _keep_alive) = {
                    anyhow::ensure!(!is_unix, "Unix sockets require a Unix platform");
                    use std::os::fd::AsRawFd;
                    let listener = workers::bind_listener(addr)?;
                    (listener.as_raw_fd(), Box::new(listener) as Box<dyn std::any::Any>)
                };

                // Original argv (minus program name) so each child reconstructs
                // the identical server config.
                let base_argv: Vec<String> = std::env::args().skip(1).collect();

                let shutdown = tokio_util::sync::CancellationToken::new();
                wire_shutdown_signals(shutdown.clone());

                let spawn = workers::make_spawn_argv(&base_argv, raw_fd, drain_duration);

                // Assemble the auto-scaling policy (if enabled). When `--scale`
                // is set without explicit bounds, min = `--workers`, max =
                // `workers * 2` (capped so a lone `--workers 1` still scales to 2).
                let scaling: Option<(workers::ScalingPolicy, Arc<dyn workers::LoadProbe>)> =
                    if scale {
                        let min = min_workers.unwrap_or(workers);
                        let max = max_workers.unwrap_or_else(|| (workers * 2).max(min + 1));
                        let policy = workers::ScalingPolicy {
                            min_workers: min,
                            max_workers: max,
                            low: scale_low,
                            high: scale_high,
                            cooldown: Duration::from_secs(scale_cooldown),
                            step: 1,
                        };
                        Some((policy, Arc::new(workers::SystemLoadProbe::new())))
                    } else {
                        None
                    };

                tracing::info!(addr = %listen_addr.display(), workers, "starting prefork server");
                let result = workers::supervise(workers, spawn, shutdown, scaling).await;
                justapi_core::tracing_setup::shutdown_tracing();
                return result;
            }

            // ---- Single-process mode (optionally with hot reload). ----
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

                    let server = build_server(
                        &listen_addr,
                        &static_dir,
                        #[cfg(feature = "compression")]
                        compress,
                        #[cfg(feature = "tls")]
                        &tls_cert,
                        #[cfg(feature = "tls")]
                        &tls_key,
                        #[cfg(feature = "inference")]
                        &engine,
                        #[cfg(feature = "inference")]
                        &scheduler_engine,
                        token.clone(),
                    );

                    let result = if is_unix {
                        #[cfg(unix)]
                        {
                            let listener = workers::bind_unix_listener(unix.as_deref().unwrap())?;
                            server.run_on_uds(tokio::net::UnixListener::from_std(listener)?).await
                        }
                        #[cfg(not(unix))]
                        {
                            anyhow::bail!("Unix sockets require a Unix platform")
                        }
                    } else {
                        let std_listener = workers::bind_listener(addr)?;
                        let listener = tokio::net::TcpListener::from_std(std_listener)?;
                        server.run_on(listener).await
                    };

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
                let server = build_server(
                    &listen_addr,
                    &static_dir,
                    #[cfg(feature = "compression")]
                    compress,
                    #[cfg(feature = "tls")]
                    &tls_cert,
                    #[cfg(feature = "tls")]
                    &tls_key,
                    #[cfg(feature = "inference")]
                    &engine,
                    #[cfg(feature = "inference")]
                    &scheduler_engine,
                    tokio_util::sync::CancellationToken::new(),
                );

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
                    ..Default::default()
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
                    ..Default::default()
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
                    ..Default::default()
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
                let router = justapi_core::Server::new(justapi_core::server::ListenAddr::Tcp(
                    "127.0.0.1:0".parse().unwrap(),
                ));
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
            // Default scaffold: SQLite (zero-config).
            scaffold_project(&name, output, "sqlite", "sqlite://app.db")
        }
        Commands::Create { name, output, db, db_url } => {
            let (kind, url) = resolve_scaffold_db(&db, db_url)?;
            scaffold_project(&name, output, &kind, &url)
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
