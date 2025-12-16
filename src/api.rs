use crate::statistics::Statistics;
use anyhow::Result;
use axum::{routing::get, Json, Router};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;
use tracing::info;

pub async fn start_api_server(stats: Arc<RwLock<Statistics>>, port: u16) -> Result<()> {
    let app = Router::new().route("/stats", get(move || get_stats(stats)));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("API server listening on {}", addr);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn get_stats(stats: Arc<RwLock<Statistics>>) -> Json<Statistics> {
    let data = {
        let s = stats.read().unwrap();
        s.clone()
    };
    Json(data)
}
