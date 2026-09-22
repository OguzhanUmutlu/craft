use crate::supervisor::Supervisor;
use crate::telemetry::generate_prometheus_metrics;
use craft_core::{CraftPaths, Result};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::io::Cursor;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

/// Constant-time token verification to mitigate timing attacks
pub fn verify_token(provided: Option<&str>, configured: Option<&str>) -> bool {
    match (provided, configured) {
        (_, None) => true, // If no token is configured, open access
        (None, Some(_)) => false,
        (Some(prov), Some(conf)) => {
            if prov.len() != conf.len() {
                return false;
            }
            let mut diff = 0u8;
            for (a, b) in prov.bytes().zip(conf.bytes()) {
                diff |= a ^ b;
            }
            diff == 0
        }
    }
}

/// Simple sliding-window rate limiter per client IP address
#[derive(Debug, Default)]
pub struct SlidingWindowRateLimiter {
    attempts: HashMap<IpAddr, Vec<Instant>>,
}

impl SlidingWindowRateLimiter {
    pub fn new() -> Self {
        Self {
            attempts: HashMap::new(),
        }
    }

    pub fn check_and_record(&mut self, ip: IpAddr, max_per_minute: usize) -> bool {
        let now = Instant::now();
        let cutoff = now.checked_sub(Duration::from_secs(60)).unwrap_or(now);

        let window = self.attempts.entry(ip).or_default();
        window.retain(|t| *t > cutoff);

        if window.len() >= max_per_minute {
            false
        } else {
            window.push(now);
            true
        }
    }
}

/// A wrapper stream that drains any prefetched buffer bytes before delegating to inner TCP stream
pub struct PrefixedStream {
    prefix: Cursor<Vec<u8>>,
    stream: TcpStream,
}

impl PrefixedStream {
    pub fn new(prefix: Vec<u8>, stream: TcpStream) -> Self {
        Self {
            prefix: Cursor::new(prefix),
            stream,
        }
    }
}

impl AsyncRead for PrefixedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let pos = self.prefix.position() as usize;
        let prefix_len = self.prefix.get_ref().len();

        if pos < prefix_len {
            let to_read = (prefix_len - pos).min(buf.remaining());
            buf.put_slice(&self.prefix.get_ref()[pos..pos + to_read]);
            self.prefix.set_position((pos + to_read) as u64);
            Poll::Ready(Ok(()))
        } else {
            Pin::new(&mut self.stream).poll_read(cx, buf)
        }
    }
}

impl AsyncWrite for PrefixedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

#[derive(Clone)]
pub struct GatewayServer {
    bind: String,
    port: u16,
    token: Option<String>,
    supervisor: Supervisor,
    paths: CraftPaths,
    rate_limiter: Arc<Mutex<SlidingWindowRateLimiter>>,
}

impl GatewayServer {
    pub fn new(
        bind: impl Into<String>,
        port: u16,
        token: Option<String>,
        supervisor: Supervisor,
        paths: CraftPaths,
    ) -> Self {
        Self {
            bind: bind.into(),
            port,
            token,
            supervisor,
            paths,
            rate_limiter: Arc::new(Mutex::new(SlidingWindowRateLimiter::new())),
        }
    }

    pub fn start(&self) -> Result<tokio::task::JoinHandle<()>> {
        let server = self.clone();
        let addr = format!("{}:{}", self.bind, self.port);

        let handle = tokio::spawn(async move {
            let listener = match TcpListener::bind(&addr).await {
                Ok(l) => {
                    info!("Gateway & Telemetry server listening on http://{}", addr);
                    l
                }
                Err(e) => {
                    error!("Failed to bind Gateway listener on {}: {}", addr, e);
                    return;
                }
            };

            loop {
                match listener.accept().await {
                    Ok((stream, peer)) => {
                        let s = server.clone();
                        tokio::spawn(async move {
                            s.handle_connection(stream, peer).await;
                        });
                    }
                    Err(e) => {
                        warn!("Gateway listener accept error: {}", e);
                    }
                }
            }
        });

        Ok(handle)
    }

