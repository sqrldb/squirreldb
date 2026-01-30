//! Native TCP wire protocol server for SquirrelDB.
//!
//! Wire Protocol Specification:
//!
//! ## Handshake
//! Client → Server:
//! - Magic: 4 bytes "SQRL"
//! - Version: 1 byte (0x01)
//! - Flags: 1 byte (bit 0: MessagePack, bit 1: JSON fallback)
//! - Auth Token Length: 2 bytes BE
//! - Auth Token: variable UTF-8
//!
//! Server → Client:
//! - Status: 1 byte (0x00=success, 0x01=version mismatch, 0x02=auth failed)
//! - Version: 1 byte
//! - Flags: 1 byte
//! - Session ID: 16 bytes UUID
//!
//! ## Message Framing
//! - Length: 4 bytes BE (max 16MB)
//! - Message Type: 1 byte (0x01=request, 0x02=response, 0x03=notification)
//! - Encoding: 1 byte (0x01=MessagePack, 0x02=JSON)
//! - Payload: variable

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

use super::{MessageHandler, RateLimiter, ServerConfig};
use crate::db::DatabaseBackend;
use crate::query::QueryEnginePool;
use crate::subscriptions::SubscriptionManager;
use crate::types::{ClientMessage, ServerMessage};

/// Protocol constants
pub const MAGIC: &[u8; 4] = b"SQRL";
pub const PROTOCOL_VERSION: u8 = 0x01;
pub const MAX_MESSAGE_SIZE: u32 = 16 * 1024 * 1024; // 16MB

/// Handshake status codes
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HandshakeStatus {
  Success = 0x00,
  VersionMismatch = 0x01,
  AuthFailed = 0x02,
}

/// Message types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageType {
  Request = 0x01,
  Response = 0x02,
  Notification = 0x03,
}

impl TryFrom<u8> for MessageType {
  type Error = ();
  fn try_from(v: u8) -> Result<Self, Self::Error> {
    match v {
      0x01 => Ok(Self::Request),
      0x02 => Ok(Self::Response),
      0x03 => Ok(Self::Notification),
      _ => Err(()),
    }
  }
}

/// Encoding formats
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Encoding {
  #[default]
  MessagePack = 0x01,
  Json = 0x02,
}

impl TryFrom<u8> for Encoding {
  type Error = ();
  fn try_from(v: u8) -> Result<Self, Self::Error> {
    match v {
      0x01 => Ok(Self::MessagePack),
      0x02 => Ok(Self::Json),
      _ => Err(()),
    }
  }
}

/// Flags in the handshake
#[derive(Debug, Clone, Copy)]
pub struct ProtocolFlags {
  pub messagepack: bool,
  pub json_fallback: bool,
}

impl From<u8> for ProtocolFlags {
  fn from(byte: u8) -> Self {
    Self {
      messagepack: byte & 0x01 != 0,
      json_fallback: byte & 0x02 != 0,
    }
  }
}

impl From<ProtocolFlags> for u8 {
  fn from(flags: ProtocolFlags) -> u8 {
    let mut byte = 0u8;
    if flags.messagepack {
      byte |= 0x01;
    }
    if flags.json_fallback {
      byte |= 0x02;
    }
    byte
  }
}

type Clients = Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<ServerMessage>>>>;

pub struct TcpServer {
  backend: Arc<dyn DatabaseBackend>,
  subs: Arc<SubscriptionManager>,
  engine_pool: Arc<QueryEnginePool>,
  rate_limiter: Arc<RateLimiter>,
  clients: Clients,
  shutdown_rx: broadcast::Receiver<()>,
  config: ServerConfig,
}

impl TcpServer {
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
    tracing::info!("TCP wire protocol listening on {}", addr);

    // Spawn task to forward subscription messages to clients
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
            tracing::warn!("TCP connection rejected from {}: {}", peer_ip, e);
            continue;
          }

          tracing::debug!("TCP connection from {}", peer);
          let backend = self.backend.clone();
          let subs = self.subs.clone();
          let engine_pool = self.engine_pool.clone();
          let rate_limiter = self.rate_limiter.clone();
          let clients = self.clients.clone();
          let config = self.config.clone();
          tokio::spawn(async move {
            let result = handle_client(
              stream,
              peer_ip,
              backend,
              subs,
              engine_pool,
              rate_limiter.clone(),
              clients,
              config,
            ).await;
            rate_limiter.release_connection(peer_ip);
            if let Err(e) = result {
              tracing::debug!("TCP client error: {}", e);
            }
          });
        }
        _ = self.shutdown_rx.recv() => {
          tracing::info!("TCP server shutting down");
          break;
        }
      }
    }
    Ok(())
  }
}

