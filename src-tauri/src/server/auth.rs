use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use parking_lot::Mutex;
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};
use subtle::ConstantTimeEq;

use super::AppState;

// --- SessionStore ---

pub struct SessionStore {
    sessions: Mutex<HashMap<String, Instant>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn create(&self) -> (String, Instant) {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let token = hex::encode(bytes);
        let expires = Instant::now() + Duration::from_secs(30 * 24 * 60 * 60);
        self.sessions.lock().insert(token.clone(), expires);
        (token, expires)
    }

    pub fn is_valid(&self, token: &str) -> bool {
        let mut sessions = self.sessions.lock();
        let now = Instant::now();
        sessions.retain(|_, exp| *exp > now);
        sessions.get(token).map_or(false, |exp| *exp > now)
    }

    pub fn delete(&self, token: &str) {
        self.sessions.lock().remove(token);
    }

    pub fn delete_all(&self) {
        self.sessions.lock().clear();
    }
}

// --- AttemptLimiter ---

pub struct AttemptLimiter {
    window: Duration,
    max_hits: usize,
    hits: Mutex<HashMap<String, Vec<Instant>>>,
}

impl AttemptLimiter {
    pub fn new(window: Duration, max_hits: usize) -> Self {
        Self {
            window,
            max_hits,
            hits: Mutex::new(HashMap::new()),
        }
    }

    pub fn allow(&self, key: &str) -> bool {
        let mut hits = self.hits.lock();
        let now = Instant::now();
        let cutoff = now - self.window;
        let entry = hits.entry(key.to_string()).or_default();
        entry.retain(|t| *t > cutoff);
        if entry.len() >= self.max_hits {
            return false;
        }
        entry.push(now);
        true
    }
}

// --- Auth Middleware ---

pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let auth_token = state.config.read().auth_token.clone();

    // No auth required if token is empty
    if auth_token.is_empty() {
        return next.run(req).await;
    }

    // Check Authorization: Bearer <token>
    if let Some(header) = req.headers().get("authorization") {
        if let Ok(val) = header.to_str() {
            if let Some(token) = val.strip_prefix("Bearer ") {
                if constant_time_eq(&auth_token, token) {
                    return next.run(req).await;
                }
            }
        }
    }

    // Check session cookie
    if let Some(cookie_header) = req.headers().get("cookie") {
        if let Ok(cookies) = cookie_header.to_str() {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(val) = cookie.strip_prefix("zenvoy_session=") {
                    if state.sessions.is_valid(val) {
                        return next.run(req).await;
                    }
                }
            }
        }
    }

    (StatusCode::UNAUTHORIZED, Json(json!({"error": "Unauthorized"}))).into_response()
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

// --- Session Routes ---

pub fn session_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/session", get(session_status))
        .route("/session/login", post(session_login))
        .route("/session/logout", post(session_logout))
        .route("/session/rotate-token", post(session_rotate_token))
        .with_state(state)
}

async fn session_status(State(state): State<Arc<AppState>>) -> Json<Value> {
    let config = state.config.read();
    let requires_auth = !config.auth_token.is_empty();
    Json(json!({
        "authenticated": !requires_auth,
        "requiresAuth": requires_auth,
    }))
}

#[derive(Deserialize)]
struct LoginBody {
    token: String,
}

async fn session_login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(body): Json<LoginBody>,
) -> Response {
    let ip = addr.ip().to_string();

    if !state.limiter.allow(&ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "Too many attempts. Try again later."})),
        )
            .into_response();
    }

    let auth_token = state.config.read().auth_token.clone();

    if auth_token.is_empty() || !constant_time_eq(&auth_token, &body.token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid token"})),
        )
            .into_response();
    }

    let (session_token, _) = state.sessions.create();
    let cookie = format!(
        "zenvoy_session={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        session_token,
        30 * 24 * 60 * 60
    );

    (
        StatusCode::OK,
        [("set-cookie", cookie)],
        Json(json!({"authenticated": true})),
    )
        .into_response()
}

async fn session_logout(
    State(state): State<Arc<AppState>>,
    req: Request,
) -> Response {
    // Delete session from store if cookie present
    if let Some(cookie_header) = req.headers().get("cookie") {
        if let Ok(cookies) = cookie_header.to_str() {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(val) = cookie.strip_prefix("zenvoy_session=") {
                    state.sessions.delete(val);
                }
            }
        }
    }

    let cookie = "zenvoy_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0";
    (
        StatusCode::OK,
        [("set-cookie", cookie.to_string())],
        Json(json!({"authenticated": false})),
    )
        .into_response()
}

async fn session_rotate_token(State(state): State<Arc<AppState>>) -> Response {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let new_token = hex::encode(bytes);

    {
        let mut config = state.config.write();
        config.auth_token = new_token.clone();
        let _ = config.save();
    }

    state.sessions.delete_all();

    (
        StatusCode::OK,
        Json(json!({"token": new_token})),
    )
        .into_response()
}
