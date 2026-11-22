use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use axum::Router;
use homarr_api::{api_router, limits, AppState};
use homarr_common::SecretCrypto;
use homarr_core::{init_logging, AppConfig, CertificateManager, DatabaseDriver, HttpClientFactory};
use homarr_db::{
    seed::{seed_defaults, seed_demo},
    Database,
};
use homarr_redis::RedisStore;
use homarr_web::web_router;
use hyper::server::conn::http1;
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use tokio::net::TcpListener;
use tower::Service;
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
};

fn ensure_sqlite_parent_dir(driver: DatabaseDriver, url: &str) {
    if driver != DatabaseDriver::Sqlite {
        return;
    }
    let clean = url
        .trim()
        .strip_prefix("sqlite://")
        .or_else(|| url.trim().strip_prefix("sqlite:"))
        .unwrap_or(url.trim());
    if clean.is_empty() || clean == ":memory:" || clean.starts_with("mode=memory") {
        return;
    }
    if let Some(parent) = Path::new(clean).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                tracing::warn!(path = %parent.display(), error = %err, "Failed to create SQLite parent directory");
            }
        }
    }
}

async fn connect_redis_with_retry(config: &AppConfig) -> RedisStore {
    if config.redis_disabled {
        return RedisStore::in_memory();
    }
    // redis' ConnectionManager retries internally with backoff, so bound each
    // attempt; otherwise an unreachable Redis blocks startup indefinitely.
    const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(2);
    for attempt in 1..=5 {
        match tokio::time::timeout(ATTEMPT_TIMEOUT, RedisStore::connect(config))
            .await
            .unwrap_or_else(|_| {
                Err(homarr_common::HomarrError::Upstream(
                    "Redis connection timed out".to_string(),
                ))
            }) {
            Ok(store) => {
                if matches!(
                    tokio::time::timeout(ATTEMPT_TIMEOUT, store.health_check()).await,
                    Ok(Ok(_))
                ) {
                    return store;
                }
            }
            Err(err) => {
                tracing::warn!(
                    attempt = attempt,
                    error = %err,
                    "Redis connection attempt failed"
                );
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    tracing::warn!("Falling back to in-memory Redis store");
    RedisStore::in_memory()
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

/// Cuts connections that do not finish sending request headers in time (slowloris).
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(30);
/// How long in-flight connections get to finish after a shutdown signal.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(30);

/// Accept loop equivalent to `axum::serve` (HTTP/1, ConnectInfo, WebSocket upgrades, graceful
/// shutdown) plus hyper's header read timeout, which `axum::serve` does not expose.
async fn serve(listener: TcpListener, app: Router) {
    let mut make_service = app.into_make_service_with_connect_info::<SocketAddr>();
    let mut builder = http1::Builder::new();
    builder
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_READ_TIMEOUT);
    // hyper-util's GracefulShutdown does not support http1 upgradeable connections, so do what
    // `axum::serve` does: signal_tx fans out the shutdown, close_tx.closed() waits for all tasks.
    let (signal_tx, signal_rx) = tokio::sync::watch::channel(());
    let (close_tx, close_rx) = tokio::sync::watch::channel(());
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    loop {
        let (stream, remote_addr) = tokio::select! {
            res = listener.accept() => match res {
                Ok(conn) => conn,
                Err(err) => {
                    // Usually EMFILE/ENFILE: back off instead of spinning.
                    tracing::warn!(error = %err, "Failed to accept connection");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            },
            _ = &mut shutdown => break,
        };
        let tower_service = match make_service.call(remote_addr).await {
            Ok(svc) => svc,
            Err(never) => match never {},
        };
        let conn = builder
            .serve_connection(
                TokioIo::new(stream),
                TowerToHyperService::new(tower_service),
            )
            .with_upgrades();
        let mut signal_rx = signal_rx.clone();
        let close_rx = close_rx.clone();
        tokio::spawn(async move {
            let mut conn = std::pin::pin!(conn);
            let res = tokio::select! {
                res = conn.as_mut() => res,
                _ = signal_rx.changed() => {
                    conn.as_mut().graceful_shutdown();
                    conn.await
                }
            };
            if let Err(err) = res {
                tracing::debug!(error = %err, "Connection closed with error");
            }
            drop(close_rx);
        });
    }

    drop(listener);
    drop(signal_rx);
    drop(signal_tx);
    drop(close_rx);
    if tokio::time::timeout(SHUTDOWN_GRACE, close_tx.closed())
        .await
        .is_err()
    {
        tracing::warn!("Timed out waiting for connections to close");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::from_env()?;
    let logs = init_logging(&config.log_level);

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        host = %config.host,
        port = config.port,
        "Starting Homarr (Rust Edition)"
    );

    let crypto = SecretCrypto::new(&config.secret_encryption_key)?;
    let certs = CertificateManager::new(config.local_certificate_path.clone());
    let http = HttpClientFactory::new(&certs)?;

    ensure_sqlite_parent_dir(config.db_driver, &config.db_url);
    let db = Database::connect(config.db_driver, &config.db_url).await?;
    db.migrate().await?;
    // Trusted certificate hostnames (e.g. self-signed Proxmox on an IP) apply to outbound HTTP.
    let trusted = homarr_db::repos::settings::list_trusted_hostnames(&db).await?;
    let pins = trusted
        .iter()
        .map(|t| (t.hostname.as_str(), t.thumbprint.as_str()));
    http.set_trusted_hostnames(pins)?;

    if config.demo_mode {
        seed_demo(&db, &crypto).await?;
    } else {
        seed_defaults(&db).await?;
    }

    let redis = connect_redis_with_retry(&config).await;
    let state = AppState::build(config.clone(), db, redis, crypto, logs, certs, http);
    state.scheduler.start().await?;

    let assets = &config.assets_dir;
    let app: Router = api_router(state.clone())
        .merge(web_router(state.clone()))
        .nest_service("/static", ServeDir::new(assets.join("static")))
        .nest_service("/public", ServeDir::new(assets.join("public")))
        .nest_service("/images", ServeDir::new(assets.join("public/images")))
        .nest_service("/logo", ServeDir::new(assets.join("public/logo")))
        .route_service(
            "/favicon.ico",
            ServeFile::new(assets.join("public/favicon.ico")),
        )
        .route_service(
            "/logo.png",
            ServeFile::new(assets.join("public/logo/logo.png")),
        )
        .route_service("/sw.js", ServeFile::new(assets.join("public/sw.js")));
    // 2 MiB default body limit (per-route overrides live in the API router), response
    // timeouts and request body read timeout.
    let app = limits::apply(app).layer(CompressionLayer::new().gzip(true));

    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(listen_addr = %addr, "Homarr listening");

    serve(listener, app).await;

    state.scheduler.shutdown().await;
    tracing::info!("Homarr shutdown complete");
    Ok(())
}
