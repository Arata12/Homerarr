use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard};
use std::time::Duration;

use futures_util::StreamExt;
use homarr_common::{is_blocked_ip, validate_outbound_url, HomarrError, HomarrResult};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::redirect::Policy;
use reqwest::{Client, Response};
use url::Url;

use crate::certificates::CertificateManager;

const MAX_REDIRECTS: usize = 5;

/// Resolver that drops blocked addresses at connect time, so a DNS answer cannot change
/// between the pre-flight check and the actual connection (DNS rebinding).
struct BlockedIpFilteringResolver;

impl Resolve for BlockedIpFilteringResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(resolve_unblocked(name.as_str().to_string()))
    }
}

async fn resolve_unblocked(
    host: String,
) -> Result<Addrs, Box<dyn std::error::Error + Send + Sync>> {
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
        .await?
        .filter(|addr| !is_blocked_ip(addr.ip()))
        .collect();
    if addrs.is_empty() {
        return Err(format!("{host} resolves only to blocked addresses").into());
    }
    Ok(Box::new(addrs.into_iter()))
}

/// Clients that trust the uploaded certificates; rebuilt by [`HttpClientFactory::reload`].
#[derive(Debug)]
struct Clients {
    standard: Client,
    no_redirect: Client,
    guarded: Client,
}

impl Clients {
    fn build(certs: &CertificateManager, pins: &[HostPin]) -> HomarrResult<Self> {
        let extra_certs = certs.load_reqwest_certificates();

        let mut standard_builder = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("Homarr/", env!("CARGO_PKG_VERSION")));
        let mut no_redirect_builder = Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(10))
            .redirect(Policy::none())
            .user_agent(concat!("Homarr/", env!("CARGO_PKG_VERSION")));
        let mut guarded_builder = Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(10))
            .redirect(Policy::none())
            // Env proxies resolve DNS themselves and would bypass the connect-time filter.
            .no_proxy()
            .dns_resolver(Arc::new(BlockedIpFilteringResolver))
            .user_agent(concat!("Homarr/", env!("CARGO_PKG_VERSION")));
        if pins.is_empty() {
            for cert in extra_certs {
                standard_builder = standard_builder.add_root_certificate(cert.clone());
                no_redirect_builder = no_redirect_builder.add_root_certificate(cert.clone());
                guarded_builder = guarded_builder.add_root_certificate(cert);
            }
        } else {
            // The preconfigured config carries the uploaded roots itself.
            let tls = pinned_tls_config(certs, pins)?;
            standard_builder = standard_builder.use_preconfigured_tls(tls.clone());
            no_redirect_builder = no_redirect_builder.use_preconfigured_tls(tls.clone());
            guarded_builder = guarded_builder.use_preconfigured_tls(tls);
        }

        let standard = standard_builder
            .build()
            .map_err(|err| HomarrError::Internal(format!("Failed to build HTTP client: {err}")))?;
        let guarded = guarded_builder.build().map_err(|err| {
            HomarrError::Internal(format!("Failed to build guarded HTTP client: {err}"))
        })?;
        let no_redirect = no_redirect_builder.build().map_err(|err| {
            HomarrError::Internal(format!("Failed to build no-redirect HTTP client: {err}"))
        })?;
        Ok(Self {
            standard,
            no_redirect,
            guarded,
        })
    }
}

/// A trusted certificate hostname: the exact leaf certificate (SHA-256) accepted for one host.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HostPin {
    host: String,
    sha256: [u8; 32],
}

impl HostPin {
    fn parse(host: &str, thumbprint: &str) -> Option<Self> {
        let hex: String = thumbprint.chars().filter(|c| *c != ':').collect();
        if hex.len() != 64 || !hex.is_ascii() {
            return None;
        }
        let mut sha256 = [0u8; 32];
        for (i, byte) in sha256.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
        }
        let host = host.trim().trim_matches(['[', ']']).to_ascii_lowercase();
        (!host.is_empty()).then_some(Self { host, sha256 })
    }

    fn matches(&self, host: &str, leaf: &[u8]) -> bool {
        use sha2::{Digest, Sha256};
        self.host == host && Sha256::digest(leaf)[..] == self.sha256[..]
    }
}

