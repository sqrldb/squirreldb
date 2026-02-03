use futures_util::{SinkExt, StreamExt};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use super::{MessageHandler, RateLimiter, ServerConfig};
use crate::db::DatabaseBackend;
use crate::query::QueryEnginePool;
use crate::subscriptions::SubscriptionManager;
use crate::types::{ClientMessage, ServerMessage};

type Clients = Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<ServerMessage>>>>;

pub struct WebSocketServer {
  backend: Arc<dyn DatabaseBackend>,
  subs: Arc<SubscriptionManager>,
  engine_pool: Arc<QueryEnginePool>,
  rate_limiter: Arc<RateLimiter>,
  clients: Clients,
  shutdown_rx: broadcast::Receiver<()>,
  config: ServerConfig,
}

impl WebSocketServer {
  pub fn new(
    backend: Arc<dyn DatabaseBackend>,
    subs: Arc<SubscriptionManager>,
    engine_pool: Arc<QueryEnginePool>,
    rate_limiter: Arc<RateLimiter>,
    shutdown_rx: broadcast::Receiver<()>,
    config: ServerConfig,
  ) -> Self {
    Self {
      backend,
      subs,
      engine_pool,
      rate_limiter,
      clients: Arc::new(RwLock::new(HashMap::new())),
      shutdown_rx,
      config,
    }
  }

  pub async fn run(mut self, addr: &str) -> Result<(), anyhow::Error> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("WebSocket listening on {}", addr);

    let clients = self.clients.clone();
    let subs = self.subs.clone();
    tokio::spawn(async move {
      let mut rx = subs.subscribe_to_outgoing();
      while let Ok((client_id, msg)) = rx.recv().await {
        if let Some(tx) = clients.read().await.get(&client_id) {
          let _ = tx.send(msg);
        }
      }
    });

    loop {
      tokio::select! {
        Ok((stream, peer)) = listener.accept() => {
          let peer_ip = peer.ip();

          // Check connection rate limit
          if let Err(e) = self.rate_limiter.check_connection(peer_ip) {
            tracing::warn!("Connection rejected from {}: {}", peer_ip, e);
            continue;
          }

          let backend = self.backend.clone();
          let subs = self.subs.clone();
          let engine_pool = self.engine_pool.clone();
          let rate_limiter = self.rate_limiter.clone();
          let clients = self.clients.clone();
          let config = self.config.clone();
          tokio::spawn(handle_client(
            stream,
            peer_ip,
            backend,
            subs,
            engine_pool,
            rate_limiter,
            clients,
            config,
          ));
        }
        _ = self.shutdown_rx.recv() => break,
      }
    }
    Ok(())
  }
}

/// Hash a token using SHA-256 for validation
fn hash_token(token: &str) -> String {
  let mut hasher = Sha256::new();
  hasher.update(token.as_bytes());
  format!("{:x}", hasher.finalize())
}

/// Authenticate a WebSocket client
/// Returns Ok(project_id) if authentication is successful, or None if auth is disabled
async fn authenticate_client(
  backend: &Arc<dyn DatabaseBackend>,
  config: &ServerConfig,
  first_message: Option<&str>,
) -> Result<Option<Uuid>, String> {
  // If auth is disabled, allow all connections
  if !config.auth.enabled {
    return Ok(None);
  }

  // Extract token from first message (expected format: {"type":"Auth","token":"..."})
  let token = match first_message {
    Some(msg) => {
      if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(msg) {
        if parsed.get("type").and_then(|t| t.as_str()) == Some("Auth") {
          parsed.get("token").and_then(|t| t.as_str()).map(|s| s.to_string())
        } else {
          None
        }
      } else {
        None
      }
    }
    None => None,
  };

  let token = token.ok_or_else(|| "Authentication required. Send {\"type\":\"Auth\",\"token\":\"your_token\"} as first message".to_string())?;

  // Check if it's the admin token
  if let Some(ref admin_token) = config.auth.admin_token {
    if !admin_token.is_empty() && crate::security::constant_time_compare(&token, admin_token) {
      return Ok(None); // Admin token grants access to all projects
    }
  }

  // Validate as API token
  let token_hash = hash_token(&token);
  match backend.validate_token(&token_hash).await {
    Ok(Some(project_id)) => Ok(Some(project_id)),
    Ok(None) => Err("Invalid token".to_string()),
    Err(e) => Err(format!("Authentication error: {}", e)),
  }
}

