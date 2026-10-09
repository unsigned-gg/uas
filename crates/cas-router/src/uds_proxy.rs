//! The Unix-socket proxy between a jailed worker and the LLM gateway.
//!
//! Spec 0013. The worker speaks HTTP/1.1 to a socket inside its workspace. The
//! proxy accepts one request per connection, checks the method and path against
//! an allowlist, drops every hop-by-hop header and any `Authorization` the worker
//! sent, adds the gateway key, forwards the request over plain TCP to the
//! upstream and streams the response back. The worker never holds the key.

use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpStream, UnixListener, UnixStream};
use tokio::sync::oneshot;

/// Request line plus headers may not exceed this many bytes (spec 0013 D5).
pub const MAX_HEADER_BYTES: usize = 16 * 1024;

/// Paths a worker may call, matched as a prefix followed by end, `/` or `?`.
pub const DEFAULT_ALLOWED_PATHS: &[&str] = &[
    "/v1/chat/completions",
    "/v1/embeddings",
    "/v1/models",
    "/v1/telemetry",
];

/// What the proxy listens on, where it forwards, and what it lets through.
#[derive(Clone)]
pub struct ProxyConfig {
    /// The socket file inside the worker's workspace (`cas_sandbox::ipc::socket_path`).
    pub socket_path: PathBuf,
    /// The gateway, reached over plain TCP.
    pub upstream: SocketAddr,
    /// The `Host` header value sent upstream.
    pub upstream_host: String,
    /// Allowed path prefixes. Defaults to [`DEFAULT_ALLOWED_PATHS`].
    pub allowed_paths: Vec<String>,
    /// The bearer token the proxy adds. `None` forwards without one.
    bearer_token: Option<String>,
}

impl ProxyConfig {
    /// A config with the default allowlist and no token.
    pub fn new(socket_path: PathBuf, upstream: SocketAddr, upstream_host: String) -> Self {
        Self {
            socket_path,
            upstream,
            upstream_host,
            allowed_paths: DEFAULT_ALLOWED_PATHS
                .iter()
                .map(|p| p.to_string())
                .collect(),
            bearer_token: None,
        }
    }

    /// Read the bearer token from a file the worker cannot reach. Leading and
    /// trailing whitespace is trimmed. The value is never logged or printed.
    pub fn with_key_file(mut self, path: &Path) -> Result<Self, ProxyError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|err| ProxyError::KeyFile(path.to_path_buf(), err))?;
        let token = raw.trim().to_string();
        if token.is_empty() {
            return Err(ProxyError::KeyFile(
                path.to_path_buf(),
                io::Error::other("key file is empty"),
            ));
        }
        self.bearer_token = Some(token);
        Ok(self)
    }

    /// Set the bearer token directly (tests, or a caller that already holds it).
    pub fn with_bearer_token(mut self, token: String) -> Self {
        self.bearer_token = Some(token);
        self
    }
}

impl fmt::Debug for ProxyConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxyConfig")
            .field("socket_path", &self.socket_path)
            .field("upstream", &self.upstream)
            .field("upstream_host", &self.upstream_host)
            .field("allowed_paths", &self.allowed_paths)
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Why the proxy could not start.
#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    /// The socket file could not be bound.
    #[error("cannot bind the proxy socket {0}: {1}")]
    Bind(PathBuf, #[source] io::Error),
    /// The key file could not be read, or was empty.
    #[error("cannot read the gateway key file {0}: {1}")]
    KeyFile(PathBuf, #[source] io::Error),
    /// The tokio runtime could not be built.
    #[error("cannot start the proxy runtime: {0}")]
    Runtime(#[source] io::Error),
}

/// A running proxy. Dropping it stops the listener, joins the runtime thread and
/// removes the socket file.
pub struct ProxyHandle {
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    socket_path: PathBuf,
}

impl ProxyHandle {
    /// Bind the socket and serve it on a dedicated runtime thread.
    pub fn start(config: ProxyConfig) -> Result<Self, ProxyError> {
        let socket_path = config.socket_path.clone();
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)
                .map_err(|err| ProxyError::Bind(socket_path.clone(), err))?;
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_io()
            .enable_time()
            .build()
            .map_err(ProxyError::Runtime)?;
        let listener = runtime
            .block_on(async { UnixListener::bind(&socket_path) })
            .map_err(|err| ProxyError::Bind(socket_path.clone(), err))?;
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let config = Arc::new(config);
        let thread = std::thread::Builder::new()
            .name("cas-uds-proxy".into())
            .spawn(move || runtime.block_on(serve(listener, config, shutdown_rx)))
            .map_err(ProxyError::Runtime)?;
        Ok(Self {
            shutdown: Some(shutdown_tx),
            thread: Some(thread),
            socket_path,
        })
    }

    /// The socket file this proxy serves.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Stop serving, join the runtime thread and remove the socket file.
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

impl Drop for ProxyHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn serve(
    listener: UnixListener,
    config: Arc<ProxyConfig>,
    mut shutdown: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    let config = Arc::clone(&config);
                    tokio::spawn(async move { handle_connection(stream, config).await });
                }
                Err(_) => break,
            },
        }
    }
}

