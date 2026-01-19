use colored::Colorize;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use super::Connection;
use crate::types::ServerMessage;

pub struct Repl {
  conn: Connection,
  editor: DefaultEditor,
}

impl Repl {
  pub fn new(conn: Connection) -> Result<Self, anyhow::Error> {
    Ok(Self {
      conn,
      editor: DefaultEditor::new()?,
    })
  }

  pub async fn run(&mut self) -> Result<(), anyhow::Error> {
    println!(
      "{} v{}",
      "SquirrelDB 🐿️".green().bold(),
      env!("CARGO_PKG_VERSION")
    );
    println!("Type {} for help\n", ".help".cyan());

    loop {
      match self.editor.readline(&format!("{} ", "squirrel>".green())) {
        Ok(line) => {
          let line = line.trim();
          if line.is_empty() {
            continue;
          }
          let _ = self.editor.add_history_entry(line);
          if line.starts_with('.') {
            if !self.command(line).await {
              break;
            }
          } else {
            self.query(line).await;
          }
        }
        Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
        Err(e) => {
          eprintln!("Error: {:?}", e);
          break;
        }
      }
    }
    Ok(())
  }

  async fn command(&self, cmd: &str) -> bool {
    match cmd.split_whitespace().next().unwrap_or("") {
      ".help" => println!("Commands: .help, .tables, .clear, .quit"),
      ".tables" => {
        if let Ok(ServerMessage::Result { data, .. }) = self.conn.list_collections().await {
          println!("{}", serde_json::to_string_pretty(&data).unwrap());
        }
      }
      ".clear" => print!("\x1B[2J\x1B[1;1H"),
      ".quit" | ".exit" => return false,
      _ => eprintln!("Unknown command"),
    }
    true
  }

  async fn query(&self, q: &str) {
    if q.contains(".changes(") {
      if let Ok(ServerMessage::Subscribed { .. }) = self.conn.subscribe(q).await {
        println!("{}", "Listening... (Ctrl+C to stop)".yellow());
        loop {
          tokio::select! {
            Some(ServerMessage::Change { change, .. }) = self.conn.recv_change() => {
              println!("{}", serde_json::to_string_pretty(&change).unwrap());
            }
            _ = tokio::signal::ctrl_c() => break,
          }
        }
      }
    } else {
      match self.conn.query(q).await {
        Ok(ServerMessage::Result { data, .. }) => {
          println!("{}", serde_json::to_string_pretty(&data).unwrap())
        }
        Ok(ServerMessage::Error { error, .. }) => eprintln!("{}: {}", "Error".red(), error),
        _ => {}
      }
    }
  }
}
