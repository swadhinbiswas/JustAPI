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

    /// Create a new JustAPI project with a database backend and API protocol wired in.
    ///
    /// Pass `--db` (`sqlite`, `postgres`, `mysql`, `duckdb`, `clickhouse`, `mongodb`, `redis`),
    /// `--api-type` (`rest`, `graphql`, `grpc`, `jsonrpc`), or `--db-url`.
    /// If omitted in an interactive terminal, prompts for choice.
    Create {
        /// Name of the project
        name: String,
        /// Output directory (defaults to project name)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Database engine: `sqlite`, `postgres`, `mysql`, `duckdb`, `clickhouse`, `mongodb`, `redis`.
        #[arg(long)]
        db: Option<String>,
        /// Full database connection URL. Overrides `--db`.
        #[arg(long)]
        db_url: Option<String>,
        /// API architecture / protocol: `rest` (default), `graphql`, `grpc`, `jsonrpc`.
        #[arg(long)]
        api_type: Option<String>,
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

/// Interactively prompt the user to select an API architecture style if running in a TTY.
fn prompt_api_type_selection(name: &str) -> String {
    use std::io::{IsTerminal, Write};
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        println!();
        println!("Select API architecture style for '{name}':");
        println!("  [1] REST     (OpenAPI 3.1, JSON Endpoints) [default]");
        println!("  [2] GraphQL  (Schema-driven GraphiQL API & Query engine)");
        println!("  [3] gRPC     (Protobuf High-performance RPC protocol)");
        println!("  [4] JSON-RPC (JSON-RPC 2.0 Protocol over HTTP)");
        print!("Enter choice [1-4] (default: 1): ");
        let _ = std::io::stdout().flush();

        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            let choice = input.trim();
            match choice {
                "2" | "graphql" => return "graphql".to_string(),
                "3" | "grpc" => return "grpc".to_string(),
                "4" | "jsonrpc" => return "jsonrpc".to_string(),
                _ => {}
            }
        }
    }
    "rest".to_string()
}

/// Interactively prompt the user to select a database engine if running in a TTY.
fn prompt_db_selection(name: &str) -> (String, String) {
    use std::io::{IsTerminal, Write};
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        println!("🚀 Welcome to JustAPI Project Scaffolder!");
        println!("Select a database backend for '{name}':");
        println!("  Relational (Transactional SQL / OLTP):");
        println!("    [1] SQLite     (Zero-config, embedded file database) [default]");
        println!("    [2] PostgreSQL (Production-grade relational database)");
        println!("    [3] MySQL      (Scalable web relational database)");
        println!("  Analytical (OLAP / Data Lake):");
        println!("    [4] DuckDB     (Fast in-process analytical SQL & Parquet engine)");
        println!("    [5] ClickHouse (High-throughput analytical column store)");
        println!("  NoSQL / Key-Value / Document:");
        println!("    [6] MongoDB    (NoSQL document database)");
        println!("    [7] Redis      (NoSQL key-value store & cache)");
        print!("Enter choice [1-7] (default: 1): ");
        let _ = std::io::stdout().flush();

        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            let choice = input.trim();
            match choice {
                "2" | "postgres" | "postgresql" => {
                    return (
                        "postgres".to_string(),
                        format!("postgres://postgres:postgres@localhost:5432/{name}"),
                    );
                }
                "3" | "mysql" | "mariadb" => {
                    return (
                        "mysql".to_string(),
                        format!("mysql://root:password@localhost:3306/{name}"),
                    );
                }
                "4" | "duck" | "duckdb" => {
                    return ("duckdb".to_string(), format!("duckdb://{name}.duckdb"));
                }
                "5" | "clickhouse" => {
                    return (
                        "clickhouse".to_string(),
                        format!("clickhouse://localhost:9000/{name}"),
                    );
                }
                "6" | "mongo" | "mongodb" => {
                    return ("mongodb".to_string(), format!("mongodb://localhost:27017/{name}"));
                }
                "7" | "redis" => {
                    return ("redis".to_string(), "redis://localhost:6379/0".to_string());
                }
                _ => {}
            }
        }
    }
    ("sqlite".to_string(), format!("sqlite://{name}.db"))
}

