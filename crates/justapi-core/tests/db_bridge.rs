//! Integration tests for the database bridge: type fidelity, blob binding,
//! robust transaction detection, streaming, isolation, and multi-engine URL
//! routing. SQLite runs in-memory (always available); Postgres/MySQL tests are
//! skipped unless a `JUSTAPI_TEST_PG_URL` / `JUSTAPI_TEST_MYSQL_URL` env var is
//! provided, so the suite stays hermetic by default.

use base64::Engine;
use justapi_core::db::{AnyPool, DatabaseConfig, DbKind, IsolationLevel, Param};

async fn sqlite_pool() -> AnyPool {
    AnyPool::connect(DatabaseConfig {
        url: "sqlite::memory:".into(),
        max_connections: 1,
        kind: Some(DbKind::Sqlite),
        ..Default::default()
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn sqlite_types_and_blobs() {
    let pool = sqlite_pool().await;
    pool.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, ok INTEGER, data BLOB, big REAL, nul TEXT)",
    )
    .await
    .unwrap();
    pool.execute_with(
        "INSERT INTO t (name, ok, data, big, nul) VALUES (?, ?, ?, ?, ?)",
        &[
            Param::Text("hi".into()),
            Param::Int(1),
            Param::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]),
            Param::Float(f64::NAN),
            Param::Null,
        ],
    )
    .await
    .unwrap();
    let rows = pool.query_with("SELECT * FROM t", &[]).await.unwrap();
    let row = rows.as_array().unwrap()[0].as_object().unwrap();
    assert_eq!(row["name"], serde_json::json!("hi"));
    assert_eq!(row["ok"], serde_json::json!(1));
    assert_eq!(row["nul"], serde_json::Value::Null);
    assert_eq!(
        row["data"],
        serde_json::json!({ "$bytes": base64::engine::general_purpose::STANDARD.encode([0xDE, 0xAD, 0xBE, 0xEF]) })
    );
    assert_eq!(row["big"], serde_json::Value::Null);
}

#[tokio::test]
async fn sqlite_transaction_read_and_write() {
    let pool = sqlite_pool().await;
    pool.execute("CREATE TABLE u (id INTEGER PRIMARY KEY, n INTEGER)").await.unwrap();
    let res = pool
        .transaction(&[
            ("INSERT INTO u (n) VALUES (?)".into(), vec![serde_json::json!(5)]),
            ("INSERT INTO u (n) VALUES (?)".into(), vec![serde_json::json!(7)]),
            ("SELECT SUM(n) AS total FROM u".into(), vec![]),
        ])
        .await
        .unwrap();
    assert_eq!(res, serde_json::json!([{ "total": 12 }]));
}

#[tokio::test]
async fn sqlite_transaction_isolation_serializable() {
    let pool = sqlite_pool().await;
    pool.execute("CREATE TABLE v (n INTEGER)").await.unwrap();
    let res = pool
        .transaction_with_isolation(
            &[("INSERT INTO v (n) VALUES (?)".into(), vec![Param::Int(1)])],
            Some(IsolationLevel::Serializable),
        )
        .await
        .unwrap();
    assert_eq!(res, serde_json::json!({ "rows_affected": 1 }));
}

#[tokio::test]
async fn sqlite_stream_chunks() {
    let pool = sqlite_pool().await;
    pool.execute("CREATE TABLE s (v INTEGER)").await.unwrap();
    for i in 0..7 {
        pool.execute_with("INSERT INTO s (v) VALUES (?)", &[Param::Int(i)]).await.unwrap();
    }
    let chunks = pool.query_stream("SELECT v FROM s ORDER BY v", &[], 3).await.unwrap();
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].as_array().unwrap().len(), 3);
    assert_eq!(chunks[2].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn sqlite_insert_returning() {
    let pool = sqlite_pool().await;
    pool.execute("CREATE TABLE r (id INTEGER PRIMARY KEY, name TEXT)").await.unwrap();
    let row = pool
        .insert_returning("r", &["name".into()], &serde_json::json!({ "name": "x" }))
        .await
        .unwrap();
    assert_eq!(row.as_array().unwrap()[0]["name"], serde_json::json!("x"));
}

#[tokio::test]
async fn db_kind_routing() {
    assert_eq!(DbKind::from_url("postgres://h/db"), DbKind::Postgres);
    assert_eq!(DbKind::from_url("postgresql://h/db"), DbKind::Postgres);
    assert_eq!(DbKind::from_url("sqlite::memory:"), DbKind::Sqlite);
    assert_eq!(DbKind::from_url("sqlite://./x"), DbKind::Sqlite);
    assert_eq!(DbKind::from_url("mysql://h/db"), DbKind::MySql);
    assert_eq!(DbKind::from_url("mariadb://h/db"), DbKind::MySql);
}

/// Postgres round-trip (skipped unless JUSTAPI_TEST_PG_URL is set).
#[tokio::test]
async fn pg_roundtrip_if_available() {
    let url = match std::env::var("JUSTAPI_TEST_PG_URL") {
        Ok(u) => u,
        Err(_) => return,
    };
    let pool = AnyPool::connect(DatabaseConfig {
        url,
        max_connections: 4,
        kind: Some(DbKind::Postgres),
        init_sql: Some(
            "DROP TABLE IF EXISTS pg_demo; CREATE TABLE pg_demo (id SERIAL PRIMARY KEY, name TEXT, qty INT)".into(),
        ),
        ..Default::default()
    })
    .await
    .unwrap();
    pool.execute_with(
        "INSERT INTO pg_demo (name, qty) VALUES (?, ?)",
        &[Param::Text("a".into()), Param::Int(3)],
    )
    .await
    .unwrap();
    let rows = pool.query_with("SELECT * FROM pg_demo ORDER BY id", &[]).await.unwrap();
    assert_eq!(rows.as_array().unwrap()[0]["name"], serde_json::json!("a"));
}

/// MySQL round-trip (skipped unless JUSTAPI_TEST_MYSQL_URL is set).
#[tokio::test]
async fn mysql_roundtrip_if_available() {
    let url = match std::env::var("JUSTAPI_TEST_MYSQL_URL") {
        Ok(u) => u,
        Err(_) => return,
    };
    let pool = AnyPool::connect(DatabaseConfig {
        url,
        max_connections: 4,
        kind: Some(DbKind::MySql),
        init_sql: Some(
            "DROP TABLE IF EXISTS my_demo; CREATE TABLE my_demo (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(64), qty INT)".into(),
        ),
        ..Default::default()
    })
    .await
    .unwrap();
    pool.execute_with(
        "INSERT INTO my_demo (name, qty) VALUES (?, ?)",
        &[Param::Text("a".into()), Param::Int(3)],
    )
    .await
    .unwrap();
    let rows = pool.query_with("SELECT * FROM my_demo ORDER BY id", &[]).await.unwrap();
    assert_eq!(rows.as_array().unwrap()[0]["name"], serde_json::json!("a"));
}