/// Accepts a pinned leaf for its hostname, otherwise defers to normal WebPKI verification.
#[derive(Debug)]
struct PinnedHostVerifier {
    pins: Vec<HostPin>,
    inner: Arc<rustls::client::WebPkiServerVerifier>,
}

impl rustls::client::danger::ServerCertVerifier for PinnedHostVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        intermediates: &[rustls::pki_types::CertificateDer<'_>],
        server_name: &rustls::pki_types::ServerName<'_>,
        ocsp_response: &[u8],
        now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        use rustls::pki_types::ServerName;
        let host = match server_name {
            ServerName::DnsName(name) => name.as_ref().to_ascii_lowercase(),
            ServerName::IpAddress(ip) => std::net::IpAddr::from(*ip).to_string(),
            _ => String::new(),
        };
        if self
            .pins
            .iter()
            .any(|pin| pin.matches(&host, end_entity.as_ref()))
        {
            return Ok(rustls::client::danger::ServerCertVerified::assertion());
        }
        self.inner.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        )
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// rustls config mirroring reqwest's defaults (webpki roots + uploaded certificates) plus the pins.
fn pinned_tls_config(
    certs: &CertificateManager,
    pins: &[HostPin],
) -> HomarrResult<rustls::ClientConfig> {
    use rustls::pki_types::{pem::PemObject, CertificateDer};
    fn tls_err(err: impl std::fmt::Display) -> HomarrError {
        HomarrError::Internal(format!("Failed to build TLS config: {err}"))
    }
    let mut roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    for file in certs.list().unwrap_or_default() {
        let ders = CertificateDer::pem_slice_iter(file.content.as_bytes()).filter_map(Result::ok);
        roots.add_parsable_certificates(ders);
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let inner = rustls::client::WebPkiServerVerifier::builder_with_provider(
        Arc::new(roots),
        provider.clone(),
    )
    .build()
    .map_err(tls_err)?;
    let verifier = PinnedHostVerifier {
        pins: pins.to_vec(),
        inner,
    };
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(tls_err)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth())
}

/// Cheap to clone: all clones share the same clients, so [`reload`](Self::reload) reaches
/// every service holding a copy.
#[derive(Debug, Clone)]
pub struct HttpClientFactory {
    clients: Arc<RwLock<Clients>>,
    ping: Client,
    certs: CertificateManager,
    /// Trusted certificate hostnames (pinned leaf thumbprints), shared by all clones.
    pins: Arc<RwLock<Vec<HostPin>>>,
    /// Set by [`restricted`](Self::restricted): every client accessor returns the guarded client.
    restricted: bool,
}

