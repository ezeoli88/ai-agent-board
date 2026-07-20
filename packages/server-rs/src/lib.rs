pub mod agent;
pub mod config;
pub mod db;
pub mod error;
pub mod mcp;
pub mod models;
pub mod routes;
pub mod services;
pub mod utils;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderValue, Method, Request, StatusCode},
    middleware,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde_json::json;
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::db::Database;
use crate::services::agent_service::AgentService;
use crate::services::git_service::GitService;
use crate::utils::{DataEventEmitter, SSEEmitter};

// Re-export Args from the binary crate via a local struct
// (main.rs defines the actual CLI args, lib.rs accepts them)

/// Arguments passed from the binary entry point.
pub struct RunArgs {
    pub port: Option<u16>,
    pub database_path: Option<String>,
    pub no_open: bool,
}

/// Shared application state passed to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub config: Arc<Config>,
    pub auth_token: Option<String>,
    pub startup_id: String,
    pub sse_emitter: Arc<SSEEmitter>,
    pub data_emitter: Arc<DataEventEmitter>,
    pub agent_service: Arc<AgentService>,
    pub git_service: Arc<GitService>,
}

/// Initializes logging with tracing-subscriber.
fn init_logging(log_level: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}

/// Generates a random startup authentication token.
fn generate_startup_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    hex::encode(bytes)
}

/// Returns the first non-internal IPv4 address (LAN IP).
fn get_lan_ip() -> String {
    use std::net::UdpSocket;
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "localhost".to_string()
}

/// Opens the default browser with the given URL.
fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

/// Checks if an origin is allowed for CORS (localhost + RFC 1918 private IPs).
fn is_allowed_origin(origin: &str) -> bool {
    let Ok(url) = url::Url::parse(origin) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };

    if host == "localhost" || host == "127.0.0.1" || host == "[::1]" {
        return true;
    }

    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        let octets = ip.octets();
        if octets[0] == 10 {
            return true;
        }
        if octets[0] == 172 && (16..=31).contains(&octets[1]) {
            return true;
        }
        if octets[0] == 192 && octets[1] == 168 {
            return true;
        }
    }

    false
}

/// Auth middleware — checks Bearer token or ?token= query param.
async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    let Some(ref expected_token) = state.auth_token else {
        return next.run(req).await;
    };

    // Loopback bypass for /api/mcp
    let is_mcp = req.uri().path().starts_with("/api/mcp");
    if is_mcp {
        if let Some(addr) = req
            .extensions()
            .get::<axum::extract::ConnectInfo<SocketAddr>>()
        {
            if addr.ip().is_loopback() {
                return next.run(req).await;
            }
        }
    }

    // Check Authorization: Bearer <token>
    if let Some(auth_header) = req.headers().get("authorization") {
        if let Ok(value) = auth_header.to_str() {
            if let Some(token) = value.strip_prefix("Bearer ") {
                if token == expected_token {
                    return next.run(req).await;
                }
            }
        }
    }

    // Check ?token= query param (for EventSource/SSE)
    if let Some(query) = req.uri().query() {
        for pair in query.split('&') {
            if let Some(token) = pair.strip_prefix("token=") {
                if token == expected_token {
                    return next.run(req).await;
                }
            }
        }
    }

    warn!(path = %req.uri().path(), "Unauthorized request");
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "Unauthorized", "message": "Valid authentication token required"})),
    )
        .into_response()
}

/// Middleware that adds no-cache headers and server ID to API responses.
async fn api_headers_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(
        "Cache-Control",
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Expires", HeaderValue::from_static("0"));
    if let Ok(val) = HeaderValue::from_str(&state.startup_id) {
        headers.insert("X-Server-ID", val);
    }
    response
}

/// Health check handler.
async fn health_handler() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

/// Builds the CORS layer allowing localhost and RFC 1918 private IPs.
fn cors_layer() -> tower_http::cors::CorsLayer {
    tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(
            |origin: &HeaderValue, _req: &axum::http::request::Parts| {
                origin
                    .to_str()
                    .map(|o| is_allowed_origin(o))
                    .unwrap_or(false)
            },
        ))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(tower_http::cors::Any)
        .allow_credentials(false)
}

/// Builds the application router.
fn build_router(state: AppState) -> Router {
    let mut api = Router::new()
        .route("/api/health", get(health_handler))
        .merge(routes::api_router())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            api_headers_middleware,
        ));

    if state.auth_token.is_some() {
        api = api.layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));
    }

    // CORS must be applied before auth so preflight OPTIONS requests pass
    api = api.layer(cors_layer());

    // Static file serving for frontend
    let public_dir = find_public_dir(state.config.is_binary_mode);

    let app = if let Some(ref dir) = public_dir {
        // SPA fallback: serve index.html for any path that doesn't match a real file
        let index_html = dir.join("index.html");
        api.fallback_service(
            ServeDir::new(dir)
                .append_index_html_on_directories(true)
                .not_found_service(ServeFile::new(index_html)),
        )
    } else {
        api.fallback(|| async {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Not Found", "message": "The requested resource does not exist"})),
            )
        })
    };

    app.with_state(state)
}

