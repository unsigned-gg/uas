//! The Unix-socket proxy between a jailed worker and the LLM gateway.
//!
//! Spec 0013. The worker speaks HTTP/1.1 to a socket inside its workspace. The
//! proxy accepts one request per connection, validates the head byte by byte,
//! checks the method and path against an allowlist, drops every hop-by-hop
//! header and any `Authorization` the worker sent, adds the gateway key,
//! forwards the request over plain TCP to the upstream and streams the
//! response back. The worker never holds the key. Every read has a bound and
//! a timeout, and the number of live connections is capped.

use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpStream, UnixListener, UnixStream};
use tokio::sync::{oneshot, Semaphore};

/// Request line plus headers may not exceed this many bytes (spec 0013 D6).
pub const MAX_HEADER_BYTES: usize = 16 * 1024;

/// `POST` targets a worker may call, matched exactly (query string excluded).
pub const DEFAULT_POST_PATHS: &[&str] =
    &["/v1/chat/completions", "/v1/embeddings", "/v1/telemetry"];

/// `GET` targets a worker may call: the prefix itself or one more path segment.
pub const DEFAULT_GET_PREFIXES: &[&str] = &["/v1/models"];

/// Spec 0013 D8 defaults.
pub const DEFAULT_MAX_CONNECTIONS: usize = 64;
/// Spec 0013 D8 defaults.
pub const DEFAULT_MAX_BODY_BYTES: u64 = 16 * 1024 * 1024;
/// Spec 0013 D8 defaults.
pub const DEFAULT_HEAD_TIMEOUT: Duration = Duration::from_secs(10);
/// Spec 0013 D8 defaults.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// Spec 0013 D8 defaults.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// What the proxy listens on, where it forwards, what it lets through, and its limits.
#[derive(Clone)]
pub struct ProxyConfig {
    /// The socket file inside the worker's workspace (`cas_sandbox::ipc::socket_path`).
    pub socket_path: PathBuf,
    /// The gateway, reached over plain TCP.
    pub upstream: SocketAddr,
    /// The `Host` header value sent upstream.
    pub upstream_host: String,
    /// Exact `POST` paths. Defaults to [`DEFAULT_POST_PATHS`].
    pub post_paths: Vec<String>,
    /// `GET` prefixes, each allowing itself and one more segment. Defaults to [`DEFAULT_GET_PREFIXES`].
    pub get_prefixes: Vec<String>,
    /// Live connections beyond this get `503` at once.
    pub max_connections: usize,
    /// A declared body above this gets `413`.
    pub max_body_bytes: u64,
    /// The head must arrive within this, else `408`.
    pub head_timeout: Duration,
    /// The whole exchange must finish within this, else both sides are closed.
    pub request_timeout: Duration,
    /// The upstream TCP connect must finish within this, else `502`.
    pub connect_timeout: Duration,
    bearer_token: Option<String>,
}

impl ProxyConfig {
    /// A config with the default allowlist, the D8 limits and no token.
    pub fn new(socket_path: PathBuf, upstream: SocketAddr, upstream_host: String) -> Self {
        Self {
            socket_path,
            upstream,
            upstream_host,
            post_paths: DEFAULT_POST_PATHS.iter().map(|p| p.to_string()).collect(),
            get_prefixes: DEFAULT_GET_PREFIXES.iter().map(|p| p.to_string()).collect(),
            max_connections: DEFAULT_MAX_CONNECTIONS,
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
            head_timeout: DEFAULT_HEAD_TIMEOUT,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            bearer_token: None,
        }
    }

    /// Read the bearer token from a file the worker cannot reach. Leading and
    /// trailing whitespace is trimmed. The value is never logged or printed.
    pub fn with_key_file(mut self, path: &Path) -> Result<Self, ProxyError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|err| ProxyError::KeyFile(path.to_path_buf(), err))?;
        let token = raw.trim().to_string();
        if token.is_empty() || !token.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(ProxyError::KeyFile(
                path.to_path_buf(),
                io::Error::other("key file is empty or not a single printable token"),
            ));
        }
        self.bearer_token = Some(token);
        Ok(self)
    }

    /// Set the bearer token directly, for a caller that already holds it or a test.
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
            .field("post_paths", &self.post_paths)
            .field("get_prefixes", &self.get_prefixes)
            .field("max_connections", &self.max_connections)
            .field("max_body_bytes", &self.max_body_bytes)
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

    /// Stop serving, join the runtime thread and remove the socket file. Live
    /// connections are cut when the runtime drops.
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
    let permits = Arc::new(Semaphore::new(config.max_connections.max(1)));
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    let config = Arc::clone(&config);
                    match Arc::clone(&permits).try_acquire_owned() {
                        Ok(permit) => {
                            tokio::spawn(async move {
                                let _permit = permit;
                                let budget = config.request_timeout;
                                // The whole exchange has one budget; on expiry both
                                // sides are dropped, which closes them.
                                let _ = tokio::time::timeout(budget, handle_connection(stream, config)).await;
                            });
                        }
                        Err(_) => {
                            tokio::spawn(async move {
                                let (_, mut write_half) = stream.into_split();
                                let _ = write_reject(&mut write_half, Reject::new(503, "too many connections")).await;
                            });
                        }
                    }
                }
                // A transient accept error (EMFILE, ECONNABORTED) must not end the
                // listener. Back off briefly and keep serving.
                Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
            },
        }
    }
}

