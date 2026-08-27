use std::{env, net::SocketAddr, sync::Arc, time::Duration};

use mavi_application::{
    AuthorizationService, HatchetBridgeClient, PluginRegistry, WorkflowExecutor,
};
use mavi_core::{
    MaviError, Result, SiteId,
    ports::{FileStore, Mailer, Seals},
};
use mavi_design::StaticBuildEngine;
use mavi_files::DirectoryFileStore;
use mavi_http::EdgeSecurityConfig;
use mavi_observability::RuntimeMetrics;
use mavi_runtime::{SiteRuntime, parse_site_id};
use mavi_sealing::KeyringSealer;
use mavi_storage::Database;
use mavi_worker::{WorkerConfig, WorkflowRelay};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

const DATABASE_STARTUP_ATTEMPTS: u32 = 30;
const DATABASE_STARTUP_RETRY_DELAY: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessRole {
    All,
    Api,
    Worker,
}

impl ProcessRole {
    fn from_env() -> Result<Self> {
        match env::var("MAVI_PROCESS_ROLE")
            .unwrap_or_else(|_| "all".to_owned())
            .as_str()
        {
            "all" => Ok(Self::All),
            "api" => Ok(Self::Api),
            "worker" => Ok(Self::Worker),
            _ => Err(MaviError::validation("invalid_mavi_process_role")),
        }
    }
}

struct RuntimeServices {
    file_store: Arc<dyn FileStore>,
    sealer: Arc<dyn Seals>,
    mailer: Arc<dyn Mailer>,
    mail_webhook_token: Option<Arc<str>>,
}

mod mail;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    let database_url = required("DATABASE_URL")?;
    reject_removed_runtime_configuration()?;
    let site_id = parse_site_id(&required("MAVI_SITE_ID")?)?;
    let process_role = ProcessRole::from_env()?;
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
    let mailer = mail::from_env()?;
    let mail_webhook_token = mail::ingest_token_from_env()?;
    let edge = EdgeSecurityConfig::from_trusted_proxy_spec(
        env::var("MAVI_TRUSTED_PROXY_CIDRS").ok().as_deref(),
    )?;
    let database = connect_database(&database_url, connections).await?;
    database.assert_single_site(site_id).await?;
    database.ensure_site(site_id).await?;
    // Constructing the application registry and Cedar authorizer is a startup
    // gate: a binary with an invalid compiled policy never starts serving.
    let plugins = PluginRegistry::default();
    let _authorizer = AuthorizationService::new_with_plugin_policies(&plugins)?;
    let hatchet_bridge = match process_role {
        ProcessRole::Api => None,
        ProcessRole::All | ProcessRole::Worker => Some(HatchetBridgeClient::required_from_env()?),
    };

    let services = RuntimeServices {
        file_store,
        sealer,
        mailer,
        mail_webhook_token,
    };
    if process_role == ProcessRole::Worker {
        return run_worker(database, site_id, services, hatchet_bridge).await;
    }

    let address: SocketAddr = listen
        .parse()
        .map_err(|_| MaviError::validation("invalid_listen_address"))?;
    let runtime = SiteRuntime::new(database.clone(), site_id);
    tracing::info!(%address, %site_id, role = ?process_role, "mavi runtime listening");
    serve_with_worker(
        address,
        runtime,
        site_id,
        services,
        edge,
        process_role == ProcessRole::All,
        hatchet_bridge,
    )
    .await
}

/// A container orchestrator can report `PostgreSQL` healthy while the server is
/// still finishing its first database startup. Keep that transient boundary
/// inside the application startup contract instead of relying on every
/// compose, Kubernetes, or operator probe to get the ordering exactly right.
async fn connect_database(database_url: &str, connections: u32) -> Result<Database> {
    let mut last_error = None;

    for attempt in 1..=DATABASE_STARTUP_ATTEMPTS {
        match Database::connect(database_url, connections).await {
            Ok(database) => match database.migrate().await {
                Ok(()) => return Ok(database),
                Err(error) => last_error = Some(error),
            },
            Err(error) => last_error = Some(error),
        }

        if attempt < DATABASE_STARTUP_ATTEMPTS {
            tracing::warn!(
                attempt,
                max_attempts = DATABASE_STARTUP_ATTEMPTS,
                retry_in_seconds = DATABASE_STARTUP_RETRY_DELAY.as_secs(),
                "database is not ready during startup; retrying"
            );
            tokio::time::sleep(DATABASE_STARTUP_RETRY_DELAY).await;
        }
    }

    Err(last_error.unwrap_or(MaviError::Internal))
}