impl HttpClientFactory {
    pub fn new(certs: &CertificateManager) -> HomarrResult<Self> {
        let clients = Clients::build(certs, &[])?;

        let ping = Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .danger_accept_invalid_certs(true)
            // Env proxies would bypass the connect-time resolver filter below.
            .no_proxy()
            // Hostnames resolving to blocked IPs are dropped at connect time; IP-literal and
            // localhost redirect targets never reach the resolver, so the policy checks them.
            .dns_resolver(Arc::new(BlockedIpFilteringResolver))
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() >= 10 {
                    return attempt.error("Too many redirects");
                }
                match validate_outbound_url(attempt.url().as_str(), None) {
                    Ok(_) => attempt.follow(),
                    Err(err) => attempt.error(err.to_string()),
                }
            }))
            .user_agent(concat!("Homarr-Ping/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|err| HomarrError::Internal(format!("Failed to build ping client: {err}")))?;

        Ok(Self {
            clients: Arc::new(RwLock::new(clients)),
            ping,
            certs: certs.clone(),
            pins: Arc::default(),
            restricted: false,
        })
    }

    /// Rebuilds the clients from the certificate directory so uploaded or removed
    /// certificates apply without a restart. In-flight requests keep their old client.
    pub fn reload(&self) -> HomarrResult<()> {
        let pins = self
            .pins
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let fresh = Clients::build(&self.certs, &pins)?;
        *self.clients.write().unwrap_or_else(PoisonError::into_inner) = fresh;
        Ok(())
    }

    /// Applies the trusted certificate hostnames (`hostname`, SHA-256 `thumbprint` as stored by
    /// upstream, e.g. `AB:CD:..`): a server presenting exactly that certificate for that hostname
    /// is trusted even when it is self-signed or issued for another name. Invalid entries are skipped.
    pub fn set_trusted_hostnames<'a>(
        &self,
        entries: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> HomarrResult<()> {
        let pins: Vec<HostPin> = entries
            .into_iter()
            .filter_map(|(host, thumbprint)| HostPin::parse(host, thumbprint))
            .collect();
        *self.pins.write().unwrap_or_else(PoisonError::into_inner) = pins;
        self.reload()
    }

    fn current(&self) -> RwLockReadGuard<'_, Clients> {
        self.clients.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// Copy whose `standard()` and `no_redirect()` hand out the guarded client, for callers
    /// that fetch URLs a non-admin chose.
    pub fn restricted(&self) -> Self {
        Self {
            restricted: true,
            ..self.clone()
        }
    }

    pub fn standard(&self) -> Client {
        if self.restricted {
            return self.guarded();
        }
        self.current().standard.clone()
    }

    pub fn no_redirect(&self) -> Client {
        if self.restricted {
            return self.guarded();
        }
        self.current().no_redirect.clone()
    }

    /// Client for untrusted URLs: no redirects, blocked IPs filtered at connect time.
    pub fn guarded(&self) -> Client {
        self.current().guarded.clone()
    }

    pub fn ping(&self) -> &Client {
        &self.ping
    }

    pub fn certs(&self) -> &CertificateManager {
        &self.certs
    }

    /// Requests a URL with SSRF protection (DNS lookup checked against blocked ranges
    /// unless `trusted_origin` matches) and manual redirect following up to 5 hops.
    pub async fn guarded_get(
        &self,
        initial_url: &str,
        initial_headers: Option<&HashMap<String, String>>,
        trusted_origin: Option<&str>,
        max_bytes: usize,
    ) -> HomarrResult<(Vec<u8>, Option<String>, u16)> {
        let mut current_url = validate_outbound_url(initial_url, trusted_origin)?;
        let mut current_headers = initial_headers.cloned();
        let mut current_trusted = trusted_origin.map(str::to_string);

        for hop in 0..=MAX_REDIRECTS {
            let origin = current_url.origin().ascii_serialization();
            let is_trusted = current_trusted.as_deref() == Some(origin.as_str());
            if !is_trusted {
                verify_dns_targets(&current_url).await?;
            }

            let client = if is_trusted {
                self.no_redirect()
            } else {
                self.guarded()
            };
            let mut req = client.get(current_url.clone());
            if let Some(ref headers) = current_headers {
                for (k, v) in headers {
                    req = req.header(k, v);
                }
            }
            let resp = req.send().await.map_err(|err| {
                HomarrError::Upstream(format!("Request to {origin} failed: {err}"))
            })?;

            let status = resp.status();
            if status.is_redirection() {
                if hop == MAX_REDIRECTS {
                    return Err(HomarrError::Upstream(format!(
                        "Too many redirects starting at {}",
                        current_url.origin().ascii_serialization()
                    )));
                }
                let location = resp
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        HomarrError::Upstream(
                            "Redirect response missing Location header".to_string(),
                        )
                    })?;
                let mut next = current_url
                    .join(location)
                    .map_err(|_| HomarrError::Upstream("Invalid redirect location".to_string()))?;
                let _ = next.set_username("");
                let _ = next.set_password(None);
                let next_origin = next.origin().ascii_serialization();
                if next_origin != origin {
                    current_headers = None;
                    current_trusted = None;
                }
                current_url = validate_outbound_url(next.as_str(), current_trusted.as_deref())?;
                continue;
            }

            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let status_code = status.as_u16();
            if !status.is_success() {
                return Ok((Vec::new(), content_type, status_code));
            }
            let bytes = read_bounded_bytes(resp, max_bytes).await?;
            return Ok((bytes, content_type, status_code));
        }

        Err(HomarrError::Upstream("Too many redirects".to_string()))
    }
}