/// Handle handshake from client
async fn handle_handshake(
  stream: &mut TcpStream,
  config: &ServerConfig,
) -> Result<(Uuid, Encoding), anyhow::Error> {
  // Read magic
  let mut magic = [0u8; 4];
  stream.read_exact(&mut magic).await?;
  if &magic != MAGIC {
    anyhow::bail!("Invalid magic bytes");
  }

  // Read version
  let version = stream.read_u8().await?;
  if version != PROTOCOL_VERSION {
    // Send version mismatch response
    stream
      .write_u8(HandshakeStatus::VersionMismatch as u8)
      .await?;
    stream.write_u8(PROTOCOL_VERSION).await?;
    stream.write_u8(0).await?;
    stream.write_all(&[0u8; 16]).await?;
    stream.flush().await?;
    anyhow::bail!(
      "Protocol version mismatch: client={}, server={}",
      version,
      PROTOCOL_VERSION
    );
  }

  // Read flags
  let flags_byte = stream.read_u8().await?;
  let flags = ProtocolFlags::from(flags_byte);

  // Read auth token
  let token_len = stream.read_u16().await?;
  let mut token_bytes = vec![0u8; token_len as usize];
  if token_len > 0 {
    stream.read_exact(&mut token_bytes).await?;
  }
  let auth_token = String::from_utf8(token_bytes).unwrap_or_default();

  // Validate auth if enabled
  if config.auth.enabled {
    // Check admin token first
    let valid_admin = config
      .auth
      .admin_token
      .as_ref()
      .is_some_and(|t| !t.is_empty() && t == &auth_token);

    // If admin token doesn't match, could check against token store
    // For now, only admin_token is supported for TCP protocol
    if !valid_admin {
      // Send auth failed response
      stream.write_u8(HandshakeStatus::AuthFailed as u8).await?;
      stream.write_u8(PROTOCOL_VERSION).await?;
      stream.write_u8(0).await?;
      stream.write_all(&[0u8; 16]).await?;
      stream.flush().await?;
      anyhow::bail!("Authentication failed: invalid or missing token");
    }
  }

  // Generate session ID
  let session_id = Uuid::new_v4();

  // Determine encoding preference
  let encoding = if flags.messagepack {
    Encoding::MessagePack
  } else {
    Encoding::Json
  };

  // Send success response
  stream.write_u8(HandshakeStatus::Success as u8).await?;
  stream.write_u8(PROTOCOL_VERSION).await?;
  stream.write_u8(flags_byte).await?;
  stream.write_all(session_id.as_bytes()).await?;
  stream.flush().await?;

  tracing::debug!(
    "TCP handshake complete: session={}, encoding={:?}",
    session_id,
    encoding
  );
  Ok((session_id, encoding))
}

