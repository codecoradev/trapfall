//! WebSocket hub — broadcast ServerMessages to connected dashboard clients.
//!
//! Auth-protected: the middleware layer on the dashboard API router
//! validates the session cookie before the WS upgrade reaches this handler.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::{extract::State, http::StatusCode, response::IntoResponse};
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

/// WebSocket upgrade handler — validates session cookie before upgrade.
pub async fn ws_handler(
    State(state): State<crate::server::AppState>,
    ws: WebSocketUpgrade,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // Auth check: validate session cookie
    let token = crate::auth::extract_session_token(&headers);
    if let Some(token) = token {
        let store = trapfall_core::Store::new(state.pool.clone());
        if store.get_session(&token).await.is_ok_and(|s| s.is_some()) {
            return ws.on_upgrade(move |socket| handle_socket(socket, state.ws_hub.clone()));
        }
    }
    (StatusCode::UNAUTHORIZED, "Not authenticated").into_response()
}

async fn handle_socket(mut socket: WebSocket, hub: WsHub) {
    let mut rx = hub.subscribe();
    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));

    loop {
        tokio::select! {
            msg_result = rx.recv() => {
                match msg_result {
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
            _ = ping_interval.tick() => {
                if socket.send(Message::Ping(vec![].into())).await.is_err() {
                    break; // Client disconnected
                }
            }
        }
    }
}
