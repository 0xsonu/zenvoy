use std::sync::Arc;

use axum::{extract::Request, extract::State, middleware::Next, response::Response};
use http::header::HeaderValue;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

use super::AppState;
use crate::config::Config;

pub async fn security_headers(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let behind_tls = state.config.read().behind_tls;
    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("x-xss-protection", HeaderValue::from_static("0"));
    headers.insert(
        "referrer-policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    if behind_tls {
        headers.insert(
            "strict-transport-security",
            HeaderValue::from_static("max-age=63072000; includeSubDomains"),
        );
    }
    res
}

pub fn build_cors(config: &Config) -> CorsLayer {
    use http::Method;

    let methods = vec![
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::OPTIONS,
    ];

    let layer = CorsLayer::new()
        .allow_methods(methods)
        .allow_headers([http::header::CONTENT_TYPE, http::header::AUTHORIZATION])
        .allow_credentials(true);

    if config.allowed_origins.is_empty() {
        layer.allow_origin(Any).allow_credentials(false)
    } else {
        let origins: Vec<HeaderValue> = config
            .allowed_origins
            .iter()
            .filter_map(|o| o.parse::<HeaderValue>().ok())
            .collect();
        layer.allow_origin(AllowOrigin::list(origins))
    }
}
