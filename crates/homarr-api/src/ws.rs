use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Extension, Request, State,
    },
    http::{
        header::{COOKIE, ORIGIN},
        HeaderMap, StatusCode,
    },
    middleware::Next,
    response::IntoResponse,
};
use homarr_auth::{
    can_view_board, get_session_from_token, session_cookie_name, AuthSession, API_KEY_HEADER_NAME,
};
use homarr_db::repos::boards;
use homarr_redis::RedisStore;
use serde_json::json;

use crate::{
    csrf::{has_session_cookie, has_valid_same_origin},
    state::AppState,
};

/// Maximum concurrent WebSocket connections across all clients.
pub const WS_MAX_CONNECTIONS: usize = 1024;
/// Maximum concurrent WebSocket connections per peer IP.
pub const WS_MAX_CONNECTIONS_PER_IP: usize = 32;
/// Clients only send pings, so inbound messages and frames stay tiny.
const WS_MAX_INBOUND_BYTES: usize = 64 * 1024;

#[derive(Default)]
struct WsCounts {
    total: usize,
    per_ip: HashMap<IpAddr, usize>,
}

/// Tracks open WebSocket connections globally and per peer IP.
#[derive(Clone)]
pub struct WsLimiter {
    counts: Arc<Mutex<WsCounts>>,
    max_total: usize,
    max_per_ip: usize,
}

impl Default for WsLimiter {
    fn default() -> Self {
        Self::new(WS_MAX_CONNECTIONS, WS_MAX_CONNECTIONS_PER_IP)
    }
}

impl WsLimiter {
    pub fn new(max_total: usize, max_per_ip: usize) -> Self {
        Self {
            counts: Arc::default(),
            max_total,
            max_per_ip,
        }
    }

    /// Reserves a slot; it is released when the returned permit is dropped.
    /// Without a peer address only the global cap applies.
    pub fn try_acquire(&self, ip: Option<IpAddr>) -> Option<WsPermit> {
        let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        if counts.total >= self.max_total {
            return None;
        }
        if let Some(ip) = ip {
            let entry = counts.per_ip.entry(ip).or_insert(0);
            if *entry >= self.max_per_ip {
                return None;
            }
            *entry += 1;
        }
        counts.total += 1;
        Some(WsPermit {
            counts: self.counts.clone(),
            ip,
        })
    }
}

pub struct WsPermit {
    counts: Arc<Mutex<WsCounts>>,
    ip: Option<IpAddr>,
}

impl Drop for WsPermit {
    fn drop(&mut self) {
        let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        counts.total = counts.total.saturating_sub(1);
        if let Some(ip) = self.ip {
            if let Some(n) = counts.per_ip.get_mut(&ip) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    counts.per_ip.remove(&ip);
                }
            }
        }
    }
}

/// Middleware rejecting WebSocket requests carrying an `ApiKey` header BEFORE upgrade extraction.
pub async fn reject_api_key_middleware(req: Request, next: Next) -> impl IntoResponse {
    if req.headers().contains_key(API_KEY_HEADER_NAME) {
        return (
            StatusCode::FORBIDDEN,
            "API key authentication is not permitted on WebSocket endpoints",
        )
            .into_response();
    }
    next.run(req).await
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // API keys must NEVER authenticate WebSocket connections (matches Homarr security model).
    if headers.contains_key(API_KEY_HEADER_NAME) {
        return (
            StatusCode::FORBIDDEN,
            "API key authentication is not permitted on WebSocket endpoints",
        )
            .into_response();
    }

    let cookie_name = session_cookie_name(&state.config.auth_cookie_prefix);
    let has_cookie = has_session_cookie(&headers, &cookie_name);
    let has_proxy_auth = state
        .config
        .auth_trusted_proxy_header
        .as_deref()
        .is_some_and(|h| headers.contains_key(h));

    if (headers.contains_key(ORIGIN) || has_cookie || has_proxy_auth)
        && !has_valid_same_origin(&headers, state.config.base_url.as_deref())
    {
        return (
            StatusCode::FORBIDDEN,
            "Cross-origin WebSocket connection rejected",
        )
            .into_response();
    }

    let session_token = headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|hdr| {
            hdr.split(';').find_map(|part| {
                let (k, v) = part.trim().split_once('=')?;
                if k.trim() == cookie_name {
                    Some(v.trim().to_string())
                } else {
                    None
                }
            })
        });

    let session = match session_token {
        Some(tok) => get_session_from_token(&state.db, &tok).await.ok().flatten(),
        None => None,
    };

    // Checked after auth/origin so the slot is only held for connections that will upgrade.
    let Some(permit) = state
        .ws_limiter
        .try_acquire(peer.map(|Extension(ConnectInfo(addr))| addr.ip()))
    else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "Too many WebSocket connections",
        )
            .into_response();
    };

    ws.max_message_size(WS_MAX_INBOUND_BYTES)
        .max_frame_size(WS_MAX_INBOUND_BYTES)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            handle_socket(socket, state, session).await;
        })
        .into_response()
}

