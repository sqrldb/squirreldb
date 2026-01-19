use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use super::{MessageHandler, RateLimiter};
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
}

impl WebSocketServer {
  pub fn new(
    backend: Arc<dyn DatabaseBackend>,
    subs: Arc<SubscriptionManager>,
    engine_pool: Arc<QueryEnginePool>,
    rate_limiter: Arc<RateLimiter>,
    shutdown_rx: broadcast::Receiver<()>,
  ) -> Self {
    Self {
      backend,
      subs,
      engine_pool,
      rate_limiter,
      clients: Arc::new(RwLock::new(HashMap::new())),
      shutdown_rx,
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
          tokio::spawn(handle_client(
            stream,
            peer_ip,
            backend,
            subs,
            engine_pool,
            rate_limiter,
            clients,
          ));
        }
        _ = self.shutdown_rx.recv() => break,
      }
    }
    Ok(())
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
) {
  let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
    rate_limiter.release_connection(peer_ip);
    return;
  };
  let client_id = Uuid::new_v4();
  let (mut sink, mut stream) = ws.split();
  let (tx, mut rx) = mpsc::unbounded_channel();

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
  subs.remove_client(client_id);
  rate_limiter.release_connection(peer_ip);
  send_task.abort();
}