fn find_public_dir(is_binary_mode: bool) -> Option<std::path::PathBuf> {
    if is_binary_mode {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("public")))
            .filter(|p| p.exists())
    } else {
        std::env::current_dir()
            .ok()
            .map(|d| d.join("public"))
            .filter(|p| p.exists())
    }
}

/// Tries to find an available port starting from the given port.
async fn find_available_port(start: u16, max_attempts: u16) -> anyhow::Result<u16> {
    for i in 0..max_attempts {
        let port = start + i;
        let addr: SocketAddr = ([0, 0, 0, 0], port).into();
        match TcpListener::bind(addr).await {
            Ok(_) => return Ok(port),
            Err(_) => {
                info!("Port {port} is in use, trying next...");
            }
        }
    }
    anyhow::bail!(
        "No available port found in range {start}-{}",
        start + max_attempts - 1
    );
}

/// Main application entry point — called from the binary's main().
pub async fn run(args: impl Into<RunArgs>) -> anyhow::Result<()> {
    let args = args.into();
    let config = Config::load(args.port, args.database_path.as_deref());

    init_logging(&config.log_level);

    info!("Starting Agent Board API");
    info!(
        port = config.port,
        database_path = %config.database_path.display(),
        repos_base_dir = %config.repos_base_dir.display(),
        worktrees_dir = %config.worktrees_dir.display(),
        log_level = %config.log_level,
        "Configuration loaded"
    );

    config.ensure_database_dir()?;

    // Initialize encryption key (persisted alongside the database)
    let key_dir = config
        .database_path
        .parent()
        .expect("database_path must have a parent directory");
    services::encryption_service::init_encryption_key(key_dir);

    // Open database and run migrations
    let db = Database::open(&config.database_path)?;
    db.call(|conn| {
        crate::db::migrations::run_migrations(conn)?;
        Ok(())
    })
    .await?;

    // Auth token
    let auth_token = if config.auth_enabled {
        let token = generate_startup_token();
        info!("Authentication enabled");
        Some(token)
    } else {
        info!("Authentication disabled (dev mode)");
        None
    };

    let startup_id = uuid::Uuid::new_v4().to_string();
    let config = Arc::new(config);

    let (sse_emitter, persist_handle) = SSEEmitter::with_persistence(db.clone());
    let sse_emitter = Arc::new(sse_emitter);
    let data_emitter = Arc::new(DataEventEmitter::new());
    let mut agent_svc = AgentService::new(Arc::clone(&sse_emitter), Arc::clone(&data_emitter));
    agent_svc.set_db(db.clone());
    let agent_service = Arc::new(agent_svc);
    let git_service = Arc::new(GitService::new(Arc::clone(&config)));

    let state = AppState {
        db,
        config: Arc::clone(&config),
        auth_token: auth_token.clone(),
        startup_id,
        sse_emitter,
        data_emitter,
        agent_service,
        git_service,
    };

    let app = build_router(state);

    // Find available port
    let port = find_available_port(config.port, 10).await?;
    if port != config.port {
        info!(
            "Default port {} was in use, using port {} instead",
            config.port, port
        );
    }

    let addr: SocketAddr = ([0, 0, 0, 0], port).into();
    let listener = TcpListener::bind(addr).await?;

    let lan_ip = get_lan_ip();
    let token_param = auth_token
        .as_ref()
        .map(|t| format!("?token={t}"))
        .unwrap_or_default();
    let local_url = format!("http://localhost:{port}{token_param}");
    let lan_url = format!("http://{lan_ip}:{port}{token_param}");

    println!();
    println!("  ┌──────────────────────────────────────────┐");
    println!("  │          agent-board is running           │");
    println!("  └──────────────────────────────────────────┘");
    println!();
    println!("  Local:   {local_url}");
    println!("  Network: {lan_url}");
    if auth_token.is_some() {
        println!("  Auth:    Enabled (token in URL)");
    } else {
        println!("  Auth:    DISABLED");
    }
    println!();
    println!("  Press Ctrl+C to stop");
    println!();

    info!("Server listening on {addr}");

    if !args.no_open {
        open_browser(&lan_url);
    }

    let serve_result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
        info!("Received shutdown signal, shutting down gracefully");
    })
    .await;

    if let Err(e) = serve_result {
        // Don't treat shutdown errors as fatal — this commonly happens on
        // Windows when the terminal is closed or Ctrl+C interrupts active connections.
        warn!("Server stopped with error: {e}");
    } else {
        info!("Server stopped");
    }

    // After serve completes, all AppState clones (including SSEEmitter Arcs) are dropped.
    // The persist_tx channel closes, causing the background loop to flush and exit.
    info!("Waiting for SSE event persistence to flush...");
    let _ = persist_handle.await;
    info!("SSE event persistence flushed");

    Ok(())
}
