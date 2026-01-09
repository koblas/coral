use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

pub mod handler;
pub mod resp;
pub mod storage;

use handler::Handler;
use storage::Storage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let listener = TcpListener::bind("127.0.0.1:6379").await?;
    let storage = Arc::new(Storage::new());

    info!("Redis server listening on 127.0.0.1:6379");

    loop {
        let (socket, addr) = listener.accept().await?;
        let storage_clone = Arc::clone(&storage);

        tokio::spawn(async move {
            info!("New connection from {}", addr);
            if let Err(e) = handle_connection(socket, storage_clone).await {
                error!("Error handling connection: {}", e);
            }
        });
    }
}

async fn handle_connection(
    mut socket: TcpStream,
    storage: Arc<Storage>,
) -> Result<(), Box<dyn std::error::Error>> {
    let handler = Handler::new(storage);
    handler.handle_stream(&mut socket).await
}
