use clap::Parser;
use squirreldb::client::{run_init, run_status, run_users, ClientArgs, Commands, Connection, Repl};
use squirreldb::types::ServerMessage;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
  let args = ClientArgs::parse();

  if let Some(cmd) = &args.subcommand {
    match cmd {
      Commands::Init { pg_url, sqlite } => {
        return run_init(pg_url.as_deref(), sqlite.as_deref()).await
      }
      Commands::Status => return run_status(&args.host).await,
      Commands::Listcollections { .. } => {
        let conn = Connection::connect(&args.host).await?;
        if let Ok(ServerMessage::Result { data, .. }) = conn.list_collections().await {
          println!("{}", serde_json::to_string_pretty(&data)?);
        }
        return Ok(());
      }
      Commands::Users { pg_url, action } => {
        return run_users(pg_url, action).await;
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