/// Whether a Redis broadcast may be forwarded to this client. Mirrors the original's
/// per-subscription checks: ping results need a signed-in user, board events need view
/// access to that board (signed-in if the event has no board id), everything else
/// (cron job status, unknown channels) is admin-only.
async fn may_receive(
    state: &AppState,
    session: Option<&AuthSession>,
    channel: &str,
    payload: &serde_json::Value,
) -> bool {
    match channel {
        RedisStore::PING_EVENTS_CHANNEL => session.is_some(),
        RedisStore::BOARD_EVENTS_CHANNEL => {
            let Some(board_id) = payload.get("boardId").and_then(|v| v.as_str()) else {
                return session.is_some();
            };
            match boards::get_by_id(&state.db, board_id).await {
                Ok(Some(board)) => can_view_board(&state.db, session, &board)
                    .await
                    .unwrap_or(false),
                _ => false,
            }
        }
        _ => session.is_some_and(AuthSession::is_admin),
    }
}

async fn handle_socket(mut socket: WebSocket, state: AppState, session: Option<AuthSession>) {
    let is_admin = session.as_ref().is_some_and(AuthSession::is_admin);
    let mut redis_rx = state.redis.subscribe();
    let mut logs_rx = state.logs.subscribe();
    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));

    let welcome = json!({
        "channel": "system:hello",
        "payload": {
            "version": env!("CARGO_PKG_VERSION"),
            "admin": is_admin
        }
    });
    if socket
        .send(Message::Text(welcome.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(p))) => {
                        if socket.send(Message::Pong(p)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Text(txt))) => {
                        if txt.trim() == "ping" {
                            let _ = socket.send(Message::Text(json!({ "channel": "pong" }).to_string().into())).await;
                        }
                    }
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            pubsub = redis_rx.recv() => {
                if let Ok(env) = pubsub {
                    let payload_val = match serde_json::from_str::<serde_json::Value>(&env.payload) {
                        Ok(v) => v,
                        Err(_) => serde_json::Value::String(env.payload),
                    };
                    if !may_receive(&state, session.as_ref(), &env.channel, &payload_val).await {
                        continue;
                    }
                    let frame = json!({
                        "channel": env.channel,
                        "payload": payload_val
                    });
                    if socket.send(Message::Text(frame.to_string().into())).await.is_err() {
                        break;
                    }
                }
            }
            log_ev = logs_rx.recv(), if is_admin => {
                if let Ok(entry) = log_ev {
                    let frame = json!({
                        "channel": "system:logs",
                        "payload": entry
                    });
                    if socket.send(Message::Text(frame.to_string().into())).await.is_err() {
                        break;
                    }
                }
            }
            _ = ping_interval.tick() => {
                if socket.send(Message::Ping(vec![1, 2, 3].into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_enforces_per_ip_and_global_caps_and_releases_on_drop() {
        let limiter = WsLimiter::new(3, 2);
        let a: IpAddr = "10.0.0.1".parse().unwrap();
        let b: IpAddr = "10.0.0.2".parse().unwrap();

        let a1 = limiter.try_acquire(Some(a)).unwrap();
        let _a2 = limiter.try_acquire(Some(a)).unwrap();
        assert!(limiter.try_acquire(Some(a)).is_none(), "per-IP cap");

        let _b1 = limiter.try_acquire(Some(b)).unwrap();
        assert!(limiter.try_acquire(Some(b)).is_none(), "global cap");

        drop(a1);
        assert!(limiter.try_acquire(Some(a)).is_some(), "slot released");
    }
}