/// One request per connection. Every outcome ends with the worker side closed.
async fn handle_connection(stream: UnixStream, config: Arc<ProxyConfig>) {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let head = match tokio::time::timeout(config.head_timeout, read_head(&mut reader)).await {
        Ok(Ok(head)) => head,
        Ok(Err(reject)) => {
            let _ = write_reject(&mut write_half, reject).await;
            return;
        }
        Err(_) => {
            let _ = write_reject(&mut write_half, Reject::new(408, "request timeout")).await;
            return;
        }
    };
    if let Err(reject) = check_request(&head, &config) {
        let _ = write_reject(&mut write_half, reject).await;
        return;
    }
    match forward(head, &mut reader, &mut write_half, &config).await {
        Ok(()) => {}
        Err(Forwarded::Reject(reject)) => {
            let _ = write_reject(&mut write_half, reject).await;
        }
        // Part of a response already reached the worker: no second response.
        Err(Forwarded::Cut) => {}
    }
}

/// A validated request head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestHead {
    /// The method, as sent.
    pub method: String,
    /// The request target, as sent (origin form, validated).
    pub target: String,
    /// Headers in order, names lowercased, `content-length` removed.
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
}

enum Forwarded {
    Reject(Reject),
    Cut,
}

/// Read the head with a hard byte bound: the reader is capped at
/// `MAX_HEADER_BYTES + 1`, so a line without a newline cannot grow memory.
async fn read_head<R>(reader: &mut R) -> Result<RequestHead, Reject>
where
    R: AsyncBufRead + Unpin,
{
    let mut limited = reader.take(MAX_HEADER_BYTES as u64 + 1);
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut lines: Vec<Vec<u8>> = Vec::new();
    loop {
        let start = buf.len();
        let n = limited
            .read_until(b'\n', &mut buf)
            .await
            .map_err(|_| Reject::new(400, "malformed request"))?;
        if buf.len() > MAX_HEADER_BYTES {
            return Err(Reject::new(431, "request header too large"));
        }
        if n == 0 {
            return Err(Reject::new(400, "malformed request"));
        }
        let line = strip_crlf(&buf[start..])?;
        if line.is_empty() {
            if lines.is_empty() {
                continue; // tolerate leading empty lines
            }
            break;
        }
        lines.push(line.to_vec());
    }
    parse_head(lines)
}

/// A line must end in `\n`, optionally preceded by exactly one `\r`; no other
/// control bytes, and nothing above ASCII, anywhere.
fn strip_crlf(line: &[u8]) -> Result<&[u8], Reject> {
    let Some(mut body) = line.strip_suffix(b"\n") else {
        return Err(Reject::new(400, "malformed request"));
    };
    if let Some(stripped) = body.strip_suffix(b"\r") {
        body = stripped;
    }
    if body
        .iter()
        .any(|&b| b == b'\r' || b == b'\n' || b == 0 || !b.is_ascii())
    {
        return Err(Reject::new(400, "control bytes in request"));
    }
    Ok(body)
}

