pub mod schema;
pub mod queries;

pub use sqlx::SqlitePool;

pub async fn init_pool(path: &std::path::Path, max_connections: u32) -> Result<SqlitePool, String> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create database directory: {e}"))?;
    }
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await
        .map_err(|e| format!("Failed to connect to SQLite: {e}"))?;
    schema::apply(&pool).await?;
    Ok(pool)
}
