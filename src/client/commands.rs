use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sqrl", about = "SquirrelDB client", version)]
pub struct ClientArgs {
  #[arg(short = 'H', long, default_value = "localhost:8080")]
  pub host: String,
  #[arg(short, long)]
  pub command: Option<String>,
  #[arg(short, long)]
  pub file: Option<String>,
  #[arg(long, default_value = "json")]
  pub format: OutputFormat,
  #[command(subcommand)]
  pub subcommand: Option<Commands>,
}

#[derive(Clone, Copy, Default, clap::ValueEnum)]
pub enum OutputFormat {
  #[default]
  Json,
  Table,
  Csv,
}

#[derive(Subcommand)]
pub enum Commands {
  /// Initialize the database schema
  Init {
    #[arg(long)]
    pg_url: Option<String>,
    #[arg(long)]
    sqlite: Option<String>,
  },
  /// Check server status
  Status,
  /// List collections
  Listcollections { db: Option<String> },
  /// Manage database users (PostgreSQL only)
  Users {
    #[arg(long, env = "DATABASE_URL")]
    pg_url: String,
    #[command(subcommand)]
    action: UsersAction,
  },
  /// Start MCP stdio server for Claude Desktop integration
  Mcp {
    #[arg(long)]
    pg_url: Option<String>,
    #[arg(long)]
    sqlite: Option<String>,
  },
}

#[derive(Subcommand)]
pub enum UsersAction {
  /// List all database users
  List,
  /// Add a new database user
  Add {
    /// Username for the new user
    username: String,
    /// Password for the new user
    #[arg(short, long)]
    password: Option<String>,
    /// Grant superuser privileges
    #[arg(long)]
    superuser: bool,
    /// Allow user to create databases
    #[arg(long)]
    createdb: bool,
  },
  /// Remove a database user
  Remove {
    /// Username to remove
    username: String,
  },
  /// Change a user's password
  Passwd {
    /// Username
    username: String,
    /// New password
    #[arg(short, long)]
    password: Option<String>,
  },
}

pub async fn run_init(pg_url: Option<&str>, sqlite: Option<&str>) -> Result<(), anyhow::Error> {
  use crate::db::{DatabaseBackend, PostgresBackend, SqliteBackend};

  let backend: Box<dyn DatabaseBackend> = if let Some(path) = sqlite {
    Box::new(SqliteBackend::new(path).await?)
  } else if let Some(url) = pg_url {
    Box::new(PostgresBackend::new(url, 5)?)
  } else {
    return Err(anyhow::anyhow!("Either --pg-url or --sqlite is required"));
  };

  backend.init_schema().await?;
  println!("Schema initialized");
  Ok(())
}

pub async fn run_mcp(pg_url: Option<&str>, sqlite: Option<&str>) -> Result<(), anyhow::Error> {
  use std::sync::Arc;

  use crate::db::{DatabaseBackend, PostgresBackend, SqliteBackend};
  use crate::mcp::McpServer;
  use crate::query::QueryEnginePool;
  use crate::server::ServerConfig;

  // Load config or use defaults
  let config = ServerConfig::find_and_load()?.unwrap_or_default();

  // Create backend from args or config
  let backend: Arc<dyn DatabaseBackend> = if let Some(path) = sqlite {
    Arc::new(SqliteBackend::new(path).await?)
  } else if let Some(url) = pg_url {
    Arc::new(PostgresBackend::new(url, config.postgres.max_connections)?)
  } else {
    // Fall back to config
    match config.backend {
      crate::server::BackendType::Sqlite => {
        Arc::new(SqliteBackend::new(&config.sqlite.path).await?)
      }
      crate::server::BackendType::Postgres => Arc::new(PostgresBackend::new(
        &config.postgres.url,
        config.postgres.max_connections,
      )?),
    }
  };

  // Initialize schema
  backend.init_schema().await?;

  // Create engine pool
  let pool_size = std::thread::available_parallelism()
    .map(|n| n.get())
    .unwrap_or(4);
  let engine_pool = Arc::new(QueryEnginePool::new(pool_size, backend.dialect()));

  // Run MCP server over stdio
  McpServer::run_stdio(backend, engine_pool).await
}

pub async fn run_status(host: &str) -> Result<(), anyhow::Error> {
  use super::Connection;
  let conn = Connection::connect(host).await?;
  conn.ping().await?;
  println!("Server running at {}", host);
  Ok(())
}

