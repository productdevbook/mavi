use std::{env, net::SocketAddr, sync::Arc};

use mavi_core::{MaviError, Result, SiteId};
use mavi_files::DirectoryFileStore;
use mavi_http::router;
use mavi_runtime::{FixedSiteResolver, Runtime};
use mavi_storage::Database;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    let database_url = required("DATABASE_URL")?;
    let site_id = SiteId::from_uuid(
        Uuid::parse_str(&required("MAVI_SITE_ID")?)
            .map_err(|_| MaviError::validation("invalid_site_id"))?,
    );
    let listen = env::var("LISTEN").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
    let connections = env::var("DATABASE_CONNECTIONS")
        .ok()
        .map(|value| value.parse::<u32>())
        .transpose()
        .map_err(|_| MaviError::validation("invalid_database_connections"))?
        .unwrap_or(10);

    let database = Database::connect(&database_url, connections).await?;
    database.migrate().await?;
    database.ensure_site(site_id).await?;

    let runtime = Runtime::new(database, FixedSiteResolver::new(site_id));
    let file_root = env::var("MAVI_FILES_DIR").unwrap_or_else(|_| "./mavi-files".to_owned());
    let file_store = Arc::new(DirectoryFileStore::at(file_root));
    let address: SocketAddr = listen
        .parse()
        .map_err(|_| MaviError::validation("invalid_listen_address"))?;
    let listener = TcpListener::bind(address)
        .await
        .map_err(|_| MaviError::Internal)?;

    tracing::info!(%address, %site_id, "mavi runtime listening");
    axum::serve(listener, router(runtime, file_store)?)
        .await
        .map_err(|_| MaviError::Internal)
}

fn required(name: &str) -> Result<String> {
    env::var(name).map_err(|_| MaviError::validation(format!("{name}_is_required")))
}
