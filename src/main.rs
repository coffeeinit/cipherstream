use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;

mod transcoder;

#[tokio::main]
async fn main() {
    // Initialize FFmpeg natively from our local ported crate!
    ffmpeg_next::init().expect("Failed to initialize FFmpeg bindings");
    println!("FFmpeg bindings initialized successfully.");

    // Build our application with some basic routes
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/upload", post(upload_handler));

    // Run our app using modern Axum 0.7 and Tokio TcpListener
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await.unwrap();
    println!("CipherStream listening on http://{}", listener.local_addr().unwrap());
    
    axum::serve(listener, app).await.unwrap();
}

async fn root_handler() -> &'static str {
    "CipherStream Video Engine Running!"
}

async fn upload_handler() -> &'static str {
    // Here we will eventually handle TUS chunked uploads
    // and pass the file off to our FFmpeg transcoder queue.
    "Upload endpoint ready."
}