/// One request per connection. Every outcome ends with the worker side closed.
async fn handle_connection(stream: UnixStream, config: Arc<ProxyConfig>) {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let request = match read_request(&mut reader).await {
        Ok(request) => request,
        Err(reject) => {
            let _ = write_reject(&mut write_half, reject).await;
            return;
        }
    };
    if let Err(reject) = check_request(&request, &config) {
        let _ = write_reject(&mut write_half, reject).await;
        return;
    }
    if let Err(err) = forward(request, reader, &mut write_half, &config).await {
        let _ = write_reject(&mut write_half, Reject::upstream(err)).await;
    }
}

/// A parsed request head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestHead {
    /// The method, as sent.
    pub method: String,
    /// The request target, as sent (origin form expected).
    pub target: String,
    /// Headers in order, names lowercased.
    pub headers: Vec<(String, String)>,
    /// The body length from `Content-Length`; zero when absent.
    pub content_length: u64,
}

/// A refusal, as an HTTP status and a short reason the worker may see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reject {
    /// The status code written to the worker.
    pub status: u16,
    /// A one-line reason; never contains upstream data or the key.
    pub reason: &'static str,
}

impl Reject {
    const fn new(status: u16, reason: &'static str) -> Self {
        Self { status, reason }
    }

    fn upstream(_err: io::Error) -> Self {
        Self::new(502, "upstream unavailable")
    }
}

async fn read_request<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> Result<RequestHead, Reject> {
    let mut total = 0usize;
    let mut line = String::new();
    let mut request_line: Option<String> = None;
    let mut headers = Vec::new();
    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .await
            .map_err(|_| Reject::new(400, "malformed request"))?;
        if n == 0 {
            return Err(Reject::new(400, "malformed request"));
        }
        total += n;
        if total > MAX_HEADER_BYTES {
            return Err(Reject::new(431, "request header too large"));
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        match request_line {
            None => {
                if trimmed.is_empty() {
                    continue;
                }
                request_line = Some(trimmed.to_string());
            }
            Some(_) => {
                if trimmed.is_empty() {
                    break;
                }
                let (name, value) = trimmed
                    .split_once(':')
                    .ok_or(Reject::new(400, "malformed header"))?;
                headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
            }
        }
    }
    parse_head(request_line.unwrap_or_default(), headers)
}

/// Build a [`RequestHead`] from a request line and lowercased headers.
pub fn parse_head(
    request_line: String,
    headers: Vec<(String, String)>,
) -> Result<RequestHead, Reject> {
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let version = parts.next().unwrap_or_default();
    if method.is_empty() || target.is_empty() || parts.next().is_some() {
        return Err(Reject::new(400, "malformed request line"));
    }
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err(Reject::new(505, "http version not supported"));
    }
    if headers.iter().any(|(name, _)| name == "transfer-encoding") {
        return Err(Reject::new(
            411,
            "chunked requests not supported; send content-length",
        ));
    }
    let content_length = match headers.iter().find(|(name, _)| name == "content-length") {
        Some((_, value)) => value
            .parse::<u64>()
            .map_err(|_| Reject::new(400, "malformed content-length"))?,
        None => 0,
    };
    Ok(RequestHead {
        method,
        target,
        headers,
        content_length,
    })
}