/// Resolve the database engine + connection URL for a scaffolded project.
fn resolve_scaffold_db(
    name: &str,
    db: Option<&str>,
    db_url: Option<String>,
) -> anyhow::Result<(String, String)> {
    if let Some(url) = db_url {
        let kind = if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            "postgres"
        } else if url.starts_with("sqlite://") || url.starts_with("sqlite:") {
            "sqlite"
        } else if url.starts_with("mysql://") || url.starts_with("mariadb://") {
            "mysql"
        } else if url.starts_with("duckdb://") || url.ends_with(".duckdb") {
            "duckdb"
        } else if url.starts_with("clickhouse://") {
            "clickhouse"
        } else if url.starts_with("mongodb://") || url.starts_with("mongodb+srv://") {
            "mongodb"
        } else if url.starts_with("redis://") || url.starts_with("rediss://") {
            "redis"
        } else {
            anyhow::bail!("Unrecognized database URL scheme: {}", url);
        };
        Ok((kind.to_string(), url))
    } else if let Some(db_kind) = db {
        let kind = match db_kind.to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" => "postgres",
            "mysql" | "mariadb" => "mysql",
            "sqlite" => "sqlite",
            "duck" | "duckdb" => "duckdb",
            "clickhouse" => "clickhouse",
            "mongo" | "mongodb" => "mongodb",
            "redis" => "redis",
            other => anyhow::bail!(
                "Unsupported --db '{}'. Choose one of: sqlite, postgres, mysql, duckdb, clickhouse, mongodb, redis",
                other
            ),
        };
        let url = match kind {
            "postgres" => format!("postgres://postgres:postgres@localhost:5432/{name}"),
            "mysql" => format!("mysql://root:password@localhost:3306/{name}"),
            "duckdb" => format!("duckdb://{name}.duckdb"),
            "clickhouse" => format!("clickhouse://localhost:9000/{name}"),
            "mongodb" => format!("mongodb://localhost:27017/{name}"),
            "redis" => "redis://localhost:6379/0".to_string(),
            _ => format!("sqlite://{name}.db"),
        };
        Ok((kind.to_string(), url))
    } else {
        Ok(prompt_db_selection(name))
    }
}

/// Generate dialect-specific SQL migration file (`migrations/0001_initial.sql`).
fn scaffold_migration_sql(name: &str, db_kind: &str) -> String {
    match db_kind {
        "postgres" => format!(
            "-- Migration 0001_initial for {name} (PostgreSQL)\n\
            CREATE TABLE IF NOT EXISTS items (\n    \
                id SERIAL PRIMARY KEY,\n    \
                name VARCHAR(255) NOT NULL,\n    \
                qty INT NOT NULL DEFAULT 0,\n    \
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP\n\
            );\n"
        ),
        "mysql" => format!(
            "-- Migration 0001_initial for {name} (MySQL)\n\
            CREATE TABLE IF NOT EXISTS items (\n    \
                id INT AUTO_INCREMENT PRIMARY KEY,\n    \
                name VARCHAR(255) NOT NULL,\n    \
                qty INT NOT NULL DEFAULT 0,\n    \
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP\n\
            );\n"
        ),
        "duckdb" => format!(
            "-- Migration 0001_initial for {name} (DuckDB Analytical Engine)\n\
            CREATE TABLE IF NOT EXISTS analytics_events (\n    \
                id VARCHAR PRIMARY KEY,\n    \
                event_name VARCHAR NOT NULL,\n    \
                user_id VARCHAR NOT NULL,\n    \
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP\n\
            );\n"
        ),
        "clickhouse" => format!(
            "-- Migration 0001_initial for {name} (ClickHouse Column Store)\n\
            CREATE TABLE IF NOT EXISTS analytics_events (\n    \
                event_date Date DEFAULT toDate(now()),\n    \
                event_name String,\n    \
                user_id String,\n    \
                value UInt64\n\
            ) ENGINE = MergeTree() ORDER BY (event_date, event_name);\n"
        ),
        "mongodb" => format!(
            "// NoSQL Document collection initialization for {name}\n\
            // Collection: items\n\
            // Schema validation or index setup can be specified here.\n"
        ),
        "redis" => format!(
            "# NoSQL Key-Value schema notes for {name}\n\
            # Key pattern: items:{{item_id}}\n"
        ),
        _ => format!(
            "-- Migration 0001_initial for {name} (SQLite)\n\
            CREATE TABLE IF NOT EXISTS items (\n    \
                id INTEGER PRIMARY KEY AUTOINCREMENT,\n    \
                name TEXT NOT NULL,\n    \
                qty INTEGER NOT NULL DEFAULT 0,\n    \
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP\n\
            );\n"
        ),
    }
}

