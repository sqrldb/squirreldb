use clap::Parser;
use squirreldb::db::{DatabaseBackend, PostgresBackend, SqliteBackend};
use squirreldb::server::{BackendType, Daemon, ServerConfig};
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser)]
#[command(name = "sqrld", about = "SquirrelDB server", version)]
struct Args {
  #[arg(long, env = "SQUIRRELDB_PG_URL")]
  pg_url: Option<String>,
  #[arg(long, env = "SQUIRRELDB_SQLITE_PATH")]
  sqlite: Option<String>,
  #[arg(short, long)]
  port: Option<u16>,
  #[arg(long)]
  host: Option<String>,
  #[arg(short, long)]
  config: Option<String>,
  #[arg(long)]
  log_level: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
  let args = Args::parse();

  // Load config: explicit path > auto-detect > defaults
  let mut config = if let Some(path) = &args.config {
    ServerConfig::from_file(path)?
  } else {
    ServerConfig::find_and_load()?.unwrap_or_default()
  };

  // CLI args override config file
  if let Some(url) = args.pg_url {
    config.postgres.url = url;
    config.backend = BackendType::Postgres;
  }
  if let Some(path) = args.sqlite {
    config.sqlite.path = path;
    config.backend = BackendType::Sqlite;
  }
  if let Some(port) = args.port {
    config.server.ports.http = port;
  }
  if let Some(host) = args.host {
    config.server.host = host;
  }
  if let Some(level) = args.log_level {
    config.logging.level = level;
  }

  tracing_subscriber::registry()
    .with(
      tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| config.logging.level.clone().into()),
    )
    .with(tracing_subscriber::fmt::layer())
    .init();

  let backend: Arc<dyn DatabaseBackend> = match config.backend {
    BackendType::Postgres => Arc::new(PostgresBackend::new(
      &config.postgres.url,
      config.postgres.max_connections,
    )?),
    BackendType::Sqlite => Arc::new(SqliteBackend::new(&config.sqlite.path).await?),
  };

  let daemon = Arc::new(Daemon::new(config, backend));
  let daemon_clone = daemon.clone();

  // Handle shutdown signals (SIGINT, SIGTERM)
  tokio::spawn(async move {
    shutdown_signal().await;
    daemon_clone.shutdown();

    // Give servers time to drain connections
    tokio::time::sleep(Duration::from_secs(5)).await;
    tracing::info!("Shutdown complete");
    std::process::exit(0);
  });

  daemon.run().await
}

async fn shutdown_signal() {
  let ctrl_c = async {
    tokio::signal::ctrl_c()
      .await
      .expect("Failed to install Ctrl+C handler");
  };

  #[cfg(unix)]
  let terminate = async {
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
      .expect("Failed to install SIGTERM handler")
      .recv()
      .await;
  };

  #[cfg(not(unix))]
  let terminate = std::future::pending::<()>();

  tokio::select! {
    _ = ctrl_c => tracing::info!("Received SIGINT"),
    _ = terminate => tracing::info!("Received SIGTERM"),
  }
}
