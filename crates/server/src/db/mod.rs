pub mod schema;
pub mod queries;

pub use sqlx::PgPool;

pub async fn init_pool(url: &str, max_connections: u32) -> Result<PgPool, String> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(url)
        .await
        .map_err(|e| format!("Failed to connect to PostgreSQL: {e}"))?;

    schema::apply(&pool).await?;
    Ok(pool)
}
