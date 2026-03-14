use axum::{extract::Request, middleware::Next, response::Response};

/// Placeholder auth middleware — will be implemented in the next task.
pub async fn require_auth(req: Request, next: Next) -> Response {
    next.run(req).await
}