async fn verify_dns_targets(url: &Url) -> HomarrResult<()> {
    let host = url
        .host_str()
        .ok_or_else(|| HomarrError::BadRequest("URL missing host".to_string()))?;
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|err| HomarrError::Upstream(format!("Failed to resolve {host}: {err}")))?;
    let mut found_any = false;
    for addr in addrs {
        found_any = true;
        if is_blocked_ip(addr.ip()) {
            return Err(HomarrError::Forbidden(format!(
                "Refusing to request {} because it resolves to blocked address {}",
                url.origin().ascii_serialization(),
                addr.ip()
            )));
        }
    }
    if !found_any {
        return Err(HomarrError::Upstream(format!(
            "{host} did not resolve to any address"
        )));
    }
    Ok(())
}

pub async fn read_bounded_bytes(response: Response, max_bytes: usize) -> HomarrResult<Vec<u8>> {
    if let Some(len) = response.content_length() {
        if len as usize > max_bytes {
            return Err(HomarrError::BadRequest(format!(
                "Response exceeded maximum allowed size of {max_bytes} bytes"
            )));
        }
    }
    let mut stream = response.bytes_stream();
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|err| HomarrError::Upstream(format!("Failed reading response body: {err}")))?;
        if buf.len() + chunk.len() > max_bytes {
            return Err(HomarrError::BadRequest(format!(
                "Response exceeded maximum allowed size of {max_bytes} bytes"
            )));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_is_shared_by_clones() {
        let certs = CertificateManager::new("./test-certs-nonexistent".into());
        let factory = HttpClientFactory::new(&certs).unwrap();
        let clone = factory.clone();
        clone.reload().unwrap();
        assert!(Arc::ptr_eq(&factory.clients, &clone.clients));
    }

    #[test]
    fn host_pins_parse_upstream_thumbprints() {
        use sha2::{Digest, Sha256};
        let leaf = b"not really a certificate";
        let thumbprint = Sha256::digest(leaf)
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":");
        let pin = HostPin::parse(" 192.168.1.10 ", &thumbprint).unwrap();
        assert!(pin.matches("192.168.1.10", leaf));
        assert!(!pin.matches("192.168.1.11", leaf));
        assert!(!pin.matches("192.168.1.10", b"other"));
        assert!(HostPin::parse("host", "AB:CD").is_none());

        let certs = CertificateManager::new("./test-certs-nonexistent".into());
        let factory = HttpClientFactory::new(&certs).unwrap();
        factory
            .set_trusted_hostnames([("192.168.1.10", thumbprint.as_str())])
            .unwrap();
        assert_eq!(factory.pins.read().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn restricted_factory_blocks_loopback_hostnames() {
        let certs = CertificateManager::new("./test-certs-nonexistent".into());
        let factory = HttpClientFactory::new(&certs).unwrap().restricted();
        for client in [factory.standard(), factory.no_redirect()] {
            let err = client.get("http://localhost:9/").send().await.unwrap_err();
            let mut chain = err.to_string();
            let mut source = std::error::Error::source(&err);
            while let Some(inner) = source {
                chain.push_str(&format!(": {inner}"));
                source = inner.source();
            }
            assert!(chain.contains("blocked addresses"), "{chain}");
        }
    }
}
