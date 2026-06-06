//! WebSocket hub — broadcast ServerMessages to connected dashboard clients.
//!
//! Uses tokio::sync::broadcast for fan-out. The digest task sends
//! ServerMessages through a broadcast channel, and each WebSocket
//! connection subscribes and forwards to its client.

use std::sync::Arc;
use axum::{
    extract::State,
    response::IntoResponse,
};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use tokio::sync::broadcast;
use trapfall_proto::ServerMessage;

/// Shared hub that broadcast ServerMessages to all connected WS clients.
#[derive(Clone)]
pub struct WsHub {
    tx: broadcast::Sender<Arc<ServerMessage>>,
}

impl WsHub {
    /// Create a new hub with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Broadcast a message to all connected clients.
    pub fn send(&self, msg: ServerMessage) {
        // Ignore send error — means no receivers
        let _ = self.tx.send(Arc::new(msg));
    }

    /// Subscribe to broadcasts. Returns a receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<ServerMessage>> {
        self.tx.subscribe()
    }
}

/// WebSocket upgrade handler — accepts connection and starts sending messages.
pub async fn ws_handler(
    State(state): State<crate::server::AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.ws_hub.clone()))
}

async fn handle_socket(socket: WebSocket, hub: WsHub) {
    let mut rx = hub.subscribe();

    // Forward broadcast messages to this WebSocket client
    loop {
        match rx.recv().await {
            Ok(msg) => {
                let json = match serde_json::to_string(&*msg) {
                    Ok(j) => j,
                    Err(e) => {
                        tracing::warn!("WS serialize error: {e}");
                        continue;
                    }
                };
                if socket.send(Message::Text(json.into())).await.is_err() {
                    // Client disconnected
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::debug!("WS client lagged {n} messages");
                continue;
            }
            Err(broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }
}