/// The allowlist. Spec 0013 "Protocol filter".
pub fn check_request(request: &RequestHead, config: &ProxyConfig) -> Result<(), Reject> {
    let path = request.target.split('?').next().unwrap_or_default();
    if !path.starts_with('/') {
        return Err(Reject::new(403, "only origin-form targets are allowed"));
    }
    if path.split('/').any(|segment| segment == "..") || path.contains("//") {
        return Err(Reject::new(403, "path traversal is not allowed"));
    }
    let allowed = config.allowed_paths.iter().any(|prefix| {
        path == prefix
            || path
                .strip_prefix(prefix.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    });
    if !allowed {
        return Err(Reject::new(403, "path is not on the allowlist"));
    }
    match request.method.as_str() {
        "POST" => Ok(()),
        "GET" if path == "/v1/models" || path.starts_with("/v1/models/") => Ok(()),
        _ => Err(Reject::new(403, "method is not allowed for this path")),
    }
}

/// Hop-by-hop and identity headers the proxy never forwards from the worker.
const DROPPED_HEADERS: &[&str] = &[
    "authorization",
    "connection",
    "host",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// The head sent upstream: worker headers minus [`DROPPED_HEADERS`], plus
/// `Host`, `Connection: close` and the gateway `Authorization`.
pub fn upstream_head(request: &RequestHead, config: &ProxyConfig) -> Vec<u8> {
    let mut out = format!("{} {} HTTP/1.1\r\n", request.method, request.target).into_bytes();
    for (name, value) in &request.headers {
        if DROPPED_HEADERS.contains(&name.as_str()) {
            continue;
        }
        out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    out.extend_from_slice(format!("host: {}\r\n", config.upstream_host).as_bytes());
    out.extend_from_slice(b"connection: close\r\n");
    if let Some(token) = &config.bearer_token {
        out.extend_from_slice(format!("authorization: Bearer {token}\r\n").as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out
}

async fn forward<R>(
    request: RequestHead,
    reader: R,
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    config: &ProxyConfig,
) -> io::Result<()>
where
    R: AsyncReadExt + Unpin,
{
    let mut upstream = TcpStream::connect(config.upstream).await?;
    upstream.write_all(&upstream_head(&request, config)).await?;
    if request.content_length > 0 {
        let mut body = reader.take(request.content_length);
        tokio::io::copy(&mut body, &mut upstream).await?;
    }
    upstream.shutdown().await?;
    tokio::io::copy(&mut upstream, writer).await?;
    writer.shutdown().await
}

async fn write_reject<W: AsyncWriteExt + Unpin>(writer: &mut W, reject: Reject) -> io::Result<()> {
    let body = format!("{}\n", reject.reason);
    let head = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        reject.status,
        reason_phrase(reject.status),
        body.len()
    );
    writer.write_all(head.as_bytes()).await?;
    writer.write_all(body.as_bytes()).await?;
    writer.shutdown().await
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        403 => "Forbidden",
        411 => "Length Required",
        431 => "Request Header Fields Too Large",
        502 => "Bad Gateway",
        505 => "HTTP Version Not Supported",
        _ => "Error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ProxyConfig {
        ProxyConfig::new(
            PathBuf::from("/tmp/unused.sock"),
            "127.0.0.1:4000".parse().expect("addr"),
            "llm.llm:4000".to_string(),
        )
        .with_bearer_token("secret".to_string())
    }

    fn head(method: &str, target: &str) -> RequestHead {
        parse_head(format!("{method} {target} HTTP/1.1"), Vec::new()).expect("head")
    }

    #[test]
    fn allowlist_accepts_the_gateway_paths() {
        let config = config();
        assert!(check_request(&head("POST", "/v1/chat/completions"), &config).is_ok());
        assert!(check_request(&head("POST", "/v1/embeddings"), &config).is_ok());
        assert!(check_request(&head("GET", "/v1/models"), &config).is_ok());
        assert!(check_request(&head("GET", "/v1/models/glm-5.2"), &config).is_ok());
        assert!(check_request(&head("POST", "/v1/telemetry?x=1"), &config).is_ok());
    }

    #[test]
    fn allowlist_rejects_everything_else() {
        let config = config();
        for (method, target) in [
            ("CONNECT", "llm.unsigned.gg:443"),
            ("GET", "/admin/keys"),
            ("GET", "/etc/passwd"),
            ("POST", "/v1/chat/completions/../../admin"),
            ("POST", "/v1/chat/completionsx"),
            ("PUT", "/v1/models"),
            ("DELETE", "/v1/chat/completions"),
            ("GET", "http://llm.llm:4000/v1/models"),
            ("GET", "/v1//models"),
        ] {
            let reject = check_request(&head(method, target), &config).expect_err(target);
            assert_eq!(reject.status, 403, "{method} {target}");
        }
    }

    #[test]
    fn parse_head_refuses_chunked_and_bad_lengths() {
        let chunked = parse_head(
            "POST /v1/embeddings HTTP/1.1".into(),
            vec![("transfer-encoding".into(), "chunked".into())],
        );
        assert_eq!(chunked.expect_err("chunked").status, 411);
        let bad = parse_head(
            "POST /v1/embeddings HTTP/1.1".into(),
            vec![("content-length".into(), "many".into())],
        );
        assert_eq!(bad.expect_err("bad length").status, 400);
        assert_eq!(
            parse_head("GET /v1/models HTTP/2".into(), Vec::new())
                .expect_err("version")
                .status,
            505
        );
    }

    #[test]
    fn upstream_head_replaces_identity_and_hop_headers() {
        let config = config();
        let request = parse_head(
            "POST /v1/chat/completions HTTP/1.1".into(),
            vec![
                ("host".into(), "worker.invalid".into()),
                ("authorization".into(), "Bearer worker-key".into()),
                ("connection".into(), "keep-alive".into()),
                ("content-type".into(), "application/json".into()),
                ("content-length".into(), "2".into()),
            ],
        )
        .expect("head");
        let head = String::from_utf8(upstream_head(&request, &config)).expect("utf8");
        assert!(head.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
        assert!(head.contains("content-type: application/json\r\n"));
        assert!(head.contains("content-length: 2\r\n"));
        assert!(head.contains("host: llm.llm:4000\r\n"));
        assert!(head.contains("connection: close\r\n"));
        assert!(head.contains("authorization: Bearer secret\r\n"));
        assert!(!head.contains("worker-key"));
        assert!(!head.contains("worker.invalid"));
        assert!(!head.contains("keep-alive"));
    }

    #[test]
    fn debug_never_prints_the_token() {
        let text = format!("{:?}", config());
        assert!(text.contains("<redacted>"));
        assert!(!text.contains("secret"));
    }
}
