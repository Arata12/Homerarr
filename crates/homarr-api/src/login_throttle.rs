//! Failed-login throttling for `POST /api/auth/login`, backed by the Redis store (in-memory fallback).

use std::net::{IpAddr, SocketAddr};

use axum::http::HeaderMap;
use homarr_common::{ip_matches_any, HomarrError, HomarrResult};
use homarr_core::AppConfig;
use homarr_redis::RedisStore;

const WINDOW_SECS: u64 = 15 * 60;
const MAX_FAILURES_PER_IP: u64 = 30;
const MAX_FAILURES_PER_IP_AND_USER: u64 = 10;
const MAX_USERNAME_KEY_LEN: usize = 64;

/// Peer address, or the `X-Forwarded-For` client when the peer is a configured trusted proxy
/// (`AUTH_TRUSTED_PROXY_IPS`). The rightmost entry that is not itself a trusted proxy is used,
/// since earlier entries are client-controlled.
pub fn client_ip(
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
    config: &AppConfig,
) -> Option<IpAddr> {
    let peer_ip = peer?.ip();
    let trusted = &config.auth_trusted_proxy_ips;
    if trusted.is_empty() || !ip_matches_any(peer_ip, trusted) {
        return Some(peer_ip);
    }
    let forwarded = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .filter_map(|v| v.trim().parse::<IpAddr>().ok())
        .rfind(|ip| !ip_matches_any(*ip, trusted));
    Some(forwarded.unwrap_or(peer_ip))
}

pub struct LoginThrottle {
    redis: RedisStore,
    ip_key: String,
    user_key: String,
}

impl LoginThrottle {
    pub fn new(redis: &RedisStore, ip: Option<IpAddr>, username: &str) -> Self {
        let ip = ip.map_or_else(|| "unknown".to_string(), |ip| ip.to_string());
        let name: String = username
            .trim()
            .to_lowercase()
            .chars()
            .take(MAX_USERNAME_KEY_LEN)
            .collect();
        Self {
            redis: redis.clone(),
            ip_key: format!("homarr:login-failures:ip:{ip}"),
            user_key: format!("homarr:login-failures:user:{ip}:{name}"),
        }
    }

    /// Errors with 429 (before any password hashing) when either counter is exhausted.
    pub async fn check(&self) -> HomarrResult<()> {
        // Fail open: a store outage must not lock everyone out of login.
        let ip = self.count_or_zero(&self.ip_key).await;
        let user = self.count_or_zero(&self.user_key).await;
        if ip >= MAX_FAILURES_PER_IP || user >= MAX_FAILURES_PER_IP_AND_USER {
            return Err(HomarrError::RateLimited {
                message: "Too many failed login attempts, try again later".into(),
                retry_after_secs: WINDOW_SECS,
            });
        }
        Ok(())
    }

    pub async fn record_failure(&self) {
        for key in [&self.ip_key, &self.user_key] {
            if let Err(err) = self.redis.incr_ex(key, WINDOW_SECS).await {
                tracing::warn!("Failed to record login failure: {err}");
            }
        }
    }

    pub async fn record_success(&self) {
        let _ = self.redis.delete(&self.user_key).await;
    }

    async fn count_or_zero(&self, key: &str) -> u64 {
        self.count(key).await.unwrap_or_else(|err| {
            tracing::warn!("Failed to read login failure counter: {err}");
            0
        })
    }

    async fn count(&self, key: &str) -> HomarrResult<u64> {
        Ok(self
            .redis
            .get_string(key)
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarded_header_only_honored_from_trusted_proxy() {
        let mut cfg = AppConfig::for_test();
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "1.2.3.4, 9.9.9.9, 10.0.0.2".parse().unwrap(),
        );
        let peer: SocketAddr = "10.0.0.2:5000".parse().unwrap();
        assert_eq!(client_ip(Some(peer), &headers, &cfg), Some(peer.ip()));
        cfg.auth_trusted_proxy_ips = vec!["10.0.0.0/8".into()];
        assert_eq!(
            client_ip(Some(peer), &headers, &cfg),
            Some("9.9.9.9".parse().unwrap())
        );
        let stranger: SocketAddr = "8.8.8.8:1".parse().unwrap();
        assert_eq!(
            client_ip(Some(stranger), &headers, &cfg),
            Some(stranger.ip())
        );
    }
}