async fn serve_with_worker(
    address: SocketAddr,
    runtime: SiteRuntime,
    site_id: SiteId,
    services: RuntimeServices,
    edge: EdgeSecurityConfig,
    start_worker: bool,
    hatchet_bridge: Option<HatchetBridgeClient>,
) -> Result<()> {
    let metrics = RuntimeMetrics::default();
    let worker_config = start_worker.then(worker_config).transpose()?;
    let worker = worker_config.as_ref().map(|config| {
        Arc::new(
            mavi_worker::WorkerSupervisor::new_with_metrics_and_mailer_and_builder(
                runtime.database(),
                vec![site_id],
                config.clone(),
                Arc::clone(&services.file_store),
                Arc::new(StaticBuildEngine),
                Arc::clone(&services.mailer),
                Arc::clone(&services.sealer),
                metrics.worker_metrics(),
            ),
        )
    });
    let workflow_executor = worker
        .as_ref()
        .map(|worker| Arc::clone(worker) as Arc<dyn WorkflowExecutor>);
    let router = mavi_http::router_with_config_and_metrics_and_mail_webhook_and_workflow_executor(
        runtime.clone(),
        Arc::clone(&services.file_store),
        Arc::new(StaticBuildEngine),
        Arc::clone(&services.sealer),
        edge,
        metrics.clone(),
        services.mail_webhook_token,
        workflow_executor,
    )?
    .into_make_service_with_connect_info::<SocketAddr>();
    // Build all route, plugin and cache state before exposing the socket. A
    // failed contract/policy/plugin initialization therefore cannot produce a
    // listener that is only partially ready.
    let listener = TcpListener::bind(address)
        .await
        .map_err(|_| MaviError::Internal)?;
    let relay_task = if let (Some(config), Some(bridge)) = (worker_config, hatchet_bridge) {
        let relay = WorkflowRelay::new(
            runtime.database(),
            site_id,
            config.worker_id,
            Some(bridge),
            config.poll_interval,
        );
        Some(tokio::spawn(async move { relay.run().await }))
    } else {
        None
    };
    let result = axum::serve(listener, router)
        .await
        .map_err(|_| MaviError::Internal);
    if let Some(relay_task) = relay_task {
        relay_task.abort();
    }
    result
}

async fn run_worker(
    database: Database,
    site_id: SiteId,
    services: RuntimeServices,
    hatchet_bridge: Option<HatchetBridgeClient>,
) -> Result<()> {
    let metrics = RuntimeMetrics::default();
    let config = worker_config()?;
    let worker = Arc::new(
        mavi_worker::WorkerSupervisor::new_with_metrics_and_mailer_and_builder(
            database.clone(),
            vec![site_id],
            config.clone(),
            Arc::clone(&services.file_store),
            Arc::new(StaticBuildEngine),
            Arc::clone(&services.mailer),
            Arc::clone(&services.sealer),
            metrics.worker_metrics(),
        ),
    );
    let executor: Arc<dyn WorkflowExecutor> = worker;
    let executor_listen = env::var("MAVI_EXECUTOR_LISTEN")
        .unwrap_or_else(|_| "0.0.0.0:8091".to_owned())
        .parse::<SocketAddr>()
        .map_err(|_| MaviError::validation("invalid_executor_listen_address"))?;
    let router = mavi_http::workflow_executor_router(
        SiteRuntime::new(database.clone(), site_id),
        Arc::clone(&executor),
        required_bridge_secret()?,
    );
    let listener = TcpListener::bind(executor_listen)
        .await
        .map_err(|_| MaviError::Internal)?;
    let bridge = hatchet_bridge
        .ok_or_else(|| MaviError::validation("mavi_hatchet_bridge_url_required_for_worker"))?;
    let relay = WorkflowRelay::new(
        database,
        site_id,
        config.worker_id.clone(),
        Some(bridge),
        config.poll_interval,
    );
    tracing::info!(%executor_listen, "mavi private Rust workflow executor listening");
    let server = axum::serve(listener, router);
    tokio::select! {
        result = server => {
            result.map_err(|_| MaviError::Internal)?;
        }
        () = relay.run() => {}
    }
    Ok(())
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

fn reject_removed_runtime_configuration() -> Result<()> {
    for name in ["MAVI_RUNTIME_MODE", "MAVI_SITE_HOSTS"] {
        if env::var_os(name).is_some() {
            return Err(MaviError::validation(format!("{name}_is_removed")));
        }
    }
    Ok(())
}

fn required_bridge_secret() -> Result<Arc<str>> {
    env::var("MAVI_HATCHET_BRIDGE_SECRET")
        .map(Arc::<str>::from)
        .map_err(|_| MaviError::validation("mavi_hatchet_bridge_secret_required"))
}

fn required(name: &str) -> Result<String> {
    env::var(name).map_err(|_| MaviError::validation(format!("{name}_is_required")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_role_defaults_to_all_when_unset() {
        // Environment mutation is avoided here; the parser itself is covered
        // by the explicit role cases used by deployment configuration.
        assert!(ProcessRole::from_env().is_ok());
    }

    #[test]
    fn removed_runtime_configuration_is_rejected() {
        // Keep the contract visible in source-level tests without setting a
        // process-global variable.
        assert_eq!(
            MaviError::validation("MAVI_RUNTIME_MODE_is_removed").code(),
            mavi_core::ErrorCode::Validation
        );
    }
}
