//! Server-wide request limits: default body size, per-route overrides and timeouts.

use std::time::Duration;

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Request},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use tower::ServiceBuilder;
use tower_http::timeout::RequestBodyTimeoutLayer;

/// Default request body limit for every route (JSON APIs, login, forms).
pub const DEFAULT_BODY_LIMIT: usize = 2 * 1024 * 1024;
/// Media upload: 32 MiB file plus multipart framing overhead.
pub const MEDIA_UPLOAD_BODY_LIMIT: usize = 33 * 1024 * 1024;
/// Backup import (admin only) carries the whole instance as JSON.
pub const BACKUP_IMPORT_BODY_LIMIT: usize = 256 * 1024 * 1024;

/// Time allowed to produce a response for regular routes.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// Time allowed for slow operations (uploads, backups, assistant calls).
pub const LONG_REQUEST_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Maximum idle time between request body frames (stops slow-body uploads).
pub const BODY_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// `None` means the route is exempt from the response timeout.
fn timeout_for(path: &str) -> Option<Duration> {
    if path == "/websockets" || path == "/ws" {
        return None;
    }
    if path.starts_with("/api/backup/")
        || path == "/api/medias"
        || path.starts_with("/api/assistant/")
        || path == "/api/icons/sync"
    {
        return Some(LONG_REQUEST_TIMEOUT);
    }
    Some(REQUEST_TIMEOUT)
}

async fn request_timeout(req: Request, next: Next) -> Response {
    let Some(limit) = timeout_for(req.uri().path()) else {
        return next.run(req).await;
    };
    match tokio::time::timeout(limit, next.run(req)).await {
        Ok(resp) => resp,
        Err(_) => (StatusCode::REQUEST_TIMEOUT, "Request timed out").into_response(),
    }
}

/// Applies the default body limit, response timeouts and body read timeout.
/// Per-route `DefaultBodyLimit` layers inside `router` override the default.
pub fn apply(router: Router) -> Router {
    router
        .layer(DefaultBodyLimit::max(DEFAULT_BODY_LIMIT))
        .layer(middleware::from_fn(request_timeout))
        .layer(
            ServiceBuilder::new()
                .layer(RequestBodyTimeoutLayer::new(BODY_READ_TIMEOUT))
                .map_request(|req: Request<_>| req.map(Body::new)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_is_exempt_and_long_routes_get_more_time() {
        assert_eq!(timeout_for("/websockets"), None);
        assert_eq!(timeout_for("/ws"), None);
        assert_eq!(timeout_for("/api/auth/login"), Some(REQUEST_TIMEOUT));
        assert_eq!(timeout_for("/api/medias"), Some(LONG_REQUEST_TIMEOUT));
        assert_eq!(
            timeout_for("/api/backup/import"),
            Some(LONG_REQUEST_TIMEOUT)
        );
    }
}
