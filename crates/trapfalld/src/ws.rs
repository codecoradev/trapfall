//! WebSocket hub — broadcast ServerMessages to connected dashboard clients.
//!
//! Auth-protected: the middleware layer on the dashboard API router
//! validates the session cookie before the WS upgrade reaches this handler.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::{extract::State, response::IntoResponse};
use std::sync::Arc;
use tokio::sync::broadcast;
use trapfall_proto::ServerMessage;

/// Shared hub that broadcasts ServerMessages to all connected WS clients.
#[derive(Clone)]
pub struct WsHub {
    tx: broadcast::Sender<Arc<ServerMessage>>,
}

impl WsHub {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn send(&self, msg: ServerMessage) {
        let _ = self.tx.send(Arc::new(msg));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<ServerMessage>> {
        self.tx.subscribe()
    }
}

/// WebSocket upgrade handler — auth is enforced by the router middleware layer.
pub async fn ws_handler(State(state): State<crate::server::AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.ws_hub.clone()))
}

async fn handle_socket(mut socket: WebSocket, hub: WsHub) {
    let mut rx = hub.subscribe();

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