fn is_tchar(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// Build a validated [`RequestHead`] from raw head lines (request line first).
pub fn parse_head(lines: Vec<Vec<u8>>) -> Result<RequestHead, Reject> {
    let mut iter = lines.into_iter();
    let request_line = iter.next().ok_or(Reject::new(400, "malformed request"))?;
    if request_line.iter().any(|&b| b < 0x21 && b != b' ') {
        return Err(Reject::new(400, "control bytes in request line"));
    }
    let request_line = String::from_utf8_lossy(&request_line).into_owned();
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let version = parts.next().unwrap_or_default();
    if method.is_empty()
        || target.is_empty()
        || parts.next().is_some()
        || !method.bytes().all(is_tchar)
    {
        return Err(Reject::new(400, "malformed request line"));
    }
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err(Reject::new(505, "http version not supported"));
    }
    let mut headers = Vec::new();
    let mut content_length: Option<u64> = None;
    for line in iter {
        if line.first().is_some_and(|&b| b == b' ' || b == b'\t') {
            return Err(Reject::new(400, "obsolete line folding is not allowed"));
        }
        let colon = line
            .iter()
            .position(|&b| b == b':')
            .ok_or(Reject::new(400, "malformed header"))?;
        let (name, value) = line.split_at(colon);
        let value = &value[1..];
        if name.is_empty() || !name.iter().all(|&b| is_tchar(b)) {
            return Err(Reject::new(400, "malformed header name"));
        }
        if value.iter().any(|&b| b < 0x20 && b != b'\t') {
            return Err(Reject::new(400, "control bytes in header value"));
        }
        let name = String::from_utf8_lossy(name).to_ascii_lowercase();
        let value = String::from_utf8_lossy(value).trim().to_string();
        match name.as_str() {
            "transfer-encoding" => {
                return Err(Reject::new(
                    411,
                    "chunked requests not supported; send content-length",
                ));
            }
            "content-length" => {
                let parsed = (value.len() <= 19
                    && !value.is_empty()
                    && value.bytes().all(|b| b.is_ascii_digit()))
                .then(|| value.parse::<u64>().ok())
                .flatten()
                .ok_or(Reject::new(400, "malformed content-length"))?;
                match content_length {
                    Some(seen) if seen != parsed => {
                        return Err(Reject::new(400, "conflicting content-length headers"));
                    }
                    _ => content_length = Some(parsed),
                }
            }
            _ => headers.push((name, value)),
        }
    }
    Ok(RequestHead {
        method,
        target,
        headers,
        content_length: content_length.unwrap_or(0),
    })
}

/// The allowlist and the limits. Spec 0013 D4, D6, D8.
pub fn check_request(request: &RequestHead, config: &ProxyConfig) -> Result<(), Reject> {
    let target = request.target.as_str();
    if !target.starts_with('/') {
        return Err(Reject::new(403, "only origin-form targets are allowed"));
    }
    if !target
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/?%".contains(&b))
    {
        return Err(Reject::new(403, "characters not allowed in target"));
    }
    // Percent-encoded dots, slashes and backslashes could decode into traversal
    // upstream; refuse them rather than guess the gateway's decoder.
    let lower = target.to_ascii_lowercase();
    if ["%2e", "%2f", "%5c"].iter().any(|seq| lower.contains(seq)) {
        return Err(Reject::new(403, "encoded path characters are not allowed"));
    }
    let mut bytes = target.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let ok = bytes.next().is_some_and(|c| c.is_ascii_hexdigit())
                && bytes.next().is_some_and(|c| c.is_ascii_hexdigit());
            if !ok {
                return Err(Reject::new(403, "malformed percent-encoding"));
            }
        }
    }
    let path = target.split('?').next().unwrap_or_default();
    if path.split('/').any(|segment| segment == "..") || path.contains("//") {
        return Err(Reject::new(403, "path traversal is not allowed"));
    }
    match request.method.as_str() {
        "POST" if config.post_paths.iter().any(|p| p == path) => Ok(()),
        "GET"
            if config.get_prefixes.iter().any(|prefix| {
                path == prefix
                    || path
                        .strip_prefix(prefix.as_str())
                        .and_then(|rest| rest.strip_prefix('/'))
                        .is_some_and(|segment| !segment.is_empty() && !segment.contains('/'))
            }) =>
        {
            Ok(())
        }
        "POST" | "GET" => Err(Reject::new(403, "path is not on the allowlist")),
        _ => Err(Reject::new(403, "method is not allowed")),
    }
    .and_then(|()| {
        if request.content_length > config.max_body_bytes {
            Err(Reject::new(413, "request body too large"))
        } else {
            Ok(())
        }
    })
}

/// Hop-by-hop and identity headers the proxy never forwards from the worker.
/// `content-length` is removed at parse time and re-emitted normalized.
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