/// Read a framed message
async fn read_frame(
  reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Result<(MessageType, Encoding, Vec<u8>), anyhow::Error> {
  // Read length (4 bytes BE)
  let length = reader.read_u32().await?;
  if length > MAX_MESSAGE_SIZE {
    anyhow::bail!("Message too large: {} > {}", length, MAX_MESSAGE_SIZE);
  }

  // Read message type (1 byte)
  let msg_type_byte = reader.read_u8().await?;
  let msg_type = MessageType::try_from(msg_type_byte)
    .map_err(|_| anyhow::anyhow!("Invalid message type: {}", msg_type_byte))?;

  // Read encoding (1 byte)
  let encoding_byte = reader.read_u8().await?;
  let encoding = Encoding::try_from(encoding_byte)
    .map_err(|_| anyhow::anyhow!("Invalid encoding: {}", encoding_byte))?;

  // Read payload
  let payload_len = length as usize - 2; // subtract type and encoding bytes
  let mut payload = vec![0u8; payload_len];
  reader.read_exact(&mut payload).await?;

  Ok((msg_type, encoding, payload))
}

/// Write a framed message
async fn write_frame(
  writer: &mut BufWriter<tokio::net::tcp::OwnedWriteHalf>,
  msg_type: MessageType,
  encoding: Encoding,
  payload: &[u8],
) -> Result<(), anyhow::Error> {
  let length = (payload.len() + 2) as u32; // +2 for type and encoding bytes

  writer.write_u32(length).await?;
  writer.write_u8(msg_type as u8).await?;
  writer.write_u8(encoding as u8).await?;
  writer.write_all(payload).await?;
  writer.flush().await?;

  Ok(())
}

/// Serialize a message with the given encoding
fn serialize_message(msg: &ServerMessage, encoding: Encoding) -> Result<Vec<u8>, anyhow::Error> {
  match encoding {
    Encoding::MessagePack => Ok(rmp_serde::to_vec(msg)?),
    Encoding::Json => Ok(serde_json::to_vec(msg)?),
  }
}

/// Deserialize a message with the given encoding
fn deserialize_message(data: &[u8], encoding: Encoding) -> Result<ClientMessage, anyhow::Error> {
  match encoding {
    Encoding::MessagePack => Ok(rmp_serde::from_slice(data)?),
    Encoding::Json => Ok(serde_json::from_slice(data)?),
  }
}

/// Handle a single TCP client connection
#[allow(clippy::too_many_arguments)]
async fn handle_client(
  mut stream: TcpStream,
  peer_ip: IpAddr,
  backend: Arc<dyn DatabaseBackend>,
  subs: Arc<SubscriptionManager>,
  engine_pool: Arc<QueryEnginePool>,
  rate_limiter: Arc<RateLimiter>,
  clients: Clients,
  config: ServerConfig,
) -> Result<(), anyhow::Error> {
  // Perform handshake
  let (client_id, encoding) = handle_handshake(&mut stream, &config).await?;

  // Split stream for concurrent read/write
  let (read_half, write_half) = stream.into_split();
  let mut reader = BufReader::new(read_half);
  let mut writer = BufWriter::new(write_half);

  // Create channel for sending messages to this client
  let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();
  clients.write().await.insert(client_id, tx);

  // Create message handler
  let handler = MessageHandler::new(backend, subs.clone(), engine_pool);
  let query_timeout = rate_limiter.query_timeout();

  // Spawn task to write outgoing messages
  let write_encoding = encoding;
  let write_task = tokio::spawn(async move {
    while let Some(msg) = rx.recv().await {
      let payload = match serialize_message(&msg, write_encoding) {
        Ok(p) => p,
        Err(e) => {
          tracing::error!("Failed to serialize message: {}", e);
          continue;
        }
      };

      // Determine message type based on ServerMessage variant
      let msg_type = match &msg {
        ServerMessage::Change { .. } => MessageType::Notification,
        _ => MessageType::Response,
      };

      if let Err(e) = write_frame(&mut writer, msg_type, write_encoding, &payload).await {
        tracing::debug!("Failed to write frame: {}", e);
        break;
      }
    }
  });

  // Read and process incoming messages
  loop {
    match read_frame(&mut reader).await {
      Ok((msg_type, frame_encoding, payload)) => {
        if msg_type != MessageType::Request {
          tracing::warn!("Unexpected message type from client: {:?}", msg_type);
          continue;
        }

        // Check request rate limit
        if let Err(e) = rate_limiter.check_request(peer_ip) {
          tracing::debug!("Rate limited request from {}: {}", peer_ip, e);
          let error_msg = ServerMessage::error("0", format!("Rate limited: {}", e));
          if let Some(tx) = clients.read().await.get(&client_id) {
            let _ = tx.send(error_msg);
          }
          continue;
        }

        // Deserialize the request
        let client_msg = match deserialize_message(&payload, frame_encoding) {
          Ok(m) => m,
          Err(e) => {
            tracing::debug!("Failed to deserialize message: {}", e);
            // Send error response
            let error_msg = ServerMessage::error("0", format!("Invalid message: {}", e));
            if let Some(tx) = clients.read().await.get(&client_id) {
              let _ = tx.send(error_msg);
            }
            continue;
          }
        };

        let msg_id = client_msg.id().to_string();

        // Acquire query permit
        let permit = match rate_limiter.acquire_query_permit(client_id) {
          Ok(p) => p,
          Err(e) => {
            tracing::debug!("Query limit exceeded for {}: {}", client_id, e);
            let error_msg = ServerMessage::error(&msg_id, e.to_string());
            if let Some(tx) = clients.read().await.get(&client_id) {
              let _ = tx.send(error_msg);
            }
            continue;
          }
        };

        // Handle the message with optional timeout
        let resp = if let Some(timeout) = query_timeout {
          match tokio::time::timeout(timeout, handler.handle(client_id, client_msg)).await {
            Ok(r) => r,
            Err(_) => {
              tracing::warn!("Query timeout for client {}", client_id);
              ServerMessage::error(&msg_id, "Query execution timed out")
            }
          }
        } else {
          handler.handle(client_id, client_msg).await
        };

        drop(permit); // Release query permit

        if let Some(tx) = clients.read().await.get(&client_id) {
          let _ = tx.send(resp);
        }
      }
      Err(e) => {
        tracing::debug!("TCP client read error: {}", e);
        break;
      }
    }
  }

  // Cleanup
  clients.write().await.remove(&client_id);
  subs.remove_client(client_id).await;
  write_task.abort();

  tracing::debug!("TCP client {} disconnected", client_id);
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_protocol_flags() {
    let flags = ProtocolFlags {
      messagepack: true,
      json_fallback: false,
    };
    let byte: u8 = flags.into();
    assert_eq!(byte, 0x01);

    let flags = ProtocolFlags {
      messagepack: true,
      json_fallback: true,
    };
    let byte: u8 = flags.into();
    assert_eq!(byte, 0x03);

    let flags = ProtocolFlags::from(0x03);
    assert!(flags.messagepack);
    assert!(flags.json_fallback);
  }

  #[test]
  fn test_message_type_conversion() {
    assert_eq!(MessageType::try_from(0x01), Ok(MessageType::Request));
    assert_eq!(MessageType::try_from(0x02), Ok(MessageType::Response));
    assert_eq!(MessageType::try_from(0x03), Ok(MessageType::Notification));
    assert_eq!(MessageType::try_from(0x99), Err(()));
  }

  #[test]
  fn test_encoding_conversion() {
    assert_eq!(Encoding::try_from(0x01), Ok(Encoding::MessagePack));
    assert_eq!(Encoding::try_from(0x02), Ok(Encoding::Json));
    assert_eq!(Encoding::try_from(0x99), Err(()));
  }
}
