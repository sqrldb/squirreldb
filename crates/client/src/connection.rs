use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use types::{ClientMessage, ServerMessage};

pub struct Connection {
  tx: mpsc::UnboundedSender<(ClientMessage, oneshot::Sender<ServerMessage>)>,
  sub_rx: Arc<Mutex<mpsc::UnboundedReceiver<ServerMessage>>>,
}

impl Connection {
  pub async fn connect(url: &str) -> Result<Self, anyhow::Error> {
    let ws_url = if url.starts_with("ws://") {
      url.into()
    } else {
      format!("ws://{}", url)
    };
    let (ws, _) = tokio_tungstenite::connect_async(&ws_url).await?;
    let (mut sink, mut stream) = ws.split();

    let (req_tx, mut req_rx) =
      mpsc::unbounded_channel::<(ClientMessage, oneshot::Sender<ServerMessage>)>();
    let (sub_tx, sub_rx) = mpsc::unbounded_channel();
    let pending: Arc<Mutex<HashMap<String, oneshot::Sender<ServerMessage>>>> =
      Arc::new(Mutex::new(HashMap::new()));

    let pending2 = pending.clone();
    tokio::spawn(async move {
      while let Some((msg, resp_tx)) = req_rx.recv().await {
        pending2.lock().await.insert(msg.id().into(), resp_tx);
        if sink
          .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
          .await
          .is_err()
        {
          break;
        }
      }
    });

    tokio::spawn(async move {
      while let Some(Ok(Message::Text(text))) = stream.next().await {
        if let Ok(msg) = serde_json::from_str::<ServerMessage>(&text) {
          let id = match &msg {
            ServerMessage::Change { .. } => {
              let _ = sub_tx.send(msg);
              continue;
            }
            ServerMessage::Result { id, .. }
            | ServerMessage::Subscribed { id }
            | ServerMessage::Unsubscribed { id }
            | ServerMessage::ProjectSelected { id, .. }
            | ServerMessage::Error { id, .. }
            | ServerMessage::Pong { id } => id.clone(),
          };
          if let Some(tx) = pending.lock().await.remove(&id) {
            let _ = tx.send(msg);
          }
        }
      }
    });

    Ok(Self {
      tx: req_tx,
      sub_rx: Arc::new(Mutex::new(sub_rx)),
    })
  }

  pub async fn send(&self, msg: ClientMessage) -> Result<ServerMessage, anyhow::Error> {
    let (tx, rx) = oneshot::channel();
    self
      .tx
      .send((msg, tx))
      .map_err(|_| anyhow::anyhow!("closed"))?;
    rx.await.map_err(|_| anyhow::anyhow!("closed"))
  }

  pub async fn query(&self, q: &str) -> Result<ServerMessage, anyhow::Error> {
    self
      .send(ClientMessage::Query {
        id: Uuid::new_v4().to_string(),
        query: q.into(),
      })
      .await
  }

  pub async fn subscribe(&self, q: &str) -> Result<ServerMessage, anyhow::Error> {
    self
      .send(ClientMessage::Subscribe {
        id: Uuid::new_v4().to_string(),
        query: q.into(),
      })
      .await
  }

  pub async fn list_collections(&self) -> Result<ServerMessage, anyhow::Error> {
    self
      .send(ClientMessage::ListCollections {
        id: Uuid::new_v4().to_string(),
      })
      .await
  }

  pub async fn ping(&self) -> Result<ServerMessage, anyhow::Error> {
    self
      .send(ClientMessage::Ping {
        id: Uuid::new_v4().to_string(),
      })
      .await
  }

  pub async fn recv_change(&self) -> Option<ServerMessage> {
    self.sub_rx.lock().await.recv().await
  }
}
