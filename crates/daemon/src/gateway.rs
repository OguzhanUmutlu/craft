use crate::supervisor::Supervisor;
use crate::telemetry::generate_prometheus_metrics;
use crate::web_dashboard::DASHBOARD_HTML;
use craft_core::{
    AuditLedger, AutoscaleRegistry, CraftPaths, Permission, RbacRegistry, Result, Role,
    ServersRegistry, UserAccount,
};
use futures_util::{SinkExt, StreamExt};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Cursor;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
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
    sessions: Arc<Mutex<HashMap<String, (UserAccount, Instant)>>>,
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
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn audit_secret(&self) -> Vec<u8> {
        self.token
            .as_ref()
            .map(|t| t.as_bytes().to_vec())
            .unwrap_or_else(|| b"craft-audit-master-hmac-key".to_vec())
    }

    pub fn start(&self) -> Result<tokio::task::JoinHandle<()>> {
        let server = self.clone();
        let addr = format!("{}:{}", self.bind, self.port);

        let handle = tokio::spawn(async move {
            let listener = match TcpListener::bind(&addr).await {
                Ok(l) => {
                    info!("Gateway & Web Dashboard server listening on http://{}", addr);
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

    async fn authenticate_request(&self, auth_token: Option<&str>) -> Option<UserAccount> {
        let tok = auth_token?;
        let mut sessions = self.sessions.lock().await;

        if let Some((user, created_at)) = sessions.get(tok) {
            // 24 hour session expiration
            if created_at.elapsed() < Duration::from_secs(86400) {
                return Some(user.clone());
            } else {
                sessions.remove(tok);
            }
        }

        // Fallback: Check if token matches gateway master token
        if let Some(ref master_tok) = self.token {
            if verify_token(Some(tok), Some(master_tok)) {
                return Some(UserAccount::new(
                    "gateway-admin",
                    "",
                    Role::SuperAdmin,
                    None,
                ));
            }
        }

        None
    }

    async fn handle_connection(&self, mut stream: TcpStream, peer: SocketAddr) {
        // Enforce basic rate limit on incoming connections
        {
            let mut limiter = self.rate_limiter.lock().await;
            if !limiter.check_and_record(peer.ip(), 150) {
                let resp = "HTTP/1.1 429 Too Many Requests\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nRate limit exceeded. Try again later.\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }
        }

        // Read initial HTTP request header
        let mut buffer = Vec::with_capacity(2048);
        let mut temp = [0u8; 1024];

        loop {
            match stream.read(&mut temp).await {
                Ok(0) => return,
                Ok(n) => {
                    buffer.extend_from_slice(&temp[..n]);
                    if buffer.windows(4).any(|w| w == b"\r\n\r\n") || buffer.len() > 16384 {
                        break;
                    }
                }
                Err(_) => return,
            }
        }

        let header_end_idx = match buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            Some(i) => i,
            None => return,
        };

        let header_bytes = &buffer[..header_end_idx];
        let mut body_bytes = buffer[header_end_idx + 4..].to_vec();

        let header_str = String::from_utf8_lossy(header_bytes);
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

        // Extract Content-Length & Authorization header
        let mut content_length = 0usize;
        let mut auth_token = query_params.get("token").cloned();

        for line in lines {
            let lower = line.to_lowercase();
            if lower.starts_with("content-length:") {
                if let Ok(len) = line[15..].trim().parse::<usize>() {
                    content_length = len;
                }
            } else if lower.starts_with("authorization:") {
                let val = line[14..].trim();
                if val.to_lowercase().starts_with("bearer ") {
                    auth_token = Some(val[7..].trim().to_string());
                }
            }
        }

        // Read remaining request body if needed
        while body_bytes.len() < content_length && body_bytes.len() < 65536 {
            let mut chunk = vec![0u8; (content_length - body_bytes.len()).min(4096)];
            match stream.read(&mut chunk).await {
                Ok(0) => break,
                Ok(n) => body_bytes.extend_from_slice(&chunk[..n]),
                Err(_) => break,
            }
        }

        // Route: GET /metrics
        if method == "GET" && path == "/metrics" {
            if !verify_token(auth_token.as_deref(), self.token.as_deref()) {
                let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n401 Unauthorized: Valid gateway token required.\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }

            let metrics = generate_prometheus_metrics(&self.supervisor, &self.paths).await;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                metrics.len(),
                metrics
            );
            let _ = stream.write_all(resp.as_bytes()).await;
            return;
        }

        // Route: GET /health
        if method == "GET" && path == "/health" {
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
            let _ = stream.write_all(resp.as_bytes()).await;
            return;
        }

        // Route: GET / or GET /dashboard -> Embedded Single-Page Web Dashboard
        if method == "GET" && (path == "/" || path == "/dashboard") {
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                DASHBOARD_HTML.len(),
                DASHBOARD_HTML
            );
            let _ = stream.write_all(resp.as_bytes()).await;
            return;
        }

        // Route: POST /api/auth/login
        if method == "POST" && path == "/api/auth/login" {
            let body_str = String::from_utf8_lossy(&body_bytes);
            let credentials: serde_json::Value = match serde_json::from_str(&body_str) {
                Ok(v) => v,
                Err(_) => {
                    let resp = "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Invalid JSON\"}\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            let username = credentials["username"].as_str().unwrap_or("").trim();
            let password = credentials["password"].as_str().unwrap_or("").trim();

            let rbac_reg = RbacRegistry::load_or_init(&self.paths).unwrap_or_default();
            let audit_secret = self.audit_secret();

            if let Some(user) = rbac_reg.authenticate(username, password) {
                // Generate secure random session token
                let mut hasher = Sha256::new();
                hasher.update(username.as_bytes());
                hasher.update(chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0).to_le_bytes());
                let session_token = hex::encode(hasher.finalize());

                {
                    let mut sessions = self.sessions.lock().await;
                    sessions.insert(session_token.clone(), (user.clone(), Instant::now()));
                }

                let _ = AuditLedger::append(
                    &self.paths,
                    &session_token[..8],
                    username,
                    "auth.login",
                    None,
                    Some(peer.ip().to_string()),
                    "SUCCESS",
                    None,
                    &audit_secret,
                );

                let resp_json = serde_json::json!({
                    "token": session_token,
                    "user": {
                        "username": user.username,
                        "role": user.role.name(),
                    }
                });

                let body = resp_json.to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            } else {
                let _ = AuditLedger::append(
                    &self.paths,
                    "none",
                    username,
                    "auth.login",
                    None,
                    Some(peer.ip().to_string()),
                    "DENIED",
                    Some("Invalid credentials".to_string()),
                    &audit_secret,
                );

                let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Invalid username or password\"}\n";
                let _ = stream.write_all(resp.as_bytes()).await;
            }
            return;
        }

        // Route: POST /api/auth/logout
        if method == "POST" && path == "/api/auth/logout" {
            if let Some(ref tok) = auth_token {
                let mut sessions = self.sessions.lock().await;
                if let Some((user, _)) = sessions.remove(tok) {
                    let audit_secret = self.audit_secret();
                    let _ = AuditLedger::append(
                        &self.paths,
                        &tok[..8.min(tok.len())],
                        &user.username,
                        "auth.logout",
                        None,
                        Some(peer.ip().to_string()),
                        "SUCCESS",
                        None,
                        &audit_secret,
                    );
                }
            }
            let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"success\":true}\n";
            let _ = stream.write_all(resp.as_bytes()).await;
            return;
        }

        // Route: GET /api/servers
        if method == "GET" && path == "/api/servers" {
            let user = match self.authenticate_request(auth_token.as_deref()).await {
                Some(u) => u,
                None => {
                    let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Authentication required\"}\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            let servers_reg = ServersRegistry::load(&self.paths).unwrap_or_default();
            let autoscale_reg = AutoscaleRegistry::load(&self.paths).unwrap_or_default();

            let mut server_items = Vec::new();
            for srv in servers_reg.servers {
                if !user.can_access_server(&srv.name) {
                    continue;
                }

                let is_running = self.supervisor.is_running(&srv.path).await;
                let policy = autoscale_reg.get_policy(&srv.name);
                let is_sleeping = !is_running && policy.map(|p| p.hibernation_enabled).unwrap_or(false);

                server_items.push(serde_json::json!({
                    "name": srv.name,
                    "software": srv.software,
                    "version": srv.version,
                    "port": srv.port.unwrap_or(25565),
                    "is_running": is_running,
                    "is_sleeping": is_sleeping,
                }));
            }

            let resp_json = serde_json::json!({
                "servers": server_items,
                "uptime_seconds": crate::telemetry::get_daemon_uptime_seconds(),
            });

            let body = resp_json.to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes()).await;
            return;
        }

        // Route: POST /api/servers/:name/:action
        if method == "POST" && path.starts_with("/api/servers/") {
            let user = match self.authenticate_request(auth_token.as_deref()).await {
                Some(u) => u,
                None => {
                    let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Authentication required\"}\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            let subpath = path.trim_start_matches("/api/servers/");
            let parts: Vec<&str> = subpath.split('/').collect();
            if parts.len() != 2 {
                let resp = "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Invalid endpoint format\"}\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }

            let server_name = parts[0];
            let action = parts[1];

            let perm = match action {
                "start" | "wake" => Permission::ServerStart,
                "stop" | "hibernate" => Permission::ServerStop,
                "restart" => Permission::ServerRestart,
                _ => {
                    let resp = "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Unknown action\"}\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            let audit_secret = self.audit_secret();

            if !user.has_permission(perm, Some(server_name)) {
                let _ = AuditLedger::append(
                    &self.paths,
                    "session",
                    &user.username,
                    &format!("server.{}", action),
                    Some(server_name.to_string()),
                    Some(peer.ip().to_string()),
                    "DENIED",
                    Some("Permission denied".to_string()),
                    &audit_secret,
                );
                let resp = "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Forbidden: Insufficient role permissions\"}\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }

            let server_path = match self.paths.resolve_server_path(None, Some(server_name), true) {
                Ok(p) => p,
                Err(e) => {
                    let resp = format!("HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{{\"error\":\"Server not found: {}\"}}\n", e);
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            let res = match action {
                "start" | "wake" => self.supervisor.start_server(&server_path).await,
                "stop" | "hibernate" => self.supervisor.stop_server(&server_path, false).await,
                "restart" => {
                    let _ = self.supervisor.stop_server(&server_path, false).await;
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    self.supervisor.start_server(&server_path).await
                }
                _ => Ok(()),
            };

            match res {
                Ok(()) => {
                    let _ = AuditLedger::append(
                        &self.paths,
                        "session",
                        &user.username,
                        &format!("server.{}", action),
                        Some(server_name.to_string()),
                        Some(peer.ip().to_string()),
                        "SUCCESS",
                        None,
                        &audit_secret,
                    );
                    let resp_json = serde_json::json!({
                        "success": true,
                        "server": server_name,
                        "action": action,
                    });
                    let body = resp_json.to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                }
                Err(e) => {
                    let _ = AuditLedger::append(
                        &self.paths,
                        "session",
                        &user.username,
                        &format!("server.{}", action),
                        Some(server_name.to_string()),
                        Some(peer.ip().to_string()),
                        "FAILED",
                        Some(e.to_string()),
                        &audit_secret,
                    );
                    let resp_json = serde_json::json!({
                        "success": false,
                        "error": e.to_string(),
                    });
                    let body = resp_json.to_string();
                    let resp = format!(
                        "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                }
            }
            return;
        }

        // Route: GET /api/audit
        if method == "GET" && path == "/api/audit" {
            let user = match self.authenticate_request(auth_token.as_deref()).await {
                Some(u) => u,
                None => {
                    let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Authentication required\"}\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            if !user.has_permission(Permission::AuditLogView, None) {
                let resp = "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Forbidden: Insufficient permissions\"}\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }

            let entries = AuditLedger::read_entries(&self.paths, Some(100)).unwrap_or_default();
            let resp_json = serde_json::json!({ "entries": entries });
            let body = resp_json.to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes()).await;
            return;
        }

        // Route: GET /api/audit/verify
        if method == "GET" && path == "/api/audit/verify" {
            let user = match self.authenticate_request(auth_token.as_deref()).await {
                Some(u) => u,
                None => {
                    let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Authentication required\"}\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            if !user.has_permission(Permission::AuditLogView, None) {
                let resp = "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Forbidden\"}\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }

            let result = AuditLedger::verify_chain(&self.paths, &self.audit_secret()).unwrap_or(
                craft_core::AuditVerificationResult {
                    is_valid: false,
                    total_entries: 0,
                    verified_entries: 0,
                    corrupted_index: None,
                    error_message: Some("Verification execution failure".to_string()),
                },
            );

            let body = serde_json::to_string(&result).unwrap_or_default();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes()).await;
            return;
        }

        // Route: GET /api/rbac/users
        if method == "GET" && path == "/api/rbac/users" {
            let user = match self.authenticate_request(auth_token.as_deref()).await {
                Some(u) => u,
                None => {
                    let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Authentication required\"}\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            if !user.has_permission(Permission::UserManage, None) {
                let resp = "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Forbidden: Requires UserManage permission\"}\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }

            let reg = RbacRegistry::load_or_init(&self.paths).unwrap_or_default();
            let scrubbed_users: Vec<serde_json::Value> = reg
                .users
                .iter()
                .map(|u| {
                    serde_json::json!({
                        "username": u.username,
                        "role": u.role.name(),
                        "assigned_servers": u.assigned_servers,
                        "disabled": u.disabled,
                        "created_at": u.created_at.to_rfc3339(),
                    })
                })
                .collect();

            let resp_json = serde_json::json!({ "users": scrubbed_users });
            let body = resp_json.to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes()).await;
            return;
        }

        // Route: GET /api/ai/status
        if method == "GET" && path == "/api/ai/status" {
            let user = match self.authenticate_request(auth_token.as_deref()).await {
                Some(u) => u,
                None => {
                    let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Authentication required\"}\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            if !user.has_permission(Permission::ServerConsoleView, None) {
                let resp = "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Forbidden: Requires ServerConsoleView permission\"}\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }

            let reg = ServersRegistry::load(&self.paths).unwrap_or_default();
            let int_reg = craft_core::IntelligenceRegistry::load(&self.paths).unwrap_or_default();
            let running = self.supervisor.get_running_paths().await;

            let mut server_statuses = Vec::new();
            for s in reg.servers {
                if !user.can_access_server(&s.name) {
                    continue;
                }
                let canonical = s.path.canonicalize().unwrap_or_else(|_| s.path.clone());
                let is_running = running.iter().any(|p| {
                    p.canonicalize().unwrap_or_else(|_| p.clone()) == canonical
                });
                let policy = int_reg.get_policy(&s.name);
                server_statuses.push(serde_json::json!({
                    "server": s.name,
                    "running": is_running,
                    "policy_mode": policy.mode.to_string(),
                    "warning_mspt": policy.mspt_warning_ms,
                    "critical_mspt": policy.mspt_critical_ms,
                    "memory_leak_threshold_mb_min": policy.memory_leak_slope_mb_min,
                }));
            }

            let resp_json = serde_json::json!({ "servers": server_statuses });
            let body = resp_json.to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes()).await;
            return;
        }

        // Route: GET /api/ai/diagnostics/:server
        if method == "GET" && path.starts_with("/api/ai/diagnostics/") {
            let server_name = path.trim_start_matches("/api/ai/diagnostics/");
            let user = match self.authenticate_request(auth_token.as_deref()).await {
                Some(u) => u,
                None => {
                    let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Authentication required\"}\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            if !user.can_access_server(server_name) || !user.has_permission(Permission::ServerConsoleView, Some(server_name)) {
                let resp = "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Forbidden: Server access restricted\"}\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }

            let diag_dir = self.paths.diagnostics_dir.join(server_name);
            let mut reports = Vec::new();
            if diag_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&diag_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                            reports.push(serde_json::json!({
                                "filename": name,
                                "size_bytes": size,
                            }));
                        }
                    }
                }
            }

            let resp_json = serde_json::json!({ "server": server_name, "reports": reports });
            let body = resp_json.to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes()).await;
            return;
        }

        // Route: POST /api/ai/remediate/:server
        if method == "POST" && path.starts_with("/api/ai/remediate/") {
            let server_name = path.trim_start_matches("/api/ai/remediate/");
            let user = match self.authenticate_request(auth_token.as_deref()).await {
                Some(u) => u,
                None => {
                    let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Authentication required\"}\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            if !user.can_access_server(server_name) || !user.has_permission(Permission::ServerConsoleInput, Some(server_name)) {
                let resp = "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Forbidden: Requires ServerConsoleInput permission\"}\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }

            let reg = ServersRegistry::load(&self.paths).unwrap_or_default();
            let s_opt = reg.find_by_name(server_name);
            if let Some(srv) = s_opt {
                let canonical = srv.path.canonicalize().unwrap_or_else(|_| srv.path.clone());
                // Ingest simple command execution
                let _ = self.supervisor.send_input(&canonical, "/kill @e[type=item]\n").await;
                let body = serde_json::json!({ "success": true, "message": "Triggered remediation command" }).to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            } else {
                let resp = "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Server not found\"}\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }
        }

        // Route: GET /ws/console
        if method == "GET" && path == "/ws/console" {
            let user = match self.authenticate_request(auth_token.as_deref()).await {
                Some(u) => u,
                None => {
                    let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n401 Unauthorized: Valid gateway session or token required.\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            let server_name = match query_params.get("server") {
                Some(s) => s.clone(),
                None => {
                    let resp = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nMissing required 'server' query parameter.\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            if !user.has_permission(Permission::ServerConsoleView, Some(&server_name)) {
                let resp = "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n403 Forbidden: Insufficient role permissions for this server console.\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }

            let server_path = match self.paths.resolve_server_path(None, Some(&server_name), true) {
                Ok(p) => p,
                Err(e) => {
                    let resp = format!("HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nServer not found: {}\n", e);
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            if !self.supervisor.is_running(&server_path).await {
                let resp = format!("HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nServer '{}' is not currently running.\n", server_name);
                let _ = stream.write_all(resp.as_bytes()).await;
                return;
            }

            let mut log_rx = match self.supervisor.get_server_log_broadcaster(&server_path).await {
                Some(rx) => rx,
                None => {
                    let resp = "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nFailed to acquire server log broadcaster.\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            // Upgrade to WebSocket
            let mut prefix_buf = buffer[..header_end_idx + 4].to_vec();
            prefix_buf.extend_from_slice(&body_bytes);
            let prefixed = PrefixedStream::new(prefix_buf, stream);

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

            info!("WebSocket console client connected for server '{}' (user: {}) from {}", server_name, user.username, peer);

            let (mut ws_sender, mut ws_receiver) = ws_stream.split();
            let supervisor = self.supervisor.clone();
            let s_path = server_path.clone();
            let s_name = server_name.clone();
            let u_clone = user.clone();
            let p_clone = self.paths.clone();
            let a_sec = self.audit_secret();

            // Task 1: forward incoming server log lines to client WebSocket
            let send_task = tokio::spawn(async move {
                while let Ok(line) = log_rx.recv().await {
                    let msg = serde_json::json!({
                        "type": "log",
                        "data": line
                    });
                    if ws_sender.send(Message::Text(msg.to_string())).await.is_err() {
                        break;
                    }
                }
            });

            // Task 2: receive incoming commands from client WebSocket and inject into stdin
            let recv_task = tokio::spawn(async move {
                while let Some(Ok(msg)) = ws_receiver.next().await {
                    match msg {
                        Message::Text(txt) => {
                            if !u_clone.has_permission(Permission::ServerConsoleInput, Some(&s_name)) {
                                continue;
                            }
                            let command_str = if let Ok(val) = serde_json::from_str::<serde_json::Value>(&txt) {
                                val["command"].as_str().unwrap_or(&txt).to_string()
                            } else {
                                txt
                            };
                            let _ = AuditLedger::append(
                                &p_clone,
                                "ws",
                                &u_clone.username,
                                "console.command",
                                Some(s_name.clone()),
                                Some(peer.ip().to_string()),
                                "SUCCESS",
                                Some(command_str.clone()),
                                &a_sec,
                            );
                            let _ = supervisor.send_input(&s_path, &command_str).await;
                        }
                        Message::Ping(_) => {}
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
        let _ = stream.write_all(resp.as_bytes()).await;
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