    async fn handle_connection(&self, mut stream: TcpStream, peer: SocketAddr) {
        // Enforce basic rate limit on incoming connections
        {
            let mut limiter = self.rate_limiter.lock().await;
            if !limiter.check_and_record(peer.ip(), 120) {
                let resp = "HTTP/1.1 429 Too Many Requests\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nRate limit exceeded. Try again later.\n";
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
                return;
            }
        }

        // Read initial HTTP request header
        let mut buffer = Vec::with_capacity(1024);
        let mut temp = [0u8; 1024];

        loop {
            match tokio::io::AsyncReadExt::read(&mut stream, &mut temp).await {
                Ok(0) => return, // Connection closed
                Ok(n) => {
                    buffer.extend_from_slice(&temp[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") || buffer.len() > 8192 {
                        break;
                    }
                }
                Err(_) => return,
            }
        }

        let header_str = String::from_utf8_lossy(&buffer);
        let mut lines = header_str.lines();
        let request_line = match lines.next() {
            Some(l) => l,
            None => return,
        };

        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("GET");
        let raw_uri = parts.next().unwrap_or("/");

        let (path, query) = match raw_uri.split_once('?') {
            Some((p, q)) => (p, Some(q)),
            None => (raw_uri, None),
        };

        // Extract query parameters
        let mut query_params = HashMap::new();
        if let Some(q) = query {
            for pair in q.split('&') {
                if let Some((k, v)) = pair.split_once('=') {
                    query_params.insert(k.to_string(), v.to_string());
                }
            }
        }

        // Extract Authorization header
        let mut auth_token = query_params.get("token").cloned();
        for line in lines {
            if line.to_lowercase().starts_with("authorization:") {
                let val = line[14..].trim();
                if val.to_lowercase().starts_with("bearer ") {
                    auth_token = Some(val[7..].trim().to_string());
                }
            }
        }

        // Route: GET /metrics
        if method == "GET" && path == "/metrics" {
            if !verify_token(auth_token.as_deref(), self.token.as_deref()) {
                let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n401 Unauthorized: Valid gateway token required.\n";
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
                return;
            }

            let metrics = generate_prometheus_metrics(&self.supervisor, &self.paths).await;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                metrics.len(),
                metrics
            );
            let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
            return;
        }

        // Route: GET /health or GET /
        if method == "GET" && (path == "/health" || path == "/") {
            let uptime = crate::telemetry::get_daemon_uptime_seconds();
            let json = serde_json::json!({
                "status": "ok",
                "version": craft_core::CRAFT_VERSION,
                "uptime_seconds": uptime,
                "gateway_port": self.port,
            });
            let body = json.to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
            return;
        }

        // Route: GET /ws/console
        if method == "GET" && path == "/ws/console" {
            if !verify_token(auth_token.as_deref(), self.token.as_deref()) {
                let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n401 Unauthorized: Valid gateway token required.\n";
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
                return;
            }

            let server_name = match query_params.get("server") {
                Some(s) => s.clone(),
                None => {
                    let resp = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nMissing required 'server' query parameter.\n";
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
                    return;
                }
            };

            let server_path = match self.paths.resolve_server_path(None, Some(&server_name), true) {
                Ok(p) => p,
                Err(e) => {
                    let resp = format!("HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nServer not found: {}\n", e);
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
                    return;
                }
            };

            if !self.supervisor.is_running(&server_path).await {
                let resp = format!("HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nServer '{}' is not currently running.\n", server_name);
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
                return;
            }

            let mut log_rx = match self.supervisor.get_server_log_broadcaster(&server_path).await {
                Some(rx) => rx,
                None => {
                    let resp = "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nFailed to acquire server log broadcaster.\n";
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
                    return;
                }
            };

            // Upgrade to WebSocket
            let prefixed = PrefixedStream::new(buffer, stream);
            let ws_stream = match tokio_tungstenite::accept_hdr_async(
                prefixed,
                |_req: &Request, resp: Response| Ok(resp),
            ).await {
                Ok(ws) => ws,
                Err(e) => {
                    warn!("WebSocket upgrade handshake failed: {}", e);
                    return;
                }
            };

            info!("WebSocket console client connected for server '{}' from {}", server_name, peer);

            let (mut ws_sender, mut ws_receiver) = ws_stream.split();
            let supervisor = self.supervisor.clone();
            let s_path = server_path.clone();

            // Task 1: forward incoming server log lines to client WebSocket
            let send_task = tokio::spawn(async move {
                while let Ok(line) = log_rx.recv().await {
                    let msg = serde_json::json!({
                        "type": "log",
                        "data": line
                    });
                    if ws_sender.send(Message::Text(msg.to_string())).await.is_err() {
                        break; // Client disconnected
                    }
                }
            });

            // Task 2: receive incoming commands from client WebSocket and inject into stdin
            let recv_task = tokio::spawn(async move {
                while let Some(Ok(msg)) = ws_receiver.next().await {
                    match msg {
                        Message::Text(txt) => {
                            let command_str = if let Ok(val) = serde_json::from_str::<serde_json::Value>(&txt) {
                                val["command"].as_str().unwrap_or(&txt).to_string()
                            } else {
                                txt
                            };
                            let _ = supervisor.send_input(&s_path, &command_str).await;
                        }
                        Message::Ping(_p) => {
                            // Tungstenite automatically responds to Pings
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
            });

            tokio::select! {
                _ = send_task => {},
                _ = recv_task => {},
            }

            info!("WebSocket console client disconnected for server '{}'", server_name);
            return;
        }

        // Unknown route -> 404
        let resp = "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n404 Not Found\n";
        let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_token_logic() {
        assert!(verify_token(None, None));
        assert!(verify_token(Some("any"), None));
        assert!(!verify_token(None, Some("secret123")));
        assert!(!verify_token(Some("wrong"), Some("secret123")));
        assert!(verify_token(Some("secret123"), Some("secret123")));
    }

    #[test]
    fn test_rate_limiter_window() {
        let mut limiter = SlidingWindowRateLimiter::new();
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        for _ in 0..5 {
            assert!(limiter.check_and_record(ip, 5));
        }
        // 6th attempt should be rejected
        assert!(!limiter.check_and_record(ip, 5));

        // Another IP should pass
        let ip2: IpAddr = "192.168.1.1".parse().unwrap();
        assert!(limiter.check_and_record(ip2, 5));
    }
}
