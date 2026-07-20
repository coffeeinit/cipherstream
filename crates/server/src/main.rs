pub mod api;
pub mod config;
pub mod core;
pub mod db;
pub mod server;
pub mod transcoder;
pub mod worker;

#[tokio::main]
async fn main() -> Result<(), String> {
    server::run_server().await
}