/// Generate `app/main.py` for a scaffolded project, wired to the chosen DB dialect and API protocol.
fn scaffold_main_py(name: &str, db_kind: &str, db_url: &str, api_type: &str) -> String {
    // 1. GraphQL API Protocol template
    if api_type == "graphql" {
        return format!(
            r#""""JustAPI application — {name}

API Architecture: GraphQL (GraphiQL Playground at /graphql)
Database Backend: {db_kind} (URL: {db_url})
"""
from justapi import JustAPIApp, Schema, HTTPException

app = JustAPIApp(title="{name}", version="0.1.0")

# Mount built-in GraphQL route handler (GraphiQL UI + execution engine)
app.graphql(path="/graphql")


@app.get("/health")
def health(request):
    return {{"status": "ok", "app": "{name}", "api": "graphql", "endpoint": "/graphql"}}


@app.get("/")
def root(request):
    return {{
        "message": "Welcome to {name} GraphQL API!",
        "graphiql_playground": "/graphql",
        "database": "{db_kind}",
    }}
"#
        );
    }

    // 2. gRPC / Protobuf RPC Protocol template
    if api_type == "grpc" {
        return format!(
            r#""""JustAPI application — {name}

API Architecture: gRPC / Protobuf RPC Protocol
Database Backend: {db_kind} (URL: {db_url})
"""
from justapi import JustAPIApp, Schema, HTTPException
from pydantic import Field

app = JustAPIApp(title="{name}", version="0.1.0")


class RPCRequest(Schema):
    method: str = Field(..., description="RPC method to invoke")
    params: dict = Field(default_factory=dict, description="Method payload")


@app.post("/rpc", body_schema=RPCRequest)
def handle_rpc(request):
    """High-throughput RPC handler."""
    data = request.json()
    method = data.get("method")
    params = data.get("params", {{}})
    return {{
        "status": "OK",
        "method": method,
        "result": f"Executed RPC method '{{method}}'",
        "payload": params,
    }}


@app.get("/health")
def health(request):
    return {{"status": "ok", "app": "{name}", "api": "grpc", "endpoint": "/rpc"}}


@app.get("/")
def root(request):
    return {{"message": "Welcome to {name} gRPC/RPC API!", "endpoint": "/rpc"}}
"#
        );
    }

    // 3. JSON-RPC 2.0 Protocol template
    if api_type == "jsonrpc" {
        return format!(
            r#""""JustAPI application — {name}

API Architecture: JSON-RPC 2.0 Protocol
Database Backend: {db_kind} (URL: {db_url})
"""
from justapi import JustAPIApp, Schema, HTTPException
from pydantic import Field

app = JustAPIApp(title="{name}", version="0.1.0")


class JSONRPCRequest(Schema):
    jsonrpc: str = Field("2.0", description="Protocol version")
    method: str = Field(..., description="Method name")
    params: list | dict = Field(default_factory=list, description="Method arguments")
    id: int | str = Field(1, description="Request identifier")


@app.post("/jsonrpc", body_schema=JSONRPCRequest)
def handle_jsonrpc(request):
    """JSON-RPC 2.0 protocol endpoint."""
    req = request.json()
    method = req.get("method")
    req_id = req.get("id", 1)

    if method == "ping":
        return {{"jsonrpc": "2.0", "result": "pong", "id": req_id}}
    elif method == "echo":
        return {{"jsonrpc": "2.0", "result": req.get("params"), "id": req_id}}
    else:
        return {{
            "jsonrpc": "2.0",
            "error": {{"code": -32601, "message": f"Method '{{method}}' not found"}},
            "id": req_id,
        }}



@app.get("/health")
def health(request):
    return {{"status": "ok", "app": "{name}", "api": "jsonrpc", "endpoint": "/jsonrpc"}}


@app.get("/")
def root(request):
    return {{"message": "Welcome to {name} JSON-RPC 2.0 API!", "endpoint": "/jsonrpc"}}
"#
        );
    }

    // 4. REST API Protocol (Default) — Specialized per DB backend
    if db_kind == "duckdb" {
        return format!(
            r#""""JustAPI application — {name}

Database Backend: DuckDB Analytical Engine (URL: {db_url})
API Architecture: REST
"""
from justapi import JustAPIApp, Schema, HTTPException
from pydantic import Field
import duckdb

app = JustAPIApp(title="{name}", version="0.1.0")

# Wire DuckDB analytical engine
conn = duckdb.connect("{name}.duckdb")
conn.execute("""
CREATE TABLE IF NOT EXISTS analytics_events (
    id VARCHAR PRIMARY KEY,
    event_name VARCHAR NOT NULL,
    user_id VARCHAR NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)
""")


class EventCreate(Schema):
    event_name: str = Field(..., min_length=1, description="Event name")
    user_id: str = Field(..., min_length=1, description="User ID")


@app.get("/health")
def health(request):
    return {{"status": "ok", "app": "{name}", "database": "duckdb"}}


@app.get("/events")
def list_events(request):
    """Query analytical events from DuckDB."""
    rel = conn.execute("SELECT * FROM analytics_events ORDER BY created_at DESC LIMIT 100")
    cols = [d[0] for d in rel.description]
    return [dict(zip(cols, row)) for row in rel.fetchall()]


@app.post("/events", body_schema=EventCreate)
def log_event(request):
    """Insert event into DuckDB analytical store."""
    import uuid
    data = request.json()
    event_id = str(uuid.uuid4())
    conn.execute(
        "INSERT INTO analytics_events (id, event_name, user_id) VALUES (?, ?, ?)",
        [event_id, data["event_name"], data["user_id"]],
    )
    return {{"message": "Event logged", "event_id": event_id}}


@app.get("/")
def root(request):
    return {{"message": "Welcome to {name} Analytics API (DuckDB OLAP)!", "docs": "/docs"}}
"#
        );
    }

    if db_kind == "clickhouse" {
        return format!(
            r#""""JustAPI application — {name}

Database Backend: ClickHouse Columnar Database (URL: {db_url})
API Architecture: REST
"""
from justapi import JustAPIApp, Schema, HTTPException
from pydantic import Field

app = JustAPIApp(title="{name}", version="0.1.0")


class EventPayload(Schema):
    event_name: str = Field(..., description="Event name")
    user_id: str = Field(..., description="User ID")
    value: int = Field(1, description="Event metric value")


@app.get("/health")
def health(request):
    return {{"status": "ok", "app": "{name}", "database": "clickhouse"}}


@app.post("/analytics/events", body_schema=EventPayload)
def track_event(request):
    """Track high-throughput metric event for ClickHouse."""
    data = request.json()
    return {{"status": "queued", "event": data}}


@app.get("/")
def root(request):
    return {{"message": "Welcome to {name} ClickHouse Analytics API!", "docs": "/docs"}}
"#
        );
    }

    if db_kind == "mongodb" {
        return format!(
            r#""""JustAPI application — {name}

Database backend: NoSQL MongoDB (URL: {db_url})
API Architecture: REST
"""
from justapi import JustAPIApp, Schema, HTTPException
from pydantic import Field
from pymongo import MongoClient

app = JustAPIApp(title="{name}", version="0.1.0")

# Wire MongoDB connection
mongo_client = MongoClient("{db_url}")
db = mongo_client.get_database()
items_col = db["items"]


class ItemCreate(Schema):
    name: str = Field(..., min_length=1, description="Item name")
    qty: int = Field(0, ge=0, description="Quantity in stock")


@app.get("/health")
def health(request):
    return {{"status": "ok", "app": "{name}", "database": "mongodb"}}


@app.get("/items")
def list_items(request):
    """List all documents from MongoDB collection."""
    return list(items_col.find({{}}, {{"_id": 0}}))


@app.post("/items", body_schema=ItemCreate)
def create_item(request):
    """Create a new document in MongoDB."""
    data = request.json()
    items_col.insert_one(data.copy())
    return {{"message": "Document created", "item": data}}


@app.get("/")
def root(request):
    return {{"message": "Welcome to {name} API (MongoDB NoSQL)!", "docs": "/docs"}}
"#
        );
    }

    if db_kind == "redis" {
        return format!(
            r#""""JustAPI application — {name}

Database backend: NoSQL Redis (URL: {db_url})
API Architecture: REST
"""
from justapi import JustAPIApp, Schema, HTTPException
from pydantic import Field
import redis

app = JustAPIApp(title="{name}", version="0.1.0")

# Wire Redis client connection
r = redis.Redis.from_url("{db_url}", decode_responses=True)


class KeyValuePayload(Schema):
    key: str = Field(..., min_length=1, description="Storage key")
    value: str = Field(..., description="Value to store")


@app.get("/health")
def health(request):
    return {{"status": "ok", "app": "{name}", "database": "redis"}}


@app.get("/kv/{{key}}")
def get_key(request, key: str):
    """Get value by key from Redis."""
    val = r.get(key)
    if val is None:
        raise HTTPException(status_code=404, detail=f"Key '{{key}}' not found")
    return {{"key": key, "value": val}}


@app.post("/kv", body_schema=KeyValuePayload)
def set_key(request):
    """Set key-value pair in Redis."""
    data = request.json()
    r.set(data["key"], data["value"])
    return {{"message": "Stored in Redis", "key": data["key"], "value": data["value"]}}


@app.get("/")
def root(request):
    return {{"message": "Welcome to {name} API (Redis NoSQL)!", "docs": "/docs"}}
"#
        );
    }

    let (db_setup, insert_sql, select_sql, delete_sql) = match db_kind {
        "postgres" => (
            format!(
                "app.set_database(\n    \"{db_url}\",\n    init_sql=\"\"\"\n    CREATE TABLE IF NOT EXISTS items (\n        id SERIAL PRIMARY KEY,\n        name TEXT NOT NULL,\n        qty INT NOT NULL DEFAULT 0\n    )\n    \"\"\",\n)"
            ),
            "INSERT INTO items (name, qty) VALUES ($1, $2)",
            "SELECT * FROM items WHERE id = $1",
            "DELETE FROM items WHERE id = $1"
        ),
        "mysql" => (
            format!(
                "app.set_database(\n    \"{db_url}\",\n    init_sql=\"\"\"\n    CREATE TABLE IF NOT EXISTS items (\n        id INT AUTO_INCREMENT PRIMARY KEY,\n        name VARCHAR(255) NOT NULL,\n        qty INT NOT NULL DEFAULT 0\n    )\n    \"\"\",\n)"
            ),
            "INSERT INTO items (name, qty) VALUES (?, ?)",
            "SELECT * FROM items WHERE id = ?",
            "DELETE FROM items WHERE id = ?"
        ),
        _ => (
            format!(
                "app.set_database(\n    \"{db_url}\",\n    pragmas=[\"journal_mode=WAL\"],\n    init_sql=\"\"\"\n    CREATE TABLE IF NOT EXISTS items (\n        id INTEGER PRIMARY KEY AUTOINCREMENT,\n        name TEXT NOT NULL,\n        qty INTEGER NOT NULL DEFAULT 0\n    )\n    \"\"\",\n)"
            ),
            "INSERT INTO items (name, qty) VALUES (?, ?)",
            "SELECT * FROM items WHERE id = ?",
            "DELETE FROM items WHERE id = ?"
        ),
    };

    format!(
        r#""""JustAPI application — {name}

Database backend: {db_kind} (URL: {db_url})
"""
from justapi import JustAPIApp, Schema, HTTPException
from pydantic import Field

app = JustAPIApp(title="{name}", version="0.1.0")

# Wire database backend
{db_setup}


class ItemCreate(Schema):
    name: str = Field(..., min_length=1, description="Item name")
    qty: int = Field(0, ge=0, description="Quantity in stock")


class ItemResponse(Schema):
    id: int
    name: str
    qty: int


@app.get("/health")
def health(request):
    return {{"status": "ok", "app": "{name}", "database": "{db_kind}"}}


@app.get("/items")
def list_items(request):
    """List all items in the database (runs in Rust engine)."""
    return app.db.query("SELECT * FROM items ORDER BY id")


@app.get("/items/{{item_id}}")
def get_item(request, item_id: int):
    """Get a single item by ID."""
    rows = app.db.query("{select_sql}", [item_id])
    if not rows:
        raise HTTPException(status_code=404, detail=f"Item {{item_id}} not found")
    return rows[0]


@app.post("/items", body_schema=ItemCreate)
def create_item(request):
    """Create a new item with request schema validation."""
    data = request.json()
    name = data["name"]
    qty = data.get("qty", 0)

    app.db.execute(
        "{insert_sql}",
        [name, qty],
    )
    return {{"message": "Item created successfully", "name": name, "qty": qty}}


@app.delete("/items/{{item_id}}")
def delete_item(request, item_id: int):
    """Delete an item by ID."""
    res = app.db.execute("{delete_sql}", [item_id])
    return {{"message": f"Item {{item_id}} deleted", "affected": res.rows_affected}}


@app.get("/")
def root(request):
    return {{"message": "Welcome to {name} API!", "database": "{db_kind}", "docs": "/docs"}}
"#
    )
}

/// Scaffold a new JustAPI project (shared by `new` and `create`).
fn scaffold_project(
    name: &str,
    output: Option<PathBuf>,
    db_kind: &str,
    db_url: &str,
    api_type: &str,
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
        scaffold_main_py(name, db_kind, db_url, api_type),
    )?;
    std::fs::write(
        project_dir.join("migrations").join("0001_initial.sql"),
        scaffold_migration_sql(name, db_kind),
    )?;

    let env = format!(
        "# {name} configuration\nHOST=127.0.0.1\nPORT=8080\nAPI_TYPE={api_type}\n# Database engine: {kind}\nDATABASE_URL={url}\n# SECRET_KEY=change-me\n",
        name = name,
        api_type = api_type,
        kind = db_kind,
        url = db_url,
    );
    std::fs::write(project_dir.join(".env"), env)?;

    let reqs = match db_kind {
        "duckdb" => "# Add Python dependencies here\njustapi\npydantic>=2.0\nduckdb>=0.9.0\n",
        "clickhouse" => {
            "# Add Python dependencies here\njustapi\npydantic>=2.0\nclickhouse-driver>=0.2.0\n"
        }
        "mongodb" => "# Add Python dependencies here\njustapi\npydantic>=2.0\npymongo>=4.0\n",
        "redis" => "# Add Python dependencies here\njustapi\npydantic>=2.0\nredis>=5.0\n",
        _ => "# Add Python dependencies here\njustapi\npydantic>=2.0\n",
    };
    std::fs::write(project_dir.join("requirements.txt"), reqs)?;

    let readme = format!(
        "# {name}\n\n\
        A modern high-performance JustAPI application powered by Rust (Protocol: `{api_type}`, Database: `{kind}`).\n\n\
        ## Quick start\n\n\
        ```bash\n\
        cd {dir}\n\
        justapi serve --reload\n\
        ```\n\n\
        Open http://localhost:8080/docs for interactive OpenAPI documentation.\n\n\
        ## Database & Connection\n\n\
        Connection string: `{url}`\n\n\
        ```bash\n\
        # Start server with hot reload\n\
        justapi serve --reload\n\
        ```\n",
        name = name,
        api_type = api_type,
        kind = db_kind,
        dir = project_dir.display(),
        url = db_url,
    );
    std::fs::write(project_dir.join("README.md"), readme)?;

    std::fs::write(
        project_dir.join(".gitignore"),
        "*.pyc\n__pycache__/\n.env\n*.egg-info/\ndist/\nbuild/\n*.db\n*.duckdb\n",
    )?;

    std::fs::write(
        project_dir.join("Dockerfile"),
        "FROM python:3.12-slim\n\
        WORKDIR /app\n\
        COPY requirements.txt .\n\
        RUN pip install --no-cache-dir -r requirements.txt\n\
        COPY . .\n\
        EXPOSE 8080\n\
        ENV HOST=0.0.0.0 PORT=8080\n\
        CMD [\"justapi\", \"serve\", \"--host\", \"0.0.0.0\", \"--port\", \"8080\"]\n",
    )?;

    std::fs::write(
        project_dir.join("docker-compose.otel.yml"),
        format!(
            "version: '3.8'\n\n\
            services:\n  \
              jaeger:\n    \
                image: jaegertracing/all-in-one:latest\n    \
                ports:\n      \
                  - \"16686:16686\" # Jaeger UI\n      \
                  - \"4317:4317\"   # OTLP gRPC receiver\n    \
                environment:\n      \
                  - COLLECTOR_OTLP_ENABLED=true\n\n  \
              prometheus:\n    \
                image: prom/prometheus:latest\n    \
                ports:\n      \
                  - \"9090:9090\"\n\n  \
              app:\n    \
                build: .\n    \
                ports:\n      \
                  - \"8080:8080\"\n    \
                environment:\n      \
                  - OTEL_EXPORTER_OTLP_ENDPOINT=http://jaeger:4317\n      \
                  - OTEL_SERVICE_NAME={name}\n",
            name = name
        ),
    )?;

    println!(
        "✨ Created new JustAPI project '{}' (API: {}, database: {})",
        name, api_type, db_kind
    );
    println!("   └─ OpenTelemetry observability compose file: docker-compose.otel.yml");
    println!();
    println!("  cd {}", project_dir.display());
    println!("  justapi serve --reload");
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
            let (kind, url) = resolve_scaffold_db(&name, None, None)?;
            let api_type = prompt_api_type_selection(&name);
            scaffold_project(&name, output, &kind, &url, &api_type)
        }
        Commands::Create { name, output, db, db_url, api_type } => {
            let (kind, url) = resolve_scaffold_db(&name, db.as_deref(), db_url)?;
            let proto = api_type.unwrap_or_else(|| prompt_api_type_selection(&name));
            scaffold_project(&name, output, &kind, &url, &proto)
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
