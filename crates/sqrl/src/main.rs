mod commands;
mod repl;

use clap::Parser;
use client::Connection;
use commands::{run_cache, run_status, ClientArgs, Commands};
use repl::Repl;
use types::ServerMessage;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
  let args = ClientArgs::parse();

  if let Some(cmd) = &args.subcommand {
    match cmd {
      Commands::Status => return run_status(&args.host).await,
      Commands::Listcollections { .. } => {
        let conn = Connection::connect(&args.host).await?;
        if let Ok(ServerMessage::Result { data, .. }) = conn.list_collections().await {
          println!("{}", serde_json::to_string_pretty(&data)?);
        }
        return Ok(());
      }
      Commands::Cache { host, action } => {
        return run_cache(host, action).await;
      }
    }
  }

  let conn = Connection::connect(&args.host).await?;

  if let Some(q) = &args.command {
    if let Ok(ServerMessage::Result { data, .. }) = conn.query(q).await {
      println!("{}", serde_json::to_string_pretty(&data)?);
    }
    return Ok(());
  }

  if let Some(file) = &args.file {
    for line in std::fs::read_to_string(file)?
      .lines()
      .filter(|l| !l.trim().is_empty() && !l.starts_with("//"))
    {
      if let Ok(ServerMessage::Result { data, .. }) = conn.query(line).await {
        println!("{}", serde_json::to_string_pretty(&data)?);
      }
    }
    return Ok(());
  }

  Repl::new(conn)?.run().await
}
