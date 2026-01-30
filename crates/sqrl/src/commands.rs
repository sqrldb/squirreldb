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
  /// Check server status
  Status,
  /// List collections
  Listcollections { db: Option<String> },
  /// Cache operations (connects to cache server via RESP protocol)
  Cache {
    /// Cache server host:port
    #[arg(short = 'H', long, default_value = "localhost:6379")]
    host: String,
    #[command(subcommand)]
    action: CacheAction,
  },
}

#[derive(Subcommand)]
pub enum CacheAction {
  /// Get a value by key
  Get {
    /// The cache key
    key: String,
  },
  /// Set a value with optional TTL
  Set {
    /// The cache key
    key: String,
    /// The value to store
    value: String,
    /// TTL in seconds (0 = no expiry)
    #[arg(short, long, default_value = "0")]
    ttl: u64,
  },
  /// Delete a key
  Del {
    /// The cache key
    key: String,
  },
  /// List keys matching a pattern
  Keys {
    /// Pattern to match (e.g., "user:*")
    #[arg(default_value = "*")]
    pattern: String,
  },
  /// Get cache statistics
  Info,
  /// Flush all keys
  Flush,
  /// Check cache server status
  Ping,
}

pub async fn run_cache(host: &str, action: &CacheAction) -> Result<(), anyhow::Error> {
  use client::resp::{parse_resp, RespValue};
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use tokio::net::TcpStream;

  // Connect to cache server
  let mut stream = TcpStream::connect(host).await.map_err(|e| {
    anyhow::anyhow!(
      "Failed to connect to cache server at {}: {}. Is the cache server running?",
      host,
      e
    )
  })?;

  // Build RESP command
  let cmd = match action {
    CacheAction::Ping => RespValue::array(vec![RespValue::bulk("PING")]),
    CacheAction::Get { key } => {
      RespValue::array(vec![RespValue::bulk("GET"), RespValue::bulk(key)])
    }
    CacheAction::Set { key, value, ttl } => {
      if *ttl > 0 {
        RespValue::array(vec![
          RespValue::bulk("SET"),
          RespValue::bulk(key),
          RespValue::bulk(value),
          RespValue::bulk("EX"),
          RespValue::bulk(&ttl.to_string()),
        ])
      } else {
        RespValue::array(vec![
          RespValue::bulk("SET"),
          RespValue::bulk(key),
          RespValue::bulk(value),
        ])
      }
    }
    CacheAction::Del { key } => {
      RespValue::array(vec![RespValue::bulk("DEL"), RespValue::bulk(key)])
    }
    CacheAction::Keys { pattern } => {
      RespValue::array(vec![RespValue::bulk("KEYS"), RespValue::bulk(pattern)])
    }
    CacheAction::Info => RespValue::array(vec![RespValue::bulk("INFO")]),
    CacheAction::Flush => RespValue::array(vec![RespValue::bulk("FLUSHDB")]),
  };

  // Send command
  stream.write_all(&cmd.encode()).await?;

  // Read response
  let mut buf = vec![0u8; 65536];
  let n = stream.read(&mut buf).await?;
  if n == 0 {
    return Err(anyhow::anyhow!("Connection closed by server"));
  }

  // Parse and display response
  let response = parse_resp(&buf[..n])?;
  print_resp_value(&response);

  Ok(())
}

fn print_resp_value(value: &client::resp::RespValue) {
  use client::resp::RespValue;

  match value {
    RespValue::SimpleString(s) => println!("{}", s),
    RespValue::Error(e) => eprintln!("(error) {}", e),
    RespValue::Integer(i) => println!("(integer) {}", i),
    RespValue::BulkString(Some(s)) => println!("\"{}\"", s),
    RespValue::BulkString(None) => println!("(nil)"),
    RespValue::Array(Some(arr)) => {
      if arr.is_empty() {
        println!("(empty array)");
      } else {
        for (i, item) in arr.iter().enumerate() {
          print!("{}) ", i + 1);
          print_resp_value_inline(item);
        }
      }
    }
    RespValue::Array(None) => println!("(nil)"),
  }
}

fn print_resp_value_inline(value: &client::resp::RespValue) {
  use client::resp::RespValue;

  match value {
    RespValue::SimpleString(s) => println!("{}", s),
    RespValue::Error(e) => println!("(error) {}", e),
    RespValue::Integer(i) => println!("{}", i),
    RespValue::BulkString(Some(s)) => println!("\"{}\"", s),
    RespValue::BulkString(None) => println!("(nil)"),
    RespValue::Array(Some(arr)) => {
      print!("[");
      for (i, item) in arr.iter().enumerate() {
        if i > 0 {
          print!(", ");
        }
        match item {
          RespValue::BulkString(Some(s)) => print!("\"{}\"", s),
          RespValue::Integer(n) => print!("{}", n),
          _ => print!("{:?}", item),
        }
      }
      println!("]");
    }
    RespValue::Array(None) => println!("(nil)"),
  }
}

pub async fn run_status(host: &str) -> Result<(), anyhow::Error> {
  let conn = client::Connection::connect(host).await?;
  conn.ping().await?;
  println!("Server running at {}", host);
  Ok(())
}