/// The head sent upstream: worker headers minus [`DROPPED_HEADERS`] and every
/// `x-litellm-*` header, plus `Host`, `Connection: close`, a normalized
/// `Content-Length` and the gateway `Authorization`.
pub fn upstream_head(request: &RequestHead, config: &ProxyConfig) -> Vec<u8> {
    let mut out = format!("{} {} HTTP/1.1\r\n", request.method, request.target).into_bytes();
    for (name, value) in &request.headers {
        if DROPPED_HEADERS.contains(&name.as_str()) || name.starts_with("x-litellm-") {
            continue;
        }
        out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    out.extend_from_slice(format!("host: {}\r\n", config.upstream_host).as_bytes());
    out.extend_from_slice(b"connection: close\r\n");
    out.extend_from_slice(format!("content-length: {}\r\n", request.content_length).as_bytes());
    if let Some(token) = &config.bearer_token {
        out.extend_from_slice(format!("authorization: Bearer {token}\r\n").as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out
}

async fn forward<R>(
    request: RequestHead,
    reader: &mut R,
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    config: &ProxyConfig,
) -> Result<(), Forwarded>
where
    R: AsyncRead + Unpin,
{
    let unavailable = || Forwarded::Reject(Reject::new(502, "upstream unavailable"));
    let mut upstream =
        tokio::time::timeout(config.connect_timeout, TcpStream::connect(config.upstream))
            .await
            .map_err(|_| unavailable())?
            .map_err(|_| unavailable())?;
    upstream
        .write_all(&upstream_head(&request, config))
        .await
        .map_err(|_| unavailable())?;
    if request.content_length > 0 {
        let mut body = reader.take(request.content_length);
        let copied = tokio::io::copy(&mut body, &mut upstream)
            .await
            .map_err(|_| Forwarded::Reject(Reject::new(400, "request body was cut short")))?;
        if copied != request.content_length {
            return Err(Forwarded::Reject(Reject::new(
                400,
                "request body was cut short",
            )));
        }
    }
    // No half-close toward the upstream: Content-Length and Connection: close
    // already delimit the request, and some servers treat EOF as a disconnect.
    let mut chunk = [0u8; 16 * 1024];
    let mut wrote_any = false;
    loop {
        let n = match upstream.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) if wrote_any => return Err(Forwarded::Cut),
            Err(_) => return Err(unavailable()),
        };
        if writer.write_all(&chunk[..n]).await.is_err() {
            return Err(Forwarded::Cut);
        }
        wrote_any = true;
    }
    if !wrote_any {
        return Err(unavailable());
    }
    let _ = writer.shutdown().await;
    Ok(())
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
        408 => "Request Timeout",
        411 => "Length Required",
        413 => "Content Too Large",
        431 => "Request Header Fields Too Large",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
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

    fn lines(text: &str) -> Vec<Vec<u8>> {
        text.lines().map(|l| l.as_bytes().to_vec()).collect()
    }

    fn head(method: &str, target: &str) -> RequestHead {
        parse_head(lines(&format!("{method} {target} HTTP/1.1"))).expect("head")
    }

    #[test]
    fn allowlist_accepts_the_gateway_paths() {
        let config = config();
        assert!(check_request(&head("POST", "/v1/chat/completions"), &config).is_ok());
        assert!(check_request(&head("POST", "/v1/embeddings"), &config).is_ok());
        assert!(check_request(&head("POST", "/v1/telemetry?x=1"), &config).is_ok());
        assert!(check_request(&head("GET", "/v1/models"), &config).is_ok());
        assert!(check_request(&head("GET", "/v1/models?x=1"), &config).is_ok());
        assert!(check_request(&head("GET", "/v1/models/glm-5.2"), &config).is_ok());
    }

    #[test]
    fn allowlist_rejects_everything_else() {
        let config = config();
        for (method, target) in [
            ("CONNECT", "llm.unsigned.gg:443"),
            ("GET", "/admin/keys"),
            ("GET", "/etc/passwd"),
            ("POST", "/v1/chat/completions/../../admin"),
            ("POST", "/v1/chat/completions/extra"),
            ("POST", "/v1/chat/completionsx"),
            ("POST", "/v1/models"),
            ("GET", "/v1/models/a/b"),
            ("PUT", "/v1/models"),
            ("DELETE", "/v1/chat/completions"),
            ("GET", "http://llm.llm:4000/v1/models"),
            ("GET", "/v1//models"),
            ("GET", "/v1/models/%2e%2e/x"),
            ("GET", "/v1/models/%2E%2E"),
            ("GET", "/v1/models%2fx"),
            ("GET", "/v1/models/a%5cb"),
            ("GET", "/v1/models/%zz"),
            ("GET", "/v1/models/a\\b"),
        ] {
            let reject = check_request(&head(method, target), &config).expect_err(target);
            assert_eq!(reject.status, 403, "{method} {target}");
        }
    }

    #[test]
    fn body_above_the_cap_is_413() {
        let config = config();
        let request = parse_head(lines(&format!(
            "POST /v1/embeddings HTTP/1.1\ncontent-length: {}",
            DEFAULT_MAX_BODY_BYTES + 1
        )))
        .expect("head");
        assert_eq!(
            check_request(&request, &config).expect_err("cap").status,
            413
        );
    }

    #[test]
    fn parse_head_refuses_chunked_and_bad_lengths() {
        let chunked = parse_head(lines(
            "POST /v1/embeddings HTTP/1.1\ntransfer-encoding: chunked",
        ));
        assert_eq!(chunked.expect_err("chunked").status, 411);
        for value in ["many", "+5", "-1", "", "99999999999999999999", "5 6"] {
            let bad = parse_head(lines(&format!(
                "POST /v1/embeddings HTTP/1.1\ncontent-length:{value}"
            )));
            assert_eq!(
                bad.expect_err(value).status,
                400,
                "content-length {value:?}"
            );
        }
        let dup = parse_head(lines(
            "POST /v1/embeddings HTTP/1.1\ncontent-length: 5\ncontent-length: 6",
        ));
        assert_eq!(dup.expect_err("dup").status, 400);
        let same = parse_head(lines(
            "POST /v1/embeddings HTTP/1.1\ncontent-length: 5\ncontent-length: 5",
        ))
        .expect("same");
        assert_eq!(same.content_length, 5);
        assert!(
            same.headers.is_empty(),
            "content-length is normalized, not forwarded"
        );
        assert_eq!(
            parse_head(lines("GET /v1/models HTTP/2"))
                .expect_err("version")
                .status,
            505
        );
    }

    #[test]
    fn parse_head_refuses_bad_names_folding_and_control_bytes() {
        for text in [
            "GET /v1/models HTTP/1.1\nx y: 1",
            "GET /v1/models HTTP/1.1\n: 1",
            "GET /v1/models HTTP/1.1\nno-colon",
            "GET /v1/models HTTP/1.1\nx: a\n b",
            "GET /v1/models HTTP/1.1\nx: a\u{1}b",
            "GET /v1/models\tHTTP/1.1",
            "G\u{1}T /v1/models HTTP/1.1",
        ] {
            let reject = parse_head(lines(text)).expect_err(text);
            assert_eq!(reject.status, 400, "{text:?}");
        }
    }

    #[test]
    fn strip_crlf_rejects_bare_cr_and_non_ascii() {
        assert_eq!(strip_crlf(b"abc\r\n").expect("crlf"), b"abc");
        assert_eq!(strip_crlf(b"abc\n").expect("lf"), b"abc");
        assert!(strip_crlf(b"a\rb\n").is_err());
        assert!(strip_crlf(b"abc").is_err());
        assert!(strip_crlf("é\n".as_bytes()).is_err());
    }

    #[test]
    fn upstream_head_replaces_identity_and_hop_headers() {
        let config = config();
        let request = parse_head(lines(
            "POST /v1/chat/completions HTTP/1.1\nhost: worker.invalid\nauthorization: Bearer worker-key\nconnection: keep-alive\nx-litellm-api-key: leak\ncontent-type: application/json\ncontent-length: 2",
        ))
        .expect("head");
        let head = String::from_utf8(upstream_head(&request, &config)).expect("utf8");
        assert!(head.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
        assert!(head.contains("content-type: application/json\r\n"));
        assert_eq!(head.matches("content-length:").count(), 1);
        assert!(head.contains("content-length: 2\r\n"));
        assert!(head.contains("host: llm.llm:4000\r\n"));
        assert!(head.contains("connection: close\r\n"));
        assert!(head.contains("authorization: Bearer secret\r\n"));
        for gone in [
            "worker-key",
            "worker.invalid",
            "keep-alive",
            "x-litellm",
            "leak",
        ] {
            assert!(!head.contains(gone), "{gone} forwarded");
        }
    }

    #[test]
    fn debug_never_prints_the_token() {
        let text = format!("{:?}", config());
        assert!(text.contains("<redacted>"));
        assert!(!text.contains("secret"));
    }

    #[tokio::test]
    async fn read_head_bounds_a_line_without_newline() {
        let huge = vec![b'a'; MAX_HEADER_BYTES * 4];
        let mut reader = BufReader::new(std::io::Cursor::new(huge));
        let reject = read_head(&mut reader).await.expect_err("bounded");
        assert_eq!(reject.status, 431);
    }

    #[tokio::test]
    async fn read_head_leaves_the_body_in_the_reader() {
        let text = b"POST /v1/embeddings HTTP/1.1\r\ncontent-length: 4\r\n\r\nbody".to_vec();
        let mut reader = BufReader::new(std::io::Cursor::new(text));
        let head = read_head(&mut reader).await.expect("head");
        assert_eq!(head.content_length, 4);
        let mut rest = Vec::new();
        reader.read_to_end(&mut rest).await.expect("rest");
        assert_eq!(rest, b"body");
    }
}
