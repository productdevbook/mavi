use std::{env, net::SocketAddr, sync::Arc, time::Duration};

use mavi_core::{MaviError, Result, SiteId, ports::FileStore};
use mavi_design::StaticBuildEngine;
use mavi_files::DirectoryFileStore;
use mavi_http::EdgeSecurityConfig;
use mavi_observability::RuntimeMetrics;
use mavi_runtime::{
    FixedSiteResolver, HostSiteResolver, Runtime, RuntimeMode, SiteResolver, parse_site_id,
};
use mavi_sealing::KeyringSealer;
use mavi_storage::{Database, SiteStatus};
use mavi_worker::WorkerConfig;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

enum StartupConfig {
    FixedSite(SiteId),
    Shard(Vec<(String, SiteId)>),
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    let database_url = required("DATABASE_URL")?;
    let startup_config = startup_config()?;
    let listen = env::var("LISTEN").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
    let connections = env::var("DATABASE_CONNECTIONS")
        .ok()
        .map(|value| value.parse::<u32>())
        .transpose()
        .map_err(|_| MaviError::validation("invalid_database_connections"))?
        .unwrap_or(10);
    let file_root = env::var("MAVI_FILES_DIR").unwrap_or_else(|_| "./mavi-files".to_owned());
    let file_store: Arc<dyn FileStore> = Arc::new(DirectoryFileStore::at(file_root));
    let sealer = Arc::new(KeyringSealer::from_spec(&required("MAVI_KEYS")?)?);
    let edge = EdgeSecurityConfig::from_trusted_proxy_spec(
        env::var("MAVI_TRUSTED_PROXY_CIDRS").ok().as_deref(),
    )?;
    let address: SocketAddr = listen
        .parse()
        .map_err(|_| MaviError::validation("invalid_listen_address"))?;

    let database = Database::connect(&database_url, connections).await?;
    database.migrate().await?;
    let listener = TcpListener::bind(address)
        .await
        .map_err(|_| MaviError::Internal)?;

    match startup_config {
        StartupConfig::FixedSite(site_id) => {
            database.ensure_site(site_id).await?;
            tracing::info!(%address, %site_id, mode = "fixed_site", "mavi runtime listening");
            serve(
                listener,
                database,
                FixedSiteResolver::new(site_id),
                vec![site_id],
                file_store,
                sealer,
                edge,
            )
            .await
        }
        StartupConfig::Shard(entries) => {
            let resolver = HostSiteResolver::new(entries.clone())?;
            let sites = entries
                .iter()
                .map(|(_, site_id)| (*site_id, SiteStatus::Active));
            database.reconcile_sites(sites).await?;
            tracing::info!(%address, mode = "shard", "mavi runtime listening");
            let site_ids = entries.into_iter().map(|(_, site_id)| site_id).collect();
            serve(
                listener, database, resolver, site_ids, file_store, sealer, edge,
            )
            .await
        }
    }
}

async fn serve<R>(
    listener: TcpListener,
    database: Database,
    resolver: R,
    sites: Vec<SiteId>,
    file_store: Arc<dyn FileStore>,
    sealer: Arc<KeyringSealer>,
    edge: EdgeSecurityConfig,
) -> Result<()>
where
    R: SiteResolver,
{
    let worker_database = database.clone();
    let runtime = Runtime::new(database, resolver);
    let metrics = RuntimeMetrics::default();
    let router = mavi_http::router_with_config_and_metrics(
        runtime,
        Arc::clone(&file_store),
        Arc::new(StaticBuildEngine),
        sealer,
        edge,
        metrics.clone(),
    )?
    .into_make_service_with_connect_info::<SocketAddr>();
    let worker = mavi_worker::WorkerSupervisor::new_with_metrics(
        worker_database,
        sites,
        worker_config()?,
        file_store,
        metrics.worker_metrics(),
    );
    let worker_task = tokio::spawn(async move { worker.run().await });
    let result = axum::serve(listener, router)
        .await
        .map_err(|_| MaviError::Internal);
    worker_task.abort();
    result
}

fn worker_config() -> Result<WorkerConfig> {
    let defaults = WorkerConfig::default();
    let default_poll_millis = u64::try_from(defaults.poll_interval.as_millis()).unwrap_or(u64::MAX);
    let worker_id = env::var("MAVI_WORKER_ID").unwrap_or(defaults.worker_id);
    let lease_seconds = env::var("MAVI_WORKER_LEASE_SECONDS")
        .ok()
        .map(|value| {
            value
                .parse::<i64>()
                .map_err(|_| MaviError::validation("invalid_worker_lease_seconds"))
        })
        .transpose()?
        .unwrap_or(defaults.lease_seconds);
    let poll_millis = env::var("MAVI_WORKER_POLL_MILLIS")
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| MaviError::validation("invalid_worker_poll_millis"))
        })
        .transpose()?
        .unwrap_or(default_poll_millis);
    WorkerConfig::new(worker_id, lease_seconds, Duration::from_millis(poll_millis))
}

fn runtime_mode() -> Result<RuntimeMode> {
    parse_runtime_mode(env::var("MAVI_RUNTIME_MODE").ok().as_deref())
}

fn startup_config() -> Result<StartupConfig> {
    match runtime_mode()? {
        RuntimeMode::FixedSite => Ok(StartupConfig::FixedSite(parse_site_id(&required(
            "MAVI_SITE_ID",
        )?)?)),
        RuntimeMode::Shard => Ok(StartupConfig::Shard(parse_site_hosts(&required(
            "MAVI_SITE_HOSTS",
        )?)?)),
    }
}

fn parse_runtime_mode(value: Option<&str>) -> Result<RuntimeMode> {
    match value.unwrap_or("fixed_site") {
        "fixed_site" => Ok(RuntimeMode::FixedSite),
        "shard" => Ok(RuntimeMode::Shard),
        _ => Err(MaviError::validation("invalid_runtime_mode")),
    }
}

fn parse_site_hosts(value: &str) -> Result<Vec<(String, SiteId)>> {
    if value.trim().is_empty() {
        return Err(MaviError::validation("site_hosts_required"));
    }

    value
        .split(',')
        .map(|entry| {
            let (host, site_id) = entry
                .trim()
                .split_once('=')
                .ok_or_else(|| MaviError::validation("invalid_site_host_entry"))?;
            let host = host.trim();
            if host.is_empty() {
                return Err(MaviError::validation("site_host_required"));
            }
            Ok((host.to_owned(), parse_site_id(site_id.trim())?))
        })
        .collect()
}

fn required(name: &str) -> Result<String> {
    env::var(name).map_err(|_| MaviError::validation(format!("{name}_is_required")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_site_is_the_default_runtime_mode() {
        assert_eq!(
            parse_runtime_mode(None).expect("default mode"),
            RuntimeMode::FixedSite
        );
    }

    #[test]
    fn shard_site_hosts_parse_into_typed_entries() {
        let first = SiteId::new();
        let second = SiteId::new();
        let entries = parse_site_hosts(&format!(
            "first.example.com={first}, second.example.com={second}"
        ))
        .expect("site host entries");

        assert_eq!(
            entries,
            vec![
                ("first.example.com".to_owned(), first),
                ("second.example.com".to_owned(), second)
            ]
        );
    }

    #[test]
    fn shard_site_hosts_reject_malformed_entries() {
        assert!(parse_site_hosts("").is_err());
        assert!(parse_site_hosts("example.com").is_err());
        assert!(parse_site_hosts("example.com=not-a-uuid").is_err());
    }
}