pub async fn run_users(pg_url: &str, action: &UsersAction) -> Result<(), anyhow::Error> {
  use tokio_postgres::NoTls;

  let (client, connection) = tokio_postgres::connect(pg_url, NoTls).await?;

  // Spawn connection handler
  tokio::spawn(async move {
    if let Err(e) = connection.await {
      eprintln!("Connection error: {}", e);
    }
  });

  match action {
    UsersAction::List => {
      let rows = client
        .query(
          "SELECT usename, usesuperuser, usecreatedb, usecanlogin
           FROM pg_user
           ORDER BY usename",
          &[],
        )
        .await?;

      println!(
        "{:<20} {:>10} {:>10} {:>10}",
        "USERNAME", "SUPERUSER", "CREATEDB", "LOGIN"
      );
      println!("{}", "-".repeat(54));
      for row in rows {
        let username: &str = row.get(0);
        let superuser: bool = row.get(1);
        let createdb: bool = row.get(2);
        let canlogin: bool = row.get(3);
        println!(
          "{:<20} {:>10} {:>10} {:>10}",
          username,
          if superuser { "yes" } else { "no" },
          if createdb { "yes" } else { "no" },
          if canlogin { "yes" } else { "no" }
        );
      }
    }

    UsersAction::Add {
      username,
      password,
      superuser,
      createdb,
    } => {
      // Validate username (alphanumeric and underscores only)
      if !username.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(anyhow::anyhow!(
          "Username must contain only alphanumeric characters and underscores"
        ));
      }

      let pwd = match password {
        Some(p) => p.clone(),
        None => prompt_password("Password: ")?,
      };

      let mut sql = format!("CREATE USER {} WITH PASSWORD ", quote_ident(username));
      sql.push_str(&quote_literal(&pwd));

      if *superuser {
        sql.push_str(" SUPERUSER");
      }
      if *createdb {
        sql.push_str(" CREATEDB");
      }

      client.execute(&sql, &[]).await?;
      println!("User '{}' created successfully", username);
    }

    UsersAction::Remove { username } => {
      // Validate username
      if !username.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(anyhow::anyhow!("Invalid username"));
      }

      let sql = format!("DROP USER IF EXISTS {}", quote_ident(username));
      client.execute(&sql, &[]).await?;
      println!("User '{}' removed", username);
    }

    UsersAction::Passwd { username, password } => {
      // Validate username
      if !username.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(anyhow::anyhow!("Invalid username"));
      }

      let pwd = match password {
        Some(p) => p.clone(),
        None => prompt_password("New password: ")?,
      };

      let sql = format!(
        "ALTER USER {} WITH PASSWORD {}",
        quote_ident(username),
        quote_literal(&pwd)
      );
      client.execute(&sql, &[]).await?;
      println!("Password updated for user '{}'", username);
    }
  }

  Ok(())
}

/// Quote a PostgreSQL identifier (table name, column name, etc.)
fn quote_ident(s: &str) -> String {
  format!("\"{}\"", s.replace('"', "\"\""))
}

/// Quote a PostgreSQL string literal
fn quote_literal(s: &str) -> String {
  format!("'{}'", s.replace('\'', "''"))
}

/// Prompt for password input (hidden)
fn prompt_password(prompt: &str) -> Result<String, anyhow::Error> {
  use std::io::{self, Write};

  print!("{}", prompt);
  io::stdout().flush()?;

  // Try to read password without echo
  #[cfg(unix)]
  {
    use std::os::unix::io::AsRawFd;
    let stdin_fd = io::stdin().as_raw_fd();

    // Get current terminal settings
    let mut termios = unsafe {
      let mut t = std::mem::zeroed();
      if libc::tcgetattr(stdin_fd, &mut t) != 0 {
        // Fall back to regular input if we can't get terminal settings
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        return Ok(input.trim().to_string());
      }
      t
    };

    // Disable echo
    let old_termios = termios;
    termios.c_lflag &= !libc::ECHO;
    unsafe {
      libc::tcsetattr(stdin_fd, libc::TCSANOW, &termios);
    }

    // Read password
    let mut input = String::new();
    let result = io::stdin().read_line(&mut input);

    // Restore terminal settings
    unsafe {
      libc::tcsetattr(stdin_fd, libc::TCSANOW, &old_termios);
    }
    println!(); // Print newline after hidden input

    result?;
    Ok(input.trim().to_string())
  }

  #[cfg(not(unix))]
  {
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
  }
}