async fn handle_client(
  stream: TcpStream,
  peer_ip: IpAddr,
  backend: Arc<dyn DatabaseBackend>,
  subs: Arc<SubscriptionManager>,
  engine_pool: Arc<QueryEnginePool>,
  rate_limiter: Arc<RateLimiter>,
  clients: Clients,
  config: ServerConfig,
) {
  let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
    rate_limiter.release_connection(peer_ip);
    return;
  };
  let client_id = Uuid::new_v4();
  let (mut sink, mut stream) = ws.split();
  let (tx, mut rx) = mpsc::unbounded_channel();

  // If auth is enabled, require authentication as first message
  let mut authenticated = !config.auth.enabled;
  let mut _project_id: Option<Uuid> = None;

  if config.auth.enabled {
    // Wait for auth message with timeout
    let auth_timeout = tokio::time::Duration::from_secs(30);
    let auth_result = tokio::time::timeout(auth_timeout, stream.next()).await;

    match auth_result {
      Ok(Some(Ok(Message::Text(text)))) => {
        match authenticate_client(&backend, &config, Some(&text)).await {
          Ok(pid) => {
            authenticated = true;
            _project_id = pid;
            // Send auth success
            let success = serde_json::json!({"type": "AuthSuccess"});
            if sink.send(Message::Text(success.to_string().into())).await.is_err() {
              rate_limiter.release_connection(peer_ip);
              return;
            }
          }
          Err(e) => {
            // Send auth failure and close
            let failure = serde_json::json!({"type": "AuthFailure", "error": e});
            let _ = sink.send(Message::Text(failure.to_string().into())).await;
            tracing::warn!("WebSocket auth failed from {}: {}", peer_ip, e);
            rate_limiter.release_connection(peer_ip);
            return;
          }
        }
      }
      Ok(Some(Ok(_))) => {
        let failure = serde_json::json!({"type": "AuthFailure", "error": "Expected text message for authentication"});
        let _ = sink.send(Message::Text(failure.to_string().into())).await;
        rate_limiter.release_connection(peer_ip);
        return;
      }
      Ok(Some(Err(_))) | Ok(None) => {
        rate_limiter.release_connection(peer_ip);
        return;
      }
      Err(_) => {
        // Timeout
        let failure = serde_json::json!({"type": "AuthFailure", "error": "Authentication timeout"});
        let _ = sink.send(Message::Text(failure.to_string().into())).await;
        rate_limiter.release_connection(peer_ip);
        return;
      }
    }
  }

  if !authenticated {
    rate_limiter.release_connection(peer_ip);
    return;
  }

  clients.write().await.insert(client_id, tx);
  let handler = MessageHandler::new(backend, subs.clone(), engine_pool);
  let query_timeout = rate_limiter.query_timeout();

  let send_task = tokio::spawn(async move {
    while let Some(msg) = rx.recv().await {
      let serialized = match serde_json::to_string(&msg) {
        Ok(s) => s,
        Err(e) => {
          tracing::error!("Failed to serialize message: {}", e);
          continue;
        }
      };
      if sink.send(Message::Text(serialized.into())).await.is_err() {
        break;
      }
    }
  });

  while let Some(Ok(Message::Text(text))) = stream.next().await {
    // Check request rate limit
    if let Err(e) = rate_limiter.check_request(peer_ip) {
      tracing::debug!("Rate limited request from {}: {}", peer_ip, e);
      if let Some(tx) = clients.read().await.get(&client_id) {
        let _ = tx.send(ServerMessage::error("0", format!("Rate limited: {}", e)));
      }
      continue;
    }

    if let Ok(msg) = serde_json::from_str::<ClientMessage>(&text) {
      let msg_id = msg.id().to_string();

      // Acquire query permit
      let permit = match rate_limiter.acquire_query_permit(client_id) {
        Ok(p) => p,
        Err(e) => {
          tracing::debug!("Query limit exceeded for {}: {}", client_id, e);
          if let Some(tx) = clients.read().await.get(&client_id) {
            let _ = tx.send(ServerMessage::error(&msg_id, e.to_string()));
          }
          continue;
        }
      };

      // Handle the message with optional timeout
      let resp = if let Some(timeout) = query_timeout {
        match tokio::time::timeout(timeout, handler.handle(client_id, msg)).await {
          Ok(r) => r,
          Err(_) => {
            tracing::warn!("Query timeout for client {}", client_id);
            ServerMessage::error(&msg_id, "Query execution timed out")
          }
        }
      } else {
        handler.handle(client_id, msg).await
      };

      drop(permit); // Release query permit

      if let Some(tx) = clients.read().await.get(&client_id) {
        let _ = tx.send(resp);
      }
    }
  }

  clients.write().await.remove(&client_id);
  subs.remove_client(client_id).await;
  rate_limiter.release_connection(peer_ip);
  send_task.abort();
}
